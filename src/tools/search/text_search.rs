//! Text-based search implementations
//!
//! Provides text search using SQLite FTS5 and database pattern matching.
//! This is the primary search method for fast, reliable results.

use anyhow::Result;
use tracing::debug;

use crate::extractors::Symbol;
use crate::handler::JulieServerHandler;
use crate::tools::search::query_preprocessor::{preprocess_query, QueryType};
use crate::utils::{exact_match_boost::ExactMatchBoost, path_relevance::PathRelevanceScorer};

use super::query::{matches_glob_pattern, preprocess_fallback_query};

/// Text search with workspace filtering and search target selection
///
/// search_target determines what to search:
/// - "definitions": Search symbol definitions (functions, classes) using symbols_fts
/// - "content": Search full file content (grep-like) using files_fts
pub async fn text_search_impl(
    query: &str,
    language: &Option<String>,
    file_pattern: &Option<String>,
    limit: u32,
    workspace_ids: Option<Vec<String>>,
    search_target: &str,
    context_lines: Option<u32>,
    handler: &JulieServerHandler,
) -> Result<Vec<Symbol>> {
    // Step 1: Preprocess query (validate, detect type, sanitize)
    let preprocessed = match preprocess_query(query) {
        Ok(p) => {
            debug!(
                "✨ Query preprocessor: '{}' → {:?} → FTS5: '{}'",
                query, p.query_type, p.fts5_query
            );
            p
        }
        Err(e) => {
            // Preprocessing failed (e.g., pure wildcard query)
            return Err(anyhow::anyhow!("Invalid query: {}", e));
        }
    };

    // Step 2: Use preprocessed FTS5 query for search
    let fts5_query = &preprocessed.fts5_query;

    // Step 3: Route based on query type and search_target
    // Symbol queries go to definitions, Pattern/Standard to content
    let effective_search_target = match preprocessed.query_type {
        QueryType::Symbol if search_target != "content" => "definitions",
        QueryType::Glob if file_pattern.is_none() => {
            // For glob queries without explicit file_pattern, search content
            // The glob matching will happen via the file_path field
            "content"
        }
        _ => search_target, // Respect user's explicit search_target
    };

    match effective_search_target {
        "definitions" => {
            // Search symbol definitions only (symbols_fts index)
            if let Some(workspace_ids) = workspace_ids {
                debug!(
                    "🔍 Symbol search with workspace filter: {:?}",
                    workspace_ids
                );
                database_search_with_workspace_filter(
                    fts5_query,
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
                    fts5_query,
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
            sqlite_fts_search(
                fts5_query,
                language,
                file_pattern,
                limit,
                workspace_ids,
                context_lines,
                handler,
            )
            .await
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
            db_lock.find_symbols_by_pattern(&processed_query)
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
            ref_db.find_symbols_by_pattern(&processed_query)
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

/// Check if a line contains useful code context (not just punctuation/whitespace)
fn is_useful_line(line: &str) -> bool {
    let trimmed = line.trim();

    // Empty or very short lines are not useful
    if trimmed.is_empty() || trimmed.len() < 2 {
        return false;
    }

    // Lines that are ONLY punctuation are not useful
    if trimmed == "}"
        || trimmed == "{"
        || trimmed == ");"
        || trimmed == "("
        || trimmed == "//"
        || trimmed == "/*"
        || trimmed == "*/"
        || trimmed == "*"
        || trimmed == "---"
        || trimmed == "```"
        || trimmed == "///"
    {
        return false;
    }

    // Lines with useful patterns are good
    if trimmed.starts_with("pub ")
        || trimmed.starts_with("fn ")
        || trimmed.starts_with("class ")
        || trimmed.starts_with("impl ")
        || trimmed.starts_with("struct ")
        || trimmed.starts_with("enum ")
        || trimmed.starts_with("interface ")
        || trimmed.starts_with("function ")
        || trimmed.starts_with("async ")
        || trimmed.starts_with("export ")
        || trimmed.starts_with("///")
        || trimmed.starts_with("/**")
    {
        return true;
    }

    // Default: useful if it has substantial content (not just punctuation)
    let has_alphanumeric = trimmed.chars().any(|c| c.is_alphanumeric());
    has_alphanumeric && trimmed.len() >= 10
}

/// Extract context lines around a match with line numbers (grep-style)
fn extract_context_lines(content: &str, match_line_num: usize, context_lines: u32) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let num_context = context_lines as usize;

    // Calculate window bounds
    let start_idx = match_line_num.saturating_sub(num_context + 1); // -1 because line_num is 1-indexed
    let end_idx = std::cmp::min(match_line_num + num_context, lines.len());

    // Extract lines with line numbers
    let mut result = Vec::new();
    for (idx, line) in lines
        .iter()
        .enumerate()
        .skip(start_idx)
        .take(end_idx - start_idx)
    {
        let line_num = idx + 1; // 1-indexed line numbers
        if line_num == match_line_num {
            // Mark the matched line with an arrow
            result.push(format!("{}→ {}", line_num, line));
        } else {
            result.push(format!("{}: {}", line_num, line));
        }
    }

    result.join("\n")
}

/// Find intelligent context when the matched line is useless
/// Searches nearby lines for symbol definitions or meaningful code
fn find_intelligent_context(
    lines: &[&str],
    match_line_idx: usize, // 0-indexed
) -> Option<(usize, String)> {
    // Search window: ±3 lines around match
    let start = match_line_idx.saturating_sub(3);
    let end = std::cmp::min(match_line_idx + 4, lines.len());

    // Priority 1: Find symbol definitions (pub fn, class, struct, etc.)
    for (idx, line) in lines.iter().enumerate().skip(start).take(end - start) {
        if is_useful_line(line) {
            let trimmed = line.trim();
            // Prioritize symbol definitions
            if trimmed.starts_with("pub ")
                || trimmed.starts_with("fn ")
                || trimmed.starts_with("class ")
                || trimmed.starts_with("impl ")
                || trimmed.starts_with("struct ")
                || trimmed.starts_with("enum ")
                || trimmed.starts_with("interface ")
                || trimmed.starts_with("function ")
                || trimmed.starts_with("export class ")
                || trimmed.starts_with("export function ")
            {
                return Some((idx + 1, line.to_string())); // Return 1-indexed line number
            }
        }
    }

    // Priority 2: Find any useful line (doc comments, meaningful code)
    for (idx, line) in lines.iter().enumerate().skip(start).take(end - start) {
        if is_useful_line(line) {
            return Some((idx + 1, line.to_string())); // Return 1-indexed line number
        }
    }

    // Fallback: return the original match if nothing better found
    None
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
    context_lines: Option<u32>,
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
            language,
            file_pattern,
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
            // 🔥 FIX: Extract the actual matched term from <mark> tags instead of trying to match entire snippet
            // FTS5 snippets can be multi-line, but we search line-by-line, so matching entire snippet fails.
            // Instead, extract the text inside <mark>...</mark> tags and search for that.
            let marked_term = if let Some(start) = result.snippet.find("<mark>") {
                if let Some(end) = result.snippet[start..].find("</mark>") {
                    result.snippet[start + 6..start + end].trim().to_string()
                } else {
                    // Fallback: use cleaned snippet
                    result.snippet.replace("...", "").replace("<mark>", "").replace("</mark>", "").trim().to_string()
                }
            } else {
                // Fallback: use cleaned snippet
                result.snippet.replace("...", "").replace("<mark>", "").replace("</mark>", "").trim().to_string()
            };

            debug!("🔍 Searching for marked term '{}' in {}", marked_term, result.path);

            // Search for the marked term in file content
            let content_lines: Vec<&str> = content.lines().collect();
            let mut found_line: Option<(usize, String)> = None;

            for (line_idx, line) in content_lines.iter().enumerate() {
                // Check for non-empty trimmed lines before matching
                let trimmed = line.trim();
                if !trimmed.is_empty() && line.contains(&marked_term)
                {
                    let initial_line_num = line_idx + 1; // 1-indexed
                    let initial_line_content = line.to_string();

                    // Phase 3: Intelligent line selection
                    // If the matched line is useless (just punctuation), find better context nearby
                    if !is_useful_line(line) {
                        debug!(
                            "⚠️ Matched line {} in {} is not useful ('{}'), searching for better context",
                            initial_line_num, result.path, trimmed
                        );

                        if let Some((better_line_num, better_content)) =
                            find_intelligent_context(&content_lines, line_idx)
                        {
                            debug!(
                                "✓ Found better context at line {} in {}",
                                better_line_num, result.path
                            );
                            found_line = Some((better_line_num, better_content));
                        } else {
                            // No better context found, use original
                            found_line = Some((initial_line_num, initial_line_content));
                        }
                    } else {
                        // Line is already useful, use it
                        found_line = Some((initial_line_num, initial_line_content));
                    }
                    break;
                }
            }

            if let Some((line_num, _line_content)) = found_line {
                // Phase 2: Multi-line context extraction
                // Extract context based on context_lines parameter (default: 1 = ±1 line)
                let num_context_lines = context_lines.unwrap_or(1);
                let code_context_text = if num_context_lines == 0 {
                    // Single line only - use the line content directly
                    content_lines
                        .get(line_num - 1)
                        .map(|l| l.to_string())
                        .unwrap_or_default()
                } else {
                    // Multi-line with grep-style formatting (line numbers + arrow indicator)
                    extract_context_lines(&content, line_num, num_context_lines)
                };

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
                    end_column: code_context_text.len() as u32,
                    start_byte: 0,
                    end_byte: 0,
                    signature: Some(format!("FTS5 match (relevance: {:.4})", result.rank)),
                    doc_comment: None,
                    visibility: None,
                    parent_id: None,
                    metadata: None,
                    semantic_group: Some("fts_match".to_string()),
                    confidence: Some(result.rank),
                    code_context: Some(code_context_text),
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

    // Database already filters by language and file_pattern with normalized patterns
    // No need for duplicate filtering here
    debug!(
        "📄 CASCADE: FTS5 returned {} file content matches",
        symbols.len()
    );
    Ok(symbols)
}
