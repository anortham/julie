//! Text-based search implementations
//!
//! Provides text search using SQLite FTS5 and database pattern matching.
//! This is the primary search method for fast, reliable results.

use anyhow::Result;
use tracing::debug;

use crate::extractors::Symbol;
use crate::handler::JulieServerHandler;
use crate::utils::{
    exact_match_boost::ExactMatchBoost, path_relevance::PathRelevanceScorer,
};

use super::query::{matches_glob_pattern, preprocess_fallback_query};

/// Text search with workspace filtering
///
/// Uses SQLite database for fast symbol pattern matching.
/// With Tantivy removed, we rely on SQLite FTS5 for fast symbol search.
pub async fn text_search_impl(
    query: &str,
    language: &Option<String>,
    file_pattern: &Option<String>,
    limit: u32,
    workspace_ids: Option<Vec<String>>,
    handler: &JulieServerHandler,
) -> Result<Vec<Symbol>> {
    if let Some(workspace_ids) = workspace_ids {
        debug!(
            "🔍 Using database search with workspace filter: {:?}",
            workspace_ids
        );
        database_search_with_workspace_filter(
            query,
            language,
            file_pattern,
            limit,
            workspace_ids,
            handler,
        )
        .await
    } else {
        // For "all" workspaces, use SQLite FTS5 file content search
        debug!("🔍 Using SQLite FTS5 for cross-workspace search");
        sqlite_fts_search(query, language, file_pattern, limit, handler).await
    }
}

/// CASCADE FALLBACK: Database search with workspace filtering
///
/// Used during the 20-30s window while HNSW semantic index builds in background after indexing.
/// Workspace-aware and provides graceful degradation, but lacks multi-word AND/OR logic.
/// INTENTIONALLY KEPT: Part of CASCADE architecture for instant search availability.
async fn database_search_with_workspace_filter(
    query: &str,
    language: &Option<String>,
    file_pattern: &Option<String>,
    limit: u32,
    workspace_ids: Vec<String>,
    handler: &JulieServerHandler,
) -> Result<Vec<Symbol>> {
    let workspace = handler
        .get_workspace()
        .await?
        .ok_or_else(|| anyhow::anyhow!("No workspace initialized"))?;

    let db = workspace
        .db
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No database available"))?;

    // Apply query preprocessing for better fallback search quality
    let processed_query = preprocess_fallback_query(query);
    debug!(
        "📝 Workspace filter query preprocessed: '{}' -> '{}'",
        query, processed_query
    );

    // Use the workspace-aware database search with processed query
    // CRITICAL FIX: Wrap blocking rusqlite call in block_in_place
    // rusqlite operations are synchronous blocking I/O that can block Tokio runtime
    let mut results = tokio::task::block_in_place(|| {
        let db_lock = db.lock().unwrap();
        db_lock.find_symbols_by_pattern(&processed_query, Some(workspace_ids.clone()))
    })?;

    // Apply language filtering if specified
    if let Some(ref lang) = language {
        results.retain(|symbol| symbol.language.eq_ignore_ascii_case(lang));
    }

    // CRITICAL FIX: Use proper glob matching instead of flawed split() logic
    // This now correctly handles patterns like "src/**/*.rs", "!**/target/*", etc.
    if let Some(ref pattern) = file_pattern {
        results.retain(|symbol| matches_glob_pattern(&symbol.file_path, pattern));
    }

    // Apply combined scoring and sorting
    let path_scorer = PathRelevanceScorer::new(query);
    let exact_match_booster = ExactMatchBoost::new(query);
    results.sort_by(|a, b| {
        let path_score_a = path_scorer.calculate_score(&a.file_path);
        let exact_boost_a = exact_match_booster.calculate_boost(&a.name);
        let combined_score_a = path_score_a * exact_boost_a;

        let path_score_b = path_scorer.calculate_score(&b.file_path);
        let exact_boost_b = exact_match_booster.calculate_boost(&b.name);
        let combined_score_b = path_score_b * exact_boost_b;

        combined_score_b
            .partial_cmp(&combined_score_a)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Apply limit
    if results.len() > limit as usize {
        results.truncate(limit as usize);
    }

    debug!(
        "🗄️ Database search with workspace filter returned {} results",
        results.len()
    );
    Ok(results)
}

/// Graceful degradation: SQLite-based search when HNSW semantic search isn't ready
///
/// CASCADE: Search using SQLite FTS5 (file content full-text search).
/// This is the final fallback that always works.
async fn sqlite_fts_search(
    query: &str,
    _language: &Option<String>,
    _file_pattern: &Option<String>,
    limit: u32,
    handler: &JulieServerHandler,
) -> Result<Vec<Symbol>> {
    debug!("🔍 CASCADE: Using SQLite FTS5 search (file content)");

    // Get workspace and database
    let workspace = handler
        .get_workspace()
        .await?
        .ok_or_else(|| anyhow::anyhow!("No workspace initialized for FTS search"))?;

    let db = workspace
        .db
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No database available for FTS search"))?;

    // Get workspace ID for filtering
    let workspace_id = {
        let registry_service =
            crate::workspace::registry_service::WorkspaceRegistryService::new(
                workspace.root.clone(),
            );
        registry_service
            .get_primary_workspace_id()
            .await?
            .unwrap_or_else(|| "primary".to_string())
    };

    // Apply basic query intelligence even in fallback mode
    // This improves search quality during the 20-30s window while HNSW builds
    let processed_query = preprocess_fallback_query(query);
    debug!(
        "📝 Fallback query preprocessed: '{}' -> '{}'",
        query, processed_query
    );

    // Use FTS5 for file content search with processed query
    // CRITICAL FIX: Wrap blocking rusqlite call in block_in_place
    // rusqlite operations are synchronous blocking I/O that can block Tokio runtime
    let file_results = tokio::task::block_in_place(|| {
        let db_lock = db.lock().unwrap();
        db_lock.search_file_content_fts(
            &processed_query,
            Some(&workspace_id),
            limit as usize,
        )
    })?;

    // Convert FileSearchResult → Symbol (FILE_CONTENT symbols for consistency)
    let mut symbols = Vec::new();
    for result in file_results {
        // Create a FILE_CONTENT symbol from the FTS result
        let symbol = crate::extractors::Symbol {
            id: format!("fts_result_{}", result.path.replace(['/', '\\'], "_")),
            name: format!(
                "FILE_CONTENT: {}",
                std::path::Path::new(&result.path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            ),
            kind: crate::extractors::SymbolKind::Module,
            language: "text".to_string(),
            file_path: result.path.clone(),
            start_line: 1,
            start_column: 0,
            end_line: 1,
            end_column: 0,
            start_byte: 0,
            end_byte: 0,
            signature: Some(format!("FTS5 match (relevance: {:.4})", result.rank)),
            doc_comment: Some(result.snippet.clone()),
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: Some("file_content".to_string()),
            confidence: Some(result.rank),
            code_context: Some(result.snippet),
        };
        symbols.push(symbol);
    }

    debug!(
        "📄 CASCADE: FTS5 returned {} file content matches",
        symbols.len()
    );
    Ok(symbols)
}
