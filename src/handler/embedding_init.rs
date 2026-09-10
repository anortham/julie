//! Deferred NL embedding provider initialization — handler-bound backing.
//!
//! `wait_for_embedding_provider_settled` is the implementation that backs
//! `ToolContext::ensure_embedding_provider`. It lives here (in the top crate,
//! adjacent to `tool_context_impl.rs`) because it names the `JulieServerHandler`
//! `workspace` field that is not part of the `ToolContext` facade.
//!
//! Handler-free helpers (`maybe_initialize_embeddings_for_nl_definitions`)
//! stay in `src/tools/search/nl_embeddings.rs`.

use std::sync::{Arc, LazyLock};
use tracing::{debug, warn};

use crate::embeddings::EmbeddingProvider;
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

/// Read and clear the deferred-init attempt counter for a workspace root.
///
/// Only available in `cfg(test)`. Used by `nl_embeddings_daemon_tests` and
/// re-exported via `crate::tools::search::nl_embeddings` for test consumers.
#[cfg(test)]
pub(crate) fn take_nl_definition_embedding_init_attempts(
    workspace_root: &std::path::Path,
) -> usize {
    let mut attempts = NL_DEFINITION_EMBEDDING_INIT_ATTEMPTS
        .lock()
        .expect("nl definition init attempt map mutex poisoned");
    attempts.remove(workspace_root).unwrap_or(0)
}

/// Return the embedding provider, running the per-workspace lazy init once
/// when no provider exists yet.
///
/// This is the backing implementation for `ToolContext::ensure_embedding_provider`.
/// It names `JulieServerHandler` directly to access workspace state, so it
/// cannot live in the handler-free `src/tools/` layer.
pub(crate) async fn wait_for_embedding_provider_settled(
    handler: &JulieServerHandler,
) -> Option<Arc<dyn EmbeddingProvider>> {
    if let Some(provider) = handler.embedding_provider().await {
        return Some(provider);
    }

    let should_attempt_init = {
        let workspace_guard = handler.workspace.read().await;
        match workspace_guard.as_ref() {
            Some(workspace) => match &workspace.embedding_runtime_status {
                None => true,
                Some(status) => status.degraded_reason.as_deref().map_or(false, |r| {
                    r.contains("timeout") || r.contains("unavailable") || r.contains("starting")
                }),
            },
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
                if let Some(ref status) = workspace.embedding_runtime_status {
                    let retryable = status.degraded_reason.as_deref().map_or(false, |r| {
                        r.contains("timeout") || r.contains("unavailable") || r.contains("starting")
                    });
                    if !retryable {
                        return None;
                    }
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

    if initialized_provider.is_some() || workspace.embedding_provider.is_none() {
        workspace.embedding_provider = initialized_provider;
        // Propagate to file watcher so incremental updates use the new provider
        if let Some(ref watcher) = workspace.watcher {
            watcher.update_embedding_provider(workspace.embedding_provider.clone());
        }
    }
    workspace.embedding_runtime_status = initialized_runtime_status;

    provider_for_return
}
