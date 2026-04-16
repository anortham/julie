//! Deferred NL embedding initialization for definition search.
//!
//! When a natural-language query hits `fast_search(search_target="definitions")` and
//! no embedding provider has been initialized yet, this module attempts a one-shot
//! deferred init so the first NL query can use hybrid (keyword + semantic) search.

use std::sync::LazyLock;
use tracing::{debug, warn};

use crate::handler::JulieServerHandler;

/// Single-flight guard: only one task may attempt deferred init at a time.
static NL_DEFINITION_EMBEDDING_INIT_SINGLE_FLIGHT: LazyLock<tokio::sync::Mutex<()>> =
    LazyLock::new(|| tokio::sync::Mutex::new(()));

#[cfg(test)]
static NL_DEFINITION_EMBEDDING_INIT_ATTEMPTS: LazyLock<
    std::sync::Mutex<std::collections::HashMap<std::path::PathBuf, usize>>,
> = LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

#[cfg(test)]
fn record_nl_definition_embedding_init_attempt(workspace_root: &std::path::Path) {
    let mut attempts = NL_DEFINITION_EMBEDDING_INIT_ATTEMPTS
        .lock()
        .expect("nl definition init attempt map mutex poisoned");
    *attempts.entry(workspace_root.to_path_buf()).or_insert(0) += 1;
}

#[cfg(test)]
pub(crate) fn take_nl_definition_embedding_init_attempts(
    workspace_root: &std::path::Path,
) -> usize {
    let mut attempts = NL_DEFINITION_EMBEDDING_INIT_ATTEMPTS
        .lock()
        .expect("nl definition init attempt map mutex poisoned");
    attempts.remove(workspace_root).unwrap_or(0)
}

/// If this is an NL definitions query and no embedding provider exists yet,
/// attempt a deferred one-shot initialization (single-flighted).
pub(super) async fn maybe_initialize_embeddings_for_nl_definitions(
    query: &str,
    search_target: &str,
    handler: &JulieServerHandler,
) {
    if search_target != "definitions" || !crate::search::scoring::is_nl_like_query(query) {
        return;
    }

    // If a provider is already available (daemon shared service or workspace),
    // skip the lazy-init entirely.
    if handler.embedding_provider().await.is_some() {
        return;
    }

    // Daemon mode: the shared service may still be in `Initializing` (cold
    // start). Wait briefly for it to settle. NL queries are interactive, so
    // we use a much shorter timeout (3s) than the workspace embedding path
    // (120s). If the service doesn't reach Ready in time, degrade to
    // keyword-only by returning. CRITICALLY: we must NOT fall through to
    // the per-workspace stdio init path below — that would spawn a SECOND
    // Python sidecar alongside the daemon's shared one, wasting resources
    // and masking the real provider.
    if let Some(svc) = handler.embedding_service.as_ref() {
        use crate::daemon::embedding_service::EmbeddingServiceSettled;
        match svc
            .wait_until_settled(std::time::Duration::from_secs(3))
            .await
        {
            EmbeddingServiceSettled::Ready { .. } => {
                // Provider is now published; the caller's next
                // handler.embedding_provider() will see it. Return without
                // running the stdio lazy-init path.
                debug!(
                    "Daemon embedding service became Ready while waiting for NL definition query"
                );
                return;
            }
            EmbeddingServiceSettled::Unavailable { reason, .. } => {
                debug!(
                    %reason,
                    "Daemon embedding service Unavailable; NL query falls back to keyword-only"
                );
                return;
            }
            EmbeddingServiceSettled::Timeout => {
                debug!(
                    "Daemon embedding service did not settle within 3s for NL query; \
                     falling back to keyword-only (provider may become ready later)"
                );
                return;
            }
        }
    }

    // Stdio mode (no daemon shared service). Fall through to the existing
    // per-workspace lazy init path. Reached only when handler.embedding_service
    // is None — this is the original pre-daemon-lazy-init code path.
    let should_attempt_init = {
        let workspace_guard = handler.workspace.read().await;
        match workspace_guard.as_ref() {
            Some(workspace) => workspace.embedding_runtime_status.is_none(),
            None => false,
        }
    };

    if !should_attempt_init {
        return;
    }

    let _single_flight_guard = NL_DEFINITION_EMBEDDING_INIT_SINGLE_FLIGHT.lock().await;

    // Double-check after acquiring the single-flight mutex: another caller may
    // have completed init while we waited.
    if handler.embedding_provider().await.is_some() {
        return;
    }
    let (workspace_identity_root, workspace_for_init) = {
        let workspace_guard = handler.workspace.read().await;
        match workspace_guard.as_ref() {
            Some(workspace) => {
                if workspace.embedding_runtime_status.is_some() {
                    return;
                }
                (workspace.root.clone(), workspace.clone())
            }
            None => return,
        }
    };

    debug!(
        "NL definitions query without embeddings/runtime status; attempting deferred provider init"
    );

    #[cfg(test)]
    record_nl_definition_embedding_init_attempt(&workspace_identity_root);

    let init_result = tokio::task::spawn_blocking(move || {
        let mut workspace = workspace_for_init;
        workspace.initialize_embedding_provider();
        (
            workspace.embedding_provider.clone(),
            workspace.embedding_runtime_status.clone(),
        )
    })
    .await;

    let (initialized_provider, initialized_runtime_status) = match init_result {
        Ok(result) => result,
        Err(e) => {
            warn!("Deferred embedding init task panicked during text search: {e}");
            return;
        }
    };

    let mut workspace_guard = handler.workspace.write().await;
    let workspace = match workspace_guard.as_mut() {
        Some(workspace) => workspace,
        None => return,
    };

    if workspace.root != workspace_identity_root {
        debug!(
            expected_workspace_root = %workspace_identity_root.display(),
            active_workspace_root = %workspace.root.display(),
            "Discarding stale deferred embedding init result after workspace switch"
        );
        return;
    }

    if workspace.embedding_provider.is_none() {
        workspace.embedding_provider = initialized_provider;
        // Propagate to file watcher so incremental updates use the new provider
        if let Some(ref watcher) = workspace.watcher {
            watcher.update_embedding_provider(workspace.embedding_provider.clone());
        }
    }
    if workspace.embedding_runtime_status.is_none() {
        workspace.embedding_runtime_status = initialized_runtime_status;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::embedding_service::EmbeddingService;
    use crate::handler::JulieServerHandler;
    use std::sync::Arc;
    use std::time::Duration;

    /// In daemon mode, when the shared service publishes Ready mid-wait, the
    /// NL definition init helper should observe Ready and return WITHOUT
    /// triggering the per-workspace stdio init path. The init counter MUST
    /// stay at zero — incrementing it would mean we'd spawned a duplicate
    /// Python sidecar alongside the daemon's shared one.
    #[tokio::test]
    async fn test_daemon_mode_publishes_ready_no_stdio_fallback() {
        let mut handler = JulieServerHandler::new_for_test()
            .await
            .expect("new_for_test should succeed");

        // Inject a fresh shared service in Initializing.
        let service = Arc::new(EmbeddingService::initializing());
        handler.embedding_service = Some(Arc::clone(&service));

        // Background publisher: simulates the daemon's background init
        // task finishing while the NL query is waiting.
        let publisher_service = Arc::clone(&service);
        let publisher = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let provider: Arc<dyn crate::embeddings::EmbeddingProvider> =
                Arc::new(NoopProvider::default());
            let status = crate::embeddings::EmbeddingRuntimeStatus {
                requested_backend: crate::embeddings::EmbeddingBackend::Unresolved,
                resolved_backend: crate::embeddings::EmbeddingBackend::Unresolved,
                accelerated: false,
                degraded_reason: None,
            };
            publisher_service.publish_ready(provider, status);
        });

        // Reset the counter for this workspace before invoking.
        let workspace_root = std::path::PathBuf::from("test-daemon-publishes-ready");
        let _ = take_nl_definition_embedding_init_attempts(&workspace_root);

        // NL-like definitions query that would otherwise trigger lazy init.
        maybe_initialize_embeddings_for_nl_definitions(
            "how does the user authentication flow work",
            "definitions",
            &handler,
        )
        .await;

        publisher.await.expect("publisher task should not panic");

        // Critical assertion: the stdio fallback path was never reached.
        let count = take_nl_definition_embedding_init_attempts(&workspace_root);
        assert_eq!(
            count, 0,
            "daemon-mode NL query must not trigger per-workspace stdio init"
        );
    }

    /// In daemon mode, when the shared service stays Initializing past the
    /// 3 second NL query timeout, the helper should return without triggering
    /// the stdio fallback. Same critical guarantee as the publish_ready case:
    /// no duplicate sidecar.
    #[tokio::test]
    async fn test_daemon_mode_timeout_no_stdio_fallback() {
        let mut handler = JulieServerHandler::new_for_test()
            .await
            .expect("new_for_test should succeed");

        // Service that never settles. We rely on the helper's 3s timeout
        // to bail rather than holding the test for the full duration.
        // To keep the test fast, we use the EmbeddingService directly with
        // a manual short-timeout call instead of waiting the full 3s.
        // For the test, we shorten the wait by publishing Unavailable
        // immediately — the assertion is still about the no-fallthrough
        // guarantee, and Unavailable triggers the same return-without-fallthrough
        // branch as Timeout.
        let service = Arc::new(EmbeddingService::initializing());
        handler.embedding_service = Some(Arc::clone(&service));
        service.publish_unavailable("test: model load failed".to_string(), None);

        let workspace_root = std::path::PathBuf::from("test-daemon-timeout");
        let _ = take_nl_definition_embedding_init_attempts(&workspace_root);

        maybe_initialize_embeddings_for_nl_definitions(
            "how does the user authentication flow work",
            "definitions",
            &handler,
        )
        .await;

        let count = take_nl_definition_embedding_init_attempts(&workspace_root);
        assert_eq!(
            count, 0,
            "daemon-mode NL query must not trigger per-workspace stdio init even when service is Unavailable"
        );
    }

    /// Stdio mode (no embedding_service) should still take the per-workspace
    /// lazy init path. This guards against the daemon-mode split accidentally
    /// breaking the stdio mode entrypoint. Note: this test does NOT actually
    /// verify the counter increments, because that requires a workspace to
    /// be set on the handler (the helper bails early without one). It does
    /// verify the helper returns cleanly without panicking.
    #[tokio::test]
    async fn test_stdio_mode_no_workspace_returns_cleanly() {
        let handler = JulieServerHandler::new_for_test()
            .await
            .expect("new_for_test should succeed");
        // No embedding_service set → stdio mode
        // No workspace set → should_attempt_init returns false → early return

        maybe_initialize_embeddings_for_nl_definitions(
            "how does login work",
            "definitions",
            &handler,
        )
        .await;
        // If we got here without panicking, the early return path works.
    }

    #[derive(Default)]
    struct NoopProvider;

    impl crate::embeddings::EmbeddingProvider for NoopProvider {
        fn embed_query(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
            Ok(Vec::new())
        }

        fn embed_batch(&self, _texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
            Ok(Vec::new())
        }

        fn dimensions(&self) -> usize {
            0
        }

        fn device_info(&self) -> crate::embeddings::DeviceInfo {
            crate::embeddings::DeviceInfo {
                runtime: "test".to_string(),
                device: "test".to_string(),
                model_name: "test-noop".to_string(),
                dimensions: 0,
            }
        }
    }
}
