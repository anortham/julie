//! Text-based search implementations
//!
//! Provides text search using SQLite FTS5 and database pattern matching.
//! This is the primary search method for fast, reliable results.

use anyhow::Result;
use tracing::{debug, warn};

use crate::extractors::Symbol;
use crate::handler::JulieServerHandler;
use crate::tools::search::query_preprocessor::{QueryType, preprocess_query};
use crate::utils::query_expansion::{expand_query, is_symbol_name_relevant};
use crate::utils::{exact_match_boost::ExactMatchBoost, path_relevance::PathRelevanceScorer};

use super::query::matches_glob_pattern;

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
    // Step 1: Expand query into multiple variants
    // Try variants in order: exact → CamelCase → snake_case → AND → wildcard → OR
    // Trigger expansion for:
    // - Multi-word queries (contains spaces)
    // - CamelCase/PascalCase queries (contains uppercase letters)
    let needs_expansion = query.contains(' ') || query.chars().any(|c| c.is_uppercase());

    let query_variants = if needs_expansion {
        let variants = expand_query(query);
        debug!(
            "🔄 Query expansion enabled: '{}' → {} variants: {:?}",
            query,
            variants.len(),
            variants
        );
        variants
    } else {
        // Pure lowercase single word - no expansion needed
        vec![query.to_string()]
    };

    // Step 2: Try each query variant until we get sufficient results
    let mut tried_variants: Vec<(String, usize)> = Vec::new();

    for (idx, variant) in query_variants.iter().enumerate() {
        debug!(
            "🔍 Trying variant {}/{}: '{}'",
            idx + 1,
            query_variants.len(),
            variant
        );

        // Preprocess this variant
        let preprocessed = match preprocess_query(variant) {
            Ok(p) => {
                debug!(
                    "✨ Query preprocessor: '{}' → {:?} → FTS5: '{}'",
                    variant, p.query_type, p.fts5_query
                );
                p
            }
            Err(e) => {
                warn!("⚠️  Variant '{}' failed preprocessing: {}", variant, e);
                tried_variants.push((variant.clone(), 0));
                continue; // Try next variant
            }
        };

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

        let results = match effective_search_target {
            "definitions" => {
                // Search symbol definitions only (symbols_fts index)
                if let Some(ref workspace_ids) = workspace_ids {
                    debug!(
                        "🔍 Symbol search with workspace filter: {:?}",
                        workspace_ids
                    );
                    database_search_with_workspace_filter(
                        fts5_query,
                        language,
                        file_pattern,
                        limit,
                        workspace_ids.clone(),
                        handler,
                    )
                    .await
                } else {
                    debug!("🔍 Symbol search in primary workspace (no workspace filter)");
                    // Get primary workspace ID and use it for filtering
                    let workspace = handler
                        .get_workspace()
                        .await?
                        .ok_or_else(|| anyhow::anyhow!("No workspace initialized"))?;
                    let registry_service =
                        crate::workspace::registry_service::WorkspaceRegistryService::new(
                            workspace.root.clone(),
                        );
                    let primary_workspace_id = registry_service
                        .get_primary_workspace_id()
                        .await?
                        .unwrap_or_else(|| "primary".to_string());

                    database_search_with_workspace_filter(
                        fts5_query,
                        language,
                        file_pattern,
                        limit,
                        vec![primary_workspace_id],
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
                    workspace_ids.clone(),
                    context_lines,
                    handler,
                )
                .await
            }
        };

        match results {
            Ok(symbols) => {
                let count = symbols.len();
                debug!("✅ Variant '{}' returned {} results", variant, count);
                tried_variants.push((variant.clone(), count));

                if count > 0 {
                    // Check if we found "good" results:
                    // 1. Actual code (not just documentation)
                    // 2. For single-word CamelCase queries: Symbol name is relevant (not just mentioned in comments)

                    // Only apply strict symbol name check for single-word CamelCase/PascalCase queries
                    // Multi-word queries and dotted identifiers don't have the spurious comment match problem
                    let is_single_word_camelcase = !query.contains(' ')
                        && !query.contains('.')
                        && query.chars().any(|c| c.is_uppercase());

                    let has_relevant_code = symbols.iter().any(|s| {
                        let is_code = s.content_type.is_none();

                        if is_single_word_camelcase {
                            // Strict check: symbol name must be relevant
                            let is_relevant = is_symbol_name_relevant(query, &s.name, variant);
                            is_code && is_relevant
                        } else {
                            // Lenient check: just needs to be code (not docs)
                            is_code
                        }
                    });

                    if has_relevant_code {
                        // Found actual code with relevant symbol names - return immediately
                        debug!(
                            "🎯 Query expansion SUCCESS: Found relevant code symbols with variant '{}' (tried {}/{})",
                            variant,
                            idx + 1,
                            query_variants.len()
                        );
                        return Ok(symbols);
                    } else {
                        // Only found documentation or spurious matches - try next variant
                        debug!(
                            "⚠️  Variant '{}' found {} results but no relevant code symbols (docs/spurious matches), trying next variant",
                            variant, count
                        );
                        // Continue to next variant
                    }
                }
                // No results or only docs, continue to next variant
            }
            Err(e) => {
                warn!("⚠️  Variant '{}' search failed: {}", variant, e);
                tried_variants.push((variant.clone(), 0));
                // Continue to next variant
            }
        }
    }

    // All variants tried, no results found
    debug!(
        "❌ Query expansion exhausted: Tried {} variants, no results. Variants: {:?}",
        tried_variants.len(),
        tried_variants
    );
    Ok(Vec::new())
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
        .expect("workspace_ids Vec should never be empty - this is a bug");

    let is_primary = target_workspace_id == &primary_workspace_id;

    // 🔥 CRITICAL FIX: Query is already sanitized by preprocess_query()!
    // Do NOT call preprocess_fallback_query() - it overrides proper sanitization
    debug!(
        "📝 Workspace filter symbol search: '{}' (workspace: {}, is_primary: {})",
        query, target_workspace_id, is_primary
    );

    // Get the correct database (primary or reference workspace)
    let mut results = if is_primary {
        // Use primary workspace database
        let db = workspace
            .db
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No database available"))?;

        tokio::task::block_in_place(|| {
            let db_lock = match db.lock() {
                Ok(guard) => guard,
                Err(poisoned) => {
                    warn!("Database mutex poisoned, recovering: {}", poisoned);
                    poisoned.into_inner()
                }
            };
            db_lock.find_symbols_by_pattern(query) // Use already-sanitized query
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

        let query_clone = query.to_string(); // Clone for move into spawn_blocking
        tokio::task::spawn_blocking(move || -> Result<Vec<Symbol>> {
            let ref_db = crate::database::SymbolDatabase::new(&ref_db_path)?;
            ref_db.find_symbols_by_pattern(&query_clone) // Use already-sanitized query
        })
        .await
        .map_err(|e| anyhow::anyhow!("Failed to search reference workspace: {}", e))??
    };

    // Apply language filtering if specified
    if let Some(lang) = language {
        results.retain(|symbol| symbol.language.eq_ignore_ascii_case(lang));
    }

    // CRITICAL FIX: Use proper glob matching instead of flawed split() logic
    // This now correctly handles patterns like "src/**/*.rs", "!**/target/*", etc.
    if let Some(pattern) = file_pattern {
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

    // 🔥 CRITICAL FIX: Query is already sanitized by preprocess_query()!
    // Do NOT call preprocess_fallback_query() here - it overrides the proper sanitization
    // that handles hyphens ("tree-sitter" → "tree OR sitter"), dots, colons, etc.
    // The query parameter here is actually the fts5_query from query_preprocessor.

    // 🔥 CONTENT SEARCH FIX: Use AND logic for multi-word queries
    // Unlike symbol search which uses OR for flexibility, content search (grep-like)
    // expects AND behavior - all words must be present, but not necessarily adjacent.
    // Example: "LazyScripts System Administration" → "LazyScripts AND System AND Administration"
    // BUT: If query already has OR (from hyphen/dot/colon sanitization), preserve it!
    let content_query = if query.split_whitespace().count() > 1
        && !query.contains('"')
        && !query.contains(" OR ")  // Don't AND-ify if already has OR from sanitization
        && !query.contains(" AND ")
    {
        query.split_whitespace().collect::<Vec<_>>().join(" AND ")
    } else {
        query.to_string()
    };

    debug!(
        "📝 Content search query: '{}' -> '{}'",
        query, content_query
    );

    // Use FTS5 for file content search with content-optimized query
    // CRITICAL FIX: Wrap blocking rusqlite call in block_in_place
    // rusqlite operations are synchronous blocking I/O that can block Tokio runtime
    let file_results = tokio::task::block_in_place(|| {
        let db_lock = match db.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                tracing::warn!("Database mutex poisoned, recovering: {}", poisoned);
                poisoned.into_inner()
            }
        };
        db_lock.search_file_content_fts(
            &content_query, // Use phrase-wrapped query for content search
            language,
            file_pattern,
            limit as usize,
        )
    })?;

    // Convert FileSearchResult → Symbol with precise line locations
    // CRITICAL FIX: Parse file content to find actual line numbers instead of fake positions
    // 🔥 CRITICAL BUG FIX: Wrap ALL database access in block_in_place to prevent race conditions
    // Bug: db.lock() called in async context causes "database disk image is malformed" errors
    // Fix: Move entire loop inside block_in_place so all DB access is isolated from Tokio runtime
    let symbols = tokio::task::block_in_place(|| {
        let mut symbols = Vec::new();
        for result in file_results {
            // Get file content to find the actual line number of the match
            let db_lock = match db.lock() {
                Ok(guard) => guard,
                Err(poisoned) => {
                    tracing::warn!("Database mutex poisoned, recovering: {}", poisoned);
                    poisoned.into_inner()
                }
            };
        if let Ok(Some(content)) = db_lock.get_file_content(&result.path) {
            // 🔥 FIX: Extract the actual matched term from <mark> tags instead of trying to match entire snippet
            // FTS5 snippets can be multi-line, but we search line-by-line, so matching entire snippet fails.
            // Instead, extract the text inside <mark>...</mark> tags and search for that.
            let marked_term = if let Some(start) = result.snippet.find("<mark>") {
                if let Some(end) = result.snippet[start..].find("</mark>") {
                    result.snippet[start + 6..start + end].trim().to_string()
                } else {
                    // Fallback: use cleaned snippet
                    result
                        .snippet
                        .replace("...", "")
                        .replace("<mark>", "")
                        .replace("</mark>", "")
                        .trim()
                        .to_string()
                }
            } else {
                // Fallback: use cleaned snippet
                result
                    .snippet
                    .replace("...", "")
                    .replace("<mark>", "")
                    .replace("</mark>", "")
                    .trim()
                    .to_string()
            };

            debug!(
                "🔍 Searching for marked term '{}' in {}",
                marked_term, result.path
            );

            // Search for the marked term in file content
            let content_lines: Vec<&str> = content.lines().collect();
            let mut found_line: Option<(usize, String)> = None;

            for (line_idx, line) in content_lines.iter().enumerate() {
                // Check for non-empty trimmed lines before matching
                let trimmed = line.trim();
                if !trimmed.is_empty() && line.contains(&marked_term) {
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
                    content_type: None,
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
                    content_type: None,
                };
                symbols.push(symbol);
            }
        }
        }
        symbols // Return symbols from block_in_place
    }); // End of block_in_place - all DB access now properly isolated

    // Database already filters by language and file_pattern with normalized patterns
    // No need for duplicate filtering here
    debug!(
        "📄 CASCADE: FTS5 returned {} file content matches",
        symbols.len()
    );
    Ok(symbols)
}
