//! Hybrid search (DEPRECATED - semantic search has been removed)
//!
//! Now simply delegates to text search. Kept for API compatibility.
//!
//! **NOTE:** No longer called from production search routing. All search now goes
//! through Tantivy.
#![allow(dead_code)]

use anyhow::Result;
use tracing::debug;

use crate::extractors::Symbol;
use crate::handler::JulieServerHandler;

/// Hybrid search - now just delegates to text search since semantic search was removed
pub async fn hybrid_search_impl(
    query: &str,
    language: &Option<String>,
    file_pattern: &Option<String>,
    limit: u32,
    workspace_ids: Option<Vec<String>>,
    search_target: &str,
    context_lines: Option<u32>,
    handler: &JulieServerHandler,
) -> Result<Vec<Symbol>> {
    debug!("🔄 Hybrid search mode (delegating to text search - semantic search removed)");

    crate::tools::search::text_search::text_search_impl(
        query,
        language,
        file_pattern,
        limit,
        workspace_ids,
        search_target,
        context_lines,
        handler,
    )
    .await
}
