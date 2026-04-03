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

    // No provider yet. In daemon mode the shared service would have returned
    // one, so this is either stdio mode or a transient initialization gap.
    // Check whether a deferred init is worth attempting.
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
