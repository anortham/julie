//! Text-based search implementations
//!
//! Provides text search using Tantivy with code-aware tokenization.
//! Replaces previous SQLite FTS5 implementation with improved handling
//! of CamelCase/snake_case splitting at index time.

use anyhow::Result;
use tracing::debug;

use crate::extractors::{Symbol, SymbolKind};
use crate::handler::JulieServerHandler;
use crate::search::SearchFilter;
use super::query::matches_glob_pattern;

/// Text search with workspace filtering and search target selection
///
/// search_target determines what to search:
/// - "definitions": Search symbol definitions (functions, classes) using Tantivy
/// - "content": Search full file content (grep-like) using Tantivy
///
/// Query expansion and preprocessing are now handled by Tantivy's CodeTokenizer
/// at index time, so CamelCase/snake_case splitting happens automatically.
pub async fn text_search_impl(
    query: &str,
    language: &Option<String>,
    file_pattern: &Option<String>,
    limit: u32,
    _workspace_ids: Option<Vec<String>>,
    search_target: &str,
    _context_lines: Option<u32>,
    handler: &JulieServerHandler,
) -> Result<Vec<Symbol>> {
    // Get the workspace and its search index
    let workspace = handler
        .get_workspace()
        .await?
        .ok_or_else(|| anyhow::anyhow!("No workspace initialized"))?;

    let search_index = workspace
        .search_index
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!(
            "Search index not initialized. Run 'manage_workspace index' first."
        ))?;

    debug!("🔍 Tantivy text search: '{}' (target: {})", query, search_target);

    // Build search filter from parameters
    let filter = SearchFilter {
        language: language.clone(),
        kind: None,
        file_pattern: file_pattern.clone(),
    };

    // Clone the Arc so we can move it into spawn_blocking
    let search_index_clone = search_index.clone();
    let query_clone = query.to_string();
    let limit_usize = limit as usize;
    let search_target_clone = search_target.to_string();

    // Perform the search in a blocking task since Tantivy uses std::sync::Mutex
    let results = tokio::task::spawn_blocking(move || -> Result<Vec<Symbol>> {
        let index = search_index_clone.lock().unwrap();

        // Route based on search_target
        if search_target_clone == "definitions" {
            debug!("🔍 Searching symbols with Tantivy");
            let search_results = index.search_symbols(&query_clone, &filter, limit_usize)?;

            // Convert SymbolSearchResult → Symbol
            let symbols = search_results
                .into_iter()
                .map(|result| Symbol {
                    id: result.id,
                    name: result.name,
                    kind: SymbolKind::from_string(&result.kind),
                    language: result.language,
                    file_path: result.file_path,
                    start_line: result.start_line,
                    signature: if result.signature.is_empty() {
                        None
                    } else {
                        Some(result.signature)
                    },
                    doc_comment: if result.doc_comment.is_empty() {
                        None
                    } else {
                        Some(result.doc_comment)
                    },
                    start_column: 0,
                    end_line: 0,
                    end_column: 0,
                    start_byte: 0,
                    end_byte: 0,
                    visibility: None,
                    parent_id: None,
                    metadata: None,
                    semantic_group: None,
                    confidence: Some(result.score),
                    code_context: None,
                    content_type: None,
                })
                .collect();

            Ok(symbols)
        } else {
            // "content" or any other value: search file content
            debug!("🔍 Searching content with Tantivy");
            let search_results = index.search_content(&query_clone, &filter, limit_usize)?;

            // Convert ContentSearchResult → Symbol (file-level matches)
            let symbols = search_results
                .into_iter()
                .map(|result| Symbol {
                    id: format!("content_{}", result.file_path.replace(['/', '\\'], "_")),
                    name: result.file_path.clone(),
                    kind: SymbolKind::Module, // Represent as file/module match
                    language: result.language,
                    file_path: result.file_path,
                    start_line: 1,
                    signature: None,
                    doc_comment: None,
                    start_column: 0,
                    end_line: 0,
                    end_column: 0,
                    start_byte: 0,
                    end_byte: 0,
                    visibility: None,
                    parent_id: None,
                    metadata: None,
                    semantic_group: Some("content_match".to_string()),
                    confidence: Some(result.score),
                    code_context: None,
                    content_type: None,
                })
                .collect();

            Ok(symbols)
        }
    })
    .await??;

    // Apply file_pattern glob matching as a post-filter if needed
    // (Tantivy may not have indexed full paths for glob matching)
    let filtered_results = if let Some(pattern) = file_pattern {
        results
            .into_iter()
            .filter(|symbol| matches_glob_pattern(&symbol.file_path, pattern))
            .collect()
    } else {
        results
    };

    debug!(
        "✅ Tantivy search returned {} results (after filtering)",
        filtered_results.len()
    );

    Ok(filtered_results)
}
