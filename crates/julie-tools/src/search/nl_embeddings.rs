//! Deferred NL embedding initialization for definition search.
//!
//! Handler-free helpers live here and will move to `julie-tools` in T2b.5b.
//! The handler-bound backing function (`wait_for_embedding_provider_settled`)
//! has moved to `src/handler/embedding_init.rs`.

use std::time::Duration;

use julie_context::ToolContext;

/// If this query looks like natural language (multi-word, no special chars)
/// and no embedding provider exists yet, attempt a deferred one-shot
/// initialization (single-flighted).
///
/// After T8 there is no `search_target` parameter — the gate is purely
/// query-shape-based so embeddings are initialized when the user asks a
/// conceptual question regardless of former target distinctions.
pub async fn maybe_initialize_embeddings_for_nl_definitions(
    query: &str,
    handler: &dyn ToolContext,
) {
    if !julie_index::search::scoring::is_nl_like_query(query) {
        return;
    }

    let _ = handler
        .ensure_embedding_provider(Duration::from_secs(3))
        .await;
}
