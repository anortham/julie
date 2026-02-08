//! Text-based search implementations
//!
//! Provides text search using Tantivy with code-aware tokenization.
//! Replaces previous SQLite FTS5 implementation with improved handling
//! of CamelCase/snake_case splitting at index time.

use anyhow::Result;
use tracing::{debug, warn};

use crate::extractors::{Symbol, SymbolKind};
use crate::handler::JulieServerHandler;
use crate::search::{SearchFilter, SearchIndex};
use super::query::matches_glob_pattern;

/// Text search with workspace filtering and search target selection
///
/// search_target determines what to search:
/// - "definitions": Search symbol definitions (functions, classes) using Tantivy
/// - "content": Search full file content (grep-like) using Tantivy with post-verification
///
/// Query expansion and preprocessing are now handled by Tantivy's CodeTokenizer
/// at index time, so CamelCase/snake_case splitting happens automatically.
///
/// For content search, Tantivy is used as a candidate retrieval stage, then each
/// candidate file is verified against actual content from SQLite to eliminate
/// false positives caused by CodeTokenizer over-splitting (e.g. "Blake3 hash"
/// tokenizes to ["blake","3","hash"], matching files with unrelated "3" and "hash").
pub async fn text_search_impl(
    query: &str,
    language: &Option<String>,
    file_pattern: &Option<String>,
    limit: u32,
    workspace_ids: Option<Vec<String>>,
    search_target: &str,
    _context_lines: Option<u32>,
    handler: &JulieServerHandler,
) -> Result<Vec<Symbol>> {
    // Get the primary workspace (always needed for path resolution)
    let workspace = handler
        .get_workspace()
        .await?
        .ok_or_else(|| anyhow::anyhow!("No workspace initialized"))?;

    // Determine if we're targeting a reference workspace
    let ref_workspace_id = if let Some(ref ids) = workspace_ids {
        if let Some(id) = ids.first() {
            let registry = crate::workspace::registry_service::WorkspaceRegistryService::new(
                workspace.root.clone(),
            );
            let primary_id = registry
                .get_primary_workspace_id()
                .await?
                .unwrap_or_default();
            if *id != primary_id {
                Some(id.clone())
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    debug!(
        "🔍 Tantivy text search: '{}' (target: {}, ref_workspace: {:?})",
        query, search_target, ref_workspace_id
    );

    // Build search filter from parameters
    let filter = SearchFilter {
        language: language.clone(),
        kind: None,
        file_pattern: file_pattern.clone(),
    };

    let query_clone = query.to_string();
    let limit_usize = limit as usize;
    let search_target_clone = search_target.to_string();

    // Reference workspace: open isolated Tantivy index + SQLite DB
    if let Some(ref_id) = ref_workspace_id {
        let tantivy_path = workspace.workspace_tantivy_path(&ref_id);
        let ref_db_path = workspace.workspace_db_path(&ref_id);

        let results = tokio::task::spawn_blocking(move || -> Result<Vec<Symbol>> {
            if !tantivy_path.join("meta.json").exists() {
                debug!("No Tantivy index for reference workspace, returning empty");
                return Ok(Vec::new());
            }

            let index = SearchIndex::open(&tantivy_path)?;

            if search_target_clone == "definitions" {
                let search_results =
                    index.search_symbols(&query_clone, &filter, limit_usize)?;

                let mut symbols: Vec<Symbol> = search_results
                    .into_iter()
                    .map(|result| tantivy_symbol_to_symbol(result))
                    .collect();

                // Enrich with code_context from reference workspace's SQLite
                if ref_db_path.exists() {
                    if let Ok(ref_db) = crate::database::SymbolDatabase::new(&ref_db_path) {
                        enrich_symbols_from_db(&mut symbols, &ref_db);
                    }
                }

                Ok(symbols)
            } else {
                // Content search on reference workspace
                let fetch_limit = limit_usize.saturating_mul(5).max(50);
                let search_results =
                    index.search_content(&query_clone, &filter, fetch_limit)?;

                let query_words: Vec<String> = query_clone
                    .split(|c: char| !c.is_alphanumeric())
                    .filter(|w| !w.is_empty())
                    .map(|w| w.to_lowercase())
                    .collect();

                let mut verified_symbols = Vec::with_capacity(limit_usize);

                if ref_db_path.exists() {
                    if let Ok(ref_db) = crate::database::SymbolDatabase::new(&ref_db_path) {
                        for result in search_results {
                            if verified_symbols.len() >= limit_usize {
                                break;
                            }
                            match ref_db.get_file_content(&result.file_path) {
                                Ok(Some(content)) => {
                                    let content_lower = content.to_lowercase();
                                    if query_words
                                        .iter()
                                        .all(|word| content_lower.contains(word.as_str()))
                                    {
                                        verified_symbols.push(content_result_to_symbol(result));
                                    }
                                }
                                _ => {
                                    verified_symbols.push(content_result_to_symbol(result));
                                }
                            }
                        }
                    }
                } else {
                    for result in search_results.into_iter().take(limit_usize) {
                        verified_symbols.push(content_result_to_symbol(result));
                    }
                }

                Ok(verified_symbols)
            }
        })
        .await??;

        // Apply file_pattern glob matching as a post-filter
        let filtered_results = if let Some(pattern) = file_pattern {
            results
                .into_iter()
                .filter(|symbol| matches_glob_pattern(&symbol.file_path, pattern))
                .collect()
        } else {
            results
        };

        debug!(
            "✅ Reference workspace search returned {} results",
            filtered_results.len()
        );

        return Ok(filtered_results);
    }

    // Primary workspace: use shared search index
    let search_index = workspace
        .search_index
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!(
            "Search index not initialized. Run 'manage_workspace index' first."
        ))?;

    // Clone the Arc so we can move it into spawn_blocking
    let search_index_clone = search_index.clone();

    // Clone DB for both definition search (code_context enrichment) and
    // content search (post-verification filtering)
    let db_clone = workspace.db.clone();

    // Perform the search in a blocking task since Tantivy uses std::sync::Mutex
    let results = tokio::task::spawn_blocking(move || -> Result<Vec<Symbol>> {
        let index = search_index_clone.lock().unwrap();

        // Route based on search_target
        if search_target_clone == "definitions" {
            debug!("🔍 Searching symbols with Tantivy");
            let search_results = index.search_symbols(&query_clone, &filter, limit_usize)?;

            let mut symbols: Vec<Symbol> = search_results
                .into_iter()
                .map(|result| tantivy_symbol_to_symbol(result))
                .collect();

            // Enrich with code_context from SQLite (Tantivy doesn't store code_body)
            if let Some(db_arc) = &db_clone {
                let db_lock = match db_arc.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => {
                        warn!("Database mutex poisoned during code_context enrichment, recovering");
                        poisoned.into_inner()
                    }
                };
                enrich_symbols_from_db(&mut symbols, &db_lock);
            }

            Ok(symbols)
        } else {
            // "content" or any other value: search file content
            debug!("🔍 Searching content with Tantivy");

            // Fetch more candidates than the limit for post-verification.
            // CodeTokenizer may over-split queries (e.g. "Blake3 hash" → ["blake","3","hash"]),
            // producing Tantivy matches that don't actually contain the query substring.
            let fetch_limit = limit_usize.saturating_mul(5).max(50);
            let search_results = index.search_content(&query_clone, &filter, fetch_limit)?;

            // Post-verify: check that all query words appear in each file's content.
            // This eliminates false positives from CodeTokenizer over-splitting
            // (e.g. "Blake3 hash" splits to ["blake","3","hash"] in Tantivy, but
            // verification requires "blake3" and "hash" as user-typed words).
            //
            // We split the query on non-alphanumeric boundaries — each resulting
            // word must appear as a case-insensitive substring in the file content.
            // Using non-alphanumeric splitting handles code delimiters like `::`
            // (Rust paths), `-` (hyphenated terms), `.` (dotted paths) naturally,
            // while preserving alphanumeric sequences like "Blake3" intact.
            let query_words: Vec<String> = query_clone
                .split(|c: char| !c.is_alphanumeric())
                .filter(|w| !w.is_empty())
                .map(|w| w.to_lowercase())
                .collect();
            let mut verified_symbols = Vec::with_capacity(limit_usize);

            if let Some(db_arc) = &db_clone {
                let db_lock = match db_arc.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => {
                        warn!("Database mutex poisoned during content verification, recovering");
                        poisoned.into_inner()
                    }
                };

                for result in search_results {
                    if verified_symbols.len() >= limit_usize {
                        break;
                    }

                    // Verify all query words appear in actual file content
                    match db_lock.get_file_content(&result.file_path) {
                        Ok(Some(content)) => {
                            let content_lower = content.to_lowercase();
                            let all_words_match = query_words
                                .iter()
                                .all(|word| content_lower.contains(word.as_str()));

                            if all_words_match {
                                verified_symbols.push(content_result_to_symbol(result));
                            } else {
                                debug!(
                                    "Filtered false positive: {} (missing query words for '{}')",
                                    result.file_path, query_clone
                                );
                            }
                        }
                        Ok(None) => {
                            // File not in DB (maybe deleted) — include as-is
                            verified_symbols.push(content_result_to_symbol(result));
                        }
                        Err(e) => {
                            // DB error — include as-is (graceful degradation)
                            debug!(
                                "Could not verify content for {}: {}",
                                result.file_path, e
                            );
                            verified_symbols.push(content_result_to_symbol(result));
                        }
                    }
                }
            } else {
                // No database available — return unverified results (graceful degradation)
                debug!("No database available for content verification, returning unverified results");
                for result in search_results.into_iter().take(limit_usize) {
                    verified_symbols.push(content_result_to_symbol(result));
                }
            }

            Ok(verified_symbols)
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

/// Convert a Tantivy SymbolSearchResult into an extractors Symbol.
fn tantivy_symbol_to_symbol(result: crate::search::index::SymbolSearchResult) -> Symbol {
    Symbol {
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
    }
}

/// Enrich symbols with code_context and visibility from a SQLite database.
fn enrich_symbols_from_db(symbols: &mut [Symbol], db: &crate::database::SymbolDatabase) {
    let ids: Vec<String> = symbols.iter().map(|s| s.id.clone()).collect();
    if ids.is_empty() {
        return;
    }
    match db.get_symbols_by_ids(&ids) {
        Ok(db_symbols) => {
            let enrichment_map: std::collections::HashMap<String, _> = db_symbols
                .into_iter()
                .map(|s| (s.id, (s.code_context, s.visibility)))
                .collect();
            for symbol in symbols.iter_mut() {
                if let Some((ctx, vis)) = enrichment_map.get(&symbol.id) {
                    symbol.code_context = ctx.clone();
                    symbol.visibility = vis.clone();
                }
            }
            debug!("✅ Enriched {} symbols from SQLite", enrichment_map.len());
        }
        Err(e) => {
            debug!("Could not enrich code_context from SQLite: {}", e);
        }
    }
}

/// Convert a ContentSearchResult into a Symbol (file-level match).
fn content_result_to_symbol(result: crate::search::index::ContentSearchResult) -> Symbol {
    Symbol {
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
    }
}
