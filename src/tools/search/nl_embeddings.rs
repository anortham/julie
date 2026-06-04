//! Deferred NL embedding initialization for definition search.
//!
//! When a natural-language query hits `fast_search(search_target="definitions")` and
//! no embedding provider has been initialized yet, this module attempts a one-shot
//! deferred init so the first NL query can use hybrid (keyword + semantic) search.

use std::sync::{Arc, LazyLock};
use std::time::Duration;
use tracing::{debug, warn};

use crate::embeddings::EmbeddingProvider;
use crate::handler::JulieServerHandler;
use julie_context::ToolContext;

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

/// If this query looks like natural language (multi-word, no special chars)
/// and no embedding provider exists yet, attempt a deferred one-shot
/// initialization (single-flighted).
///
/// After T8 there is no `search_target` parameter — the gate is purely
/// query-shape-based so embeddings are initialized when the user asks a
/// conceptual question regardless of former target distinctions.
pub(crate) async fn maybe_initialize_embeddings_for_nl_definitions(
    query: &str,
    handler: &dyn ToolContext,
) {
    if !crate::search::scoring::is_nl_like_query(query) {
        return;
    }

    let _ = handler.ensure_embedding_provider(Duration::from_secs(3)).await;
}

pub(crate) async fn wait_for_embedding_provider_settled(
    handler: &JulieServerHandler,
    daemon_timeout: Duration,
) -> Option<Arc<dyn EmbeddingProvider>> {
    // If a provider is already available (daemon shared service or workspace),
    // return it immediately.
    if let Some(provider) = handler.embedding_provider().await {
        return Some(provider);
    }

    // Daemon mode: the shared service may still be in `Initializing` (cold
    // start). Wait briefly for it to settle. CRITICALLY: we must NOT fall
    // through to the per-workspace stdio init path below — that would spawn a
    // SECOND Python sidecar alongside the daemon's shared one, wasting
    // resources and masking the real provider.
    if let Some(svc) = handler.embedding_service.as_ref() {
        use crate::daemon::embedding_service::EmbeddingServiceSettled;
        match svc.wait_until_settled(daemon_timeout).await {
            EmbeddingServiceSettled::Ready { provider, .. } => {
                debug!(
                    "Daemon embedding service became Ready while waiting for NL definition query"
                );
                return Some(provider);
            }
            EmbeddingServiceSettled::Unavailable { reason, .. } => {
                debug!(
                    %reason,
                    "Daemon embedding service Unavailable; NL query falls back to keyword-only"
                );
                return None;
            }
            EmbeddingServiceSettled::Timeout => {
                debug!(
                    "Daemon embedding service did not settle within 3s for NL query; \
                     falling back to keyword-only (provider may become ready later)"
                );
                return None;
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
        return None;
    }

    let _single_flight_guard = NL_DEFINITION_EMBEDDING_INIT_SINGLE_FLIGHT.lock().await;

    // Double-check after acquiring the single-flight mutex: another caller may
    // have completed init while we waited.
    if let Some(provider) = handler.embedding_provider().await {
        return Some(provider);
    }
    let (workspace_identity_root, workspace_for_init) = {
        let workspace_guard = handler.workspace.read().await;
        match workspace_guard.as_ref() {
            Some(workspace) => {
                if workspace.embedding_runtime_status.is_some() {
                    return None;
                }
                (workspace.root.clone(), workspace.clone())
            }
            None => return None,
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
            return None;
        }
    };
    let provider_for_return = initialized_provider.clone();

    let mut workspace_guard = handler.workspace.write().await;
    let workspace = match workspace_guard.as_mut() {
        Some(workspace) => workspace,
        None => return None,
    };

    if workspace.root != workspace_identity_root {
        debug!(
            expected_workspace_root = %workspace_identity_root.display(),
            active_workspace_root = %workspace.root.display(),
            "Discarding stale deferred embedding init result after workspace switch"
        );
        return None;
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

    provider_for_return
}
