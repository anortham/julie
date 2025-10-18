//! Text-based search implementations
//!
//! Provides text search using SQLite FTS5 and database pattern matching.
//! This is the primary search method for fast, reliable results.

use anyhow::Result;
use tracing::debug;

use crate::extractors::Symbol;
use crate::handler::JulieServerHandler;
use crate::utils::{exact_match_boost::ExactMatchBoost, path_relevance::PathRelevanceScorer};

use super::query::{matches_glob_pattern, preprocess_fallback_query};

/// Text search with workspace filtering and scope selection
///
/// Scope determines what to search:
/// - "symbols": Search symbol definitions (functions, classes) using symbols_fts
/// - "content": Search full file content (grep-like) using files_fts
pub async fn text_search_impl(
    query: &str,
    language: &Option<String>,
    file_pattern: &Option<String>,
    limit: u32,
    workspace_ids: Option<Vec<String>>,
    scope: &str,
    handler: &JulieServerHandler,
) -> Result<Vec<Symbol>> {
    match scope {
        "symbols" => {
            // Search symbol definitions only (symbols_fts index)
            if let Some(workspace_ids) = workspace_ids {
                debug!(
                    "🔍 Symbol search with workspace filter: {:?}",
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
                debug!("🔍 Symbol search across all workspaces");
                database_search_with_workspace_filter(
                    query,
                    language,
                    file_pattern,
                    limit,
                    vec![], // Empty vec means search primary workspace
                    handler,
                )
                .await
            }
        }
        _ => {
            // "content" or any other value: Search full file content (files_fts index)
            debug!("🔍 Content search (full file text)");
            sqlite_fts_search(query, language, file_pattern, limit, workspace_ids, handler).await
        }
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

    // Determine if searching primary or reference workspace
    let registry_service =
        crate::workspace::registry_service::WorkspaceRegistryService::new(workspace.root.clone());
    let primary_workspace_id = registry_service
        .get_primary_workspace_id()
        .await?
        .unwrap_or_else(|| "primary".to_string());

    let target_workspace_id = workspace_ids
        .first()
        .ok_or_else(|| anyhow::anyhow!("No workspace ID provided"))?;

    let is_primary = target_workspace_id == &primary_workspace_id;

    // Apply query preprocessing for better fallback search quality
    let processed_query = preprocess_fallback_query(query);
    debug!(
        "📝 Workspace filter query preprocessed: '{}' -> '{}' (workspace: {}, is_primary: {})",
        query, processed_query, target_workspace_id, is_primary
    );

    // Get the correct database (primary or reference workspace)
    let mut results = if is_primary {
        // Use primary workspace database
        let db = workspace
            .db
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No database available"))?;

        tokio::task::block_in_place(|| {
            let db_lock = db.lock().unwrap();
            db_lock.find_symbols_by_pattern(&processed_query, Some(workspace_ids.clone()))
        })?
    } else {
        // Open reference workspace database
        let ref_db_path = workspace.workspace_db_path(target_workspace_id);
        if !ref_db_path.exists() {
            return Err(anyhow::anyhow!(
                "Reference workspace database not found: {}",
                target_workspace_id
            ));
        }

        debug!("📂 Opening reference workspace DB: {:?}", ref_db_path);

        tokio::task::spawn_blocking(move || -> Result<Vec<Symbol>> {
            let ref_db = crate::database::SymbolDatabase::new(&ref_db_path)?;
            ref_db.find_symbols_by_pattern(&processed_query, Some(workspace_ids.clone()))
        })
        .await
        .map_err(|e| anyhow::anyhow!("Failed to search reference workspace: {}", e))??
    };

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
    language: &Option<String>,
    file_pattern: &Option<String>,
    limit: u32,
    workspace_ids: Option<Vec<String>>,
    handler: &JulieServerHandler,
) -> Result<Vec<Symbol>> {
    debug!("🔍 CASCADE: Using SQLite FTS5 search (file content)");

    // Get workspace
    let workspace = handler
        .get_workspace()
        .await?
        .ok_or_else(|| anyhow::anyhow!("No workspace initialized for FTS search"))?;

    // Get the correct database based on workspace filter
    let db = if let Some(workspace_ids) = workspace_ids {
        // Workspace filter specified - determine if primary or reference
        let registry_service = crate::workspace::registry_service::WorkspaceRegistryService::new(
            workspace.root.clone(),
        );
        let primary_workspace_id = registry_service
            .get_primary_workspace_id()
            .await?
            .unwrap_or_else(|| "primary".to_string());

        let target_workspace_id = workspace_ids
            .first()
            .ok_or_else(|| anyhow::anyhow!("Empty workspace ID list"))?;

        let is_primary = target_workspace_id == &primary_workspace_id;

        debug!(
            "🔍 Content search targeting workspace: {} (is_primary: {})",
            target_workspace_id, is_primary
        );

        if is_primary {
            // Use primary workspace database
            workspace
                .db
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("No database available for FTS search"))?
                .clone()
        } else {
            // Open reference workspace database
            let ref_db_path = workspace.workspace_db_path(target_workspace_id);
            if !ref_db_path.exists() {
                return Err(anyhow::anyhow!(
                    "Reference workspace database not found: {}",
                    target_workspace_id
                ));
            }

            debug!(
                "📂 Opening reference workspace DB for content search: {:?}",
                ref_db_path
            );

            // Create Arc<Mutex<SymbolDatabase>> for consistent type
            std::sync::Arc::new(std::sync::Mutex::new(crate::database::SymbolDatabase::new(
                &ref_db_path,
            )?))
        }
    } else {
        // No workspace filter - use primary workspace database directly
        debug!("🔍 Content search using primary workspace (no filter specified)");
        workspace
            .db
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No database available for FTS search"))?
            .clone()
    };

    // Apply basic query intelligence even in fallback mode
    // This improves search quality during the 20-30s window while HNSW builds
    let processed_query = preprocess_fallback_query(query);

    // 🔥 CONTENT SEARCH FIX: Use AND logic for multi-word queries
    // Unlike symbol search which uses OR for flexibility, content search (grep-like)
    // expects AND behavior - all words must be present, but not necessarily adjacent.
    // Example: "LazyScripts System Administration" → "LazyScripts AND System AND Administration"
    let content_query = if processed_query.split_whitespace().count() > 1
        && !processed_query.contains('"')
        && !processed_query.contains(" OR ")
        && !processed_query.contains(" AND ")
    {
        processed_query
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" AND ")
    } else {
        processed_query.clone()
    };

    debug!(
        "📝 Content search query: '{}' -> '{}'",
        query, content_query
    );

    // Use FTS5 for file content search with content-optimized query
    // CRITICAL FIX: Wrap blocking rusqlite call in block_in_place
    // rusqlite operations are synchronous blocking I/O that can block Tokio runtime
    let file_results = tokio::task::block_in_place(|| {
        let db_lock = db.lock().unwrap();
        db_lock.search_file_content_fts(
            &content_query, // Use phrase-wrapped query for content search
            limit as usize,
        )
    })?;

    // Convert FileSearchResult → Symbol with precise line locations
    // CRITICAL FIX: Parse file content to find actual line numbers instead of fake positions
    let mut symbols = Vec::new();
    for result in file_results {
        // Get file content to find the actual line number of the match
        let db_lock = db.lock().unwrap();
        if let Ok(Some(content)) = db_lock.get_file_content(&result.path) {
            // Find the line containing the snippet text
            // Remove FTS highlighting markers (...) from snippet for matching
            let clean_snippet = result.snippet.replace("...", "").trim().to_string();

            // Search for the snippet in file content
            let mut found_line: Option<(usize, String)> = None;
            for (line_idx, line) in content.lines().enumerate() {
                if line.contains(&clean_snippet) || clean_snippet.contains(line.trim()) {
                    found_line = Some((line_idx + 1, line.to_string()));
                    break;
                }
            }

            if let Some((line_num, line_content)) = found_line {
                // Create a proper symbol with real line location
                let symbol = crate::extractors::Symbol {
                    id: format!("fts_{}_{}", result.path.replace(['/', '\\'], "_"), line_num),
                    name: format!(
                        "{}:{}",
                        std::path::Path::new(&result.path)
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy(),
                        line_num
                    ),
                    kind: crate::extractors::SymbolKind::Module,
                    language: "text".to_string(),
                    file_path: result.path.clone(),
                    start_line: line_num as u32,
                    start_column: 0,
                    end_line: line_num as u32,
                    end_column: line_content.len() as u32,
                    start_byte: 0,
                    end_byte: 0,
                    signature: Some(format!("FTS5 match (relevance: {:.4})", result.rank)),
                    doc_comment: None,
                    visibility: None,
                    parent_id: None,
                    metadata: None,
                    semantic_group: Some("fts_match".to_string()),
                    confidence: Some(result.rank),
                    code_context: Some(line_content.trim().to_string()),
                };
                symbols.push(symbol);
            } else {
                // Fallback: couldn't find exact line, use snippet as context
                debug!(
                    "⚠️ Could not locate exact line for FTS match in {}",
                    result.path
                );
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
        }
    }

    // Apply language and file pattern filtering to content search results
    // This ensures scope="content" respects the same filters as scope="symbols"
    let total_before_filter = symbols.len();
    let filtered_symbols: Vec<Symbol> = symbols
        .into_iter()
        .filter(|symbol| {
            // Apply language filter if specified
            let language_match = if let Some(ref lang) = language {
                // Extract file extension and match against language
                let path = std::path::Path::new(&symbol.file_path);
                if let Some(ext) = path.extension() {
                    let ext_str = ext.to_string_lossy().to_lowercase();
                    // Simple extension matching - could be enhanced with language detection
                    match lang.to_lowercase().as_str() {
                        "rust" => ext_str == "rs",
                        "typescript" => ext_str == "ts" || ext_str == "tsx",
                        "javascript" => ext_str == "js" || ext_str == "jsx" || ext_str == "mjs",
                        "python" => ext_str == "py",
                        "java" => ext_str == "java",
                        "csharp" | "c#" => ext_str == "cs",
                        "cpp" | "c++" => ext_str == "cpp" || ext_str == "cc" || ext_str == "cxx",
                        "c" => ext_str == "c" || ext_str == "h",
                        _ => ext_str == lang.to_lowercase(),
                    }
                } else {
                    false
                }
            } else {
                true // No language filter, accept all
            };

            // Apply file pattern filter if specified
            let file_match = file_pattern
                .as_ref()
                .map(|pattern| super::query::matches_glob_pattern(&symbol.file_path, pattern))
                .unwrap_or(true);

            language_match && file_match
        })
        .collect();

    debug!(
        "📄 CASCADE: FTS5 returned {} file content matches (filtered to {})",
        total_before_filter,
        filtered_symbols.len()
    );
    Ok(filtered_symbols)
}
