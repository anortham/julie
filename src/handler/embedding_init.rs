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

    #[cfg(test)]
    record_nl_definition_embedding_init_attempt(&handler.current_workspace_root());

    handler
        .acquire_embedding_provider(std::time::Duration::from_secs(30))
        .await
}
