//! Text-based search using Tantivy with code-aware tokenization.

use anyhow::Result;
use std::sync::LazyLock;
use tracing::{debug, info, warn};

use super::query::matches_glob_pattern;
use crate::extractors::{Symbol, SymbolKind};
use crate::handler::JulieServerHandler;
use crate::search::scoring::{apply_centrality_boost, promote_exact_name_matches};
use crate::search::{SearchFilter, SearchIndex};

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
pub(crate) fn take_nl_definition_embedding_init_attempts(workspace_root: &std::path::Path) -> usize {
    let mut attempts = NL_DEFINITION_EMBEDDING_INIT_ATTEMPTS
        .lock()
        .expect("nl definition init attempt map mutex poisoned");
    attempts.remove(workspace_root).unwrap_or(0)
}

/// Text search with workspace filtering and search target selection.
///
/// - `"definitions"`: Symbol search via Tantivy with 5x over-fetch + exact-name promotion.
/// - `"content"`: File content search with post-verification against SQLite to eliminate
///   false positives from CodeTokenizer over-splitting.
///
/// Returns `(symbols, relaxed)` where `relaxed` = true on AND→OR fallback.
pub async fn text_search_impl(
    query: &str,
    language: &Option<String>,
    file_pattern: &Option<String>,
    limit: u32,
    workspace_ids: Option<Vec<String>>,
    search_target: &str,
    _context_lines: Option<u32>,
    handler: &JulieServerHandler,
) -> Result<(Vec<Symbol>, bool)> {
    maybe_initialize_embeddings_for_nl_definitions(query, search_target, handler).await;

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
        let ref_embedding_provider = workspace.embedding_provider.clone();

        let results = tokio::task::spawn_blocking(move || -> Result<(Vec<Symbol>, bool)> {
            if !tantivy_path.join("meta.json").exists() {
                debug!("No Tantivy index for reference workspace, returning empty");
                return Ok((Vec::new(), false));
            }

            let configs = crate::search::LanguageConfigs::load_embedded();
            let index = SearchIndex::open_with_language_configs(&tantivy_path, &configs)?;

            if search_target_clone == "definitions" {
                // Check if this is an NL query and we have embeddings available
                let use_hybrid = crate::search::scoring::is_nl_like_query(&query_clone)
                    && ref_embedding_provider.is_some();

                if use_hybrid {
                    debug!("🔍 NL query on reference workspace, using hybrid search");

                    let ref_db = crate::database::SymbolDatabase::new(&ref_db_path)?;
                    let mut hybrid_results = crate::search::hybrid::hybrid_search(
                        &query_clone,
                        &filter,
                        limit_usize,
                        &index,
                        &ref_db,
                        ref_embedding_provider.as_deref(),
                    )?;
                    let relaxed = hybrid_results.relaxed;

                    // Apply centrality boost + exact-name promotion + truncate
                    let symbol_ids: Vec<&str> = hybrid_results
                        .results
                        .iter()
                        .map(|r| r.id.as_str())
                        .collect();
                    if let Ok(ref_scores) = ref_db.get_reference_scores(&symbol_ids) {
                        apply_centrality_boost(&mut hybrid_results.results, &ref_scores);
                    }
                    promote_exact_name_matches(&mut hybrid_results.results, &query_clone);
                    hybrid_results.results.truncate(limit_usize);

                    let mut symbols: Vec<Symbol> = hybrid_results
                        .results
                        .into_iter()
                        .map(|result| tantivy_symbol_to_symbol(result))
                        .collect();

                    enrich_symbols_from_db(&mut symbols, &ref_db);

                    Ok((symbols, relaxed))
                } else {
                    // Keyword search: over-fetch so exact-name definitions aren't lost
                    let tantivy_limit = if filter.file_pattern.is_some() {
                        limit_usize.saturating_mul(50).max(500).min(5000)
                    } else {
                        limit_usize.saturating_mul(10).max(200)
                    };
                    let search = index.search_symbols(&query_clone, &filter, tantivy_limit)?;
                    let relaxed = search.relaxed;

                    // Apply file_pattern filter BEFORE symbol conversion + enrichment
                    let mut filtered_results: Vec<_> =
                        if let Some(ref pattern) = filter.file_pattern {
                            search
                                .results
                                .into_iter()
                                .filter(|r| matches_glob_pattern(&r.file_path, pattern))
                                .collect()
                        } else {
                            search.results
                        };

                    // Open reference workspace DB once for both centrality boost and enrichment
                    let ref_db_opt = if ref_db_path.exists() {
                        crate::database::SymbolDatabase::new(&ref_db_path).ok()
                    } else {
                        None
                    };

                    // Apply centrality boost for reference workspace
                    if let Some(ref ref_db) = ref_db_opt {
                        let symbol_ids: Vec<&str> =
                            filtered_results.iter().map(|r| r.id.as_str()).collect();
                        if let Ok(ref_scores) = ref_db.get_reference_scores(&symbol_ids) {
                            apply_centrality_boost(&mut filtered_results, &ref_scores);
                        }
                    }

                    // Promote exact name matches to the top (stable partition)
                    promote_exact_name_matches(&mut filtered_results, &query_clone);

                    // Trim back to the user's requested limit after over-fetch + promotion
                    filtered_results.truncate(limit_usize);

                    let mut symbols: Vec<Symbol> = filtered_results
                        .into_iter()
                        .map(|result| tantivy_symbol_to_symbol(result))
                        .collect();

                    // Enrich with code_context from reference workspace's SQLite
                    if let Some(ref ref_db) = ref_db_opt {
                        enrich_symbols_from_db(&mut symbols, &ref_db);
                    }

                    Ok((symbols, relaxed))
                }
            } else {
                // Content search on reference workspace
                let fetch_limit = if filter.file_pattern.is_some() {
                    limit_usize.saturating_mul(100).max(500).min(1000)
                } else {
                    limit_usize.saturating_mul(5).max(50)
                };
                let content_search = index.search_content(&query_clone, &filter, fetch_limit)?;
                let content_relaxed = content_search.relaxed;
                let search_results = content_search.results;

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

                            // Apply file_pattern filter BEFORE content verification
                            if let Some(ref pattern) = filter.file_pattern {
                                if !matches_glob_pattern(&result.file_path, pattern) {
                                    continue;
                                }
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

                Ok((verified_symbols, content_relaxed))
            }
        })
        .await??;

        let (results, relaxed) = results;

        // Defense-in-depth: post-filter by file_pattern
        // (primary filtering now happens inside the collection loops above)
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

        return Ok((filtered_results, relaxed));
    }

    // Primary workspace: use shared search index
    let search_index = workspace.search_index.as_ref().ok_or_else(|| {
        anyhow::anyhow!("Search index not initialized. Run 'manage_workspace index' first.")
    })?;

    // Clone the Arc so we can move it into spawn_blocking
    let search_index_clone = search_index.clone();

    // Clone DB for both definition search (code_context enrichment) and
    // content search (post-verification filtering)
    let db_clone = workspace.db.clone();

    // Clone embedding provider for semantic fallback (cheap Arc clone)
    let embedding_provider = workspace.embedding_provider.clone();

    // Perform the search in a blocking task since Tantivy uses std::sync::Mutex
    let results = tokio::task::spawn_blocking(move || -> Result<(Vec<Symbol>, bool)> {
        let index = search_index_clone.lock().unwrap();

        // Route based on search_target
        if search_target_clone == "definitions" {
            // Check if this is an NL query and we have embeddings available
            let use_hybrid = crate::search::scoring::is_nl_like_query(&query_clone)
                && embedding_provider.is_some();

            if use_hybrid {
                info!(
                    "🔍 Hybrid search (keyword + semantic) for NL query: '{}'",
                    query_clone
                );
            } else {
                info!(
                    "🔍 Hybrid search skipped: is_nl={}, has_embeddings={}",
                    crate::search::scoring::is_nl_like_query(&query_clone),
                    embedding_provider.is_some()
                );
            }

            if use_hybrid {
                let db_guard = db_clone
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Database not initialized"))?;
                let db_lock = match db_guard.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => {
                        warn!("Database mutex poisoned during hybrid search, recovering");
                        poisoned.into_inner()
                    }
                };

                let mut hybrid_results = crate::search::hybrid::hybrid_search(
                    &query_clone,
                    &filter,
                    limit_usize,
                    &index,
                    &db_lock,
                    embedding_provider.as_deref(),
                )?;
                let relaxed = hybrid_results.relaxed;

                // Apply same post-processing as keyword path:
                // centrality boost → exact-name promotion → truncate
                let symbol_ids: Vec<&str> = hybrid_results
                    .results
                    .iter()
                    .map(|r| r.id.as_str())
                    .collect();
                if let Ok(ref_scores) = db_lock.get_reference_scores(&symbol_ids) {
                    apply_centrality_boost(&mut hybrid_results.results, &ref_scores);
                }
                promote_exact_name_matches(&mut hybrid_results.results, &query_clone);
                hybrid_results.results.truncate(limit_usize);

                let mut symbols: Vec<Symbol> = hybrid_results
                    .results
                    .into_iter()
                    .map(|result| tantivy_symbol_to_symbol(result))
                    .collect();

                // Enrich with code_context from SQLite
                enrich_symbols_from_db(&mut symbols, &db_lock);

                Ok((symbols, relaxed))
            } else {
                debug!("🔍 Searching symbols with Tantivy");

                // Over-fetch so exact-name definitions aren't lost to higher-scoring references
                let tantivy_limit = if filter.file_pattern.is_some() {
                    limit_usize.saturating_mul(50).max(500).min(5000)
                } else {
                    limit_usize.saturating_mul(10).max(200)
                };
                let search = index.search_symbols(&query_clone, &filter, tantivy_limit)?;
                let relaxed = search.relaxed;

                // Apply file_pattern filter BEFORE symbol conversion + enrichment
                let mut filtered_results: Vec<_> = if let Some(ref pattern) = filter.file_pattern {
                    search
                        .results
                        .into_iter()
                        .filter(|r| matches_glob_pattern(&r.file_path, pattern))
                        .collect()
                } else {
                    search.results
                };

                // Apply centrality boost from graph reference scores
                if let Some(db_arc) = &db_clone {
                    let db_lock = match db_arc.lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => {
                            warn!("Database mutex poisoned during centrality boost, recovering");
                            poisoned.into_inner()
                        }
                    };
                    let symbol_ids: Vec<&str> =
                        filtered_results.iter().map(|r| r.id.as_str()).collect();
                    if let Ok(ref_scores) = db_lock.get_reference_scores(&symbol_ids) {
                        apply_centrality_boost(&mut filtered_results, &ref_scores);
                    }
                    drop(db_lock);
                }

                // Promote exact name matches to the top (stable partition)
                promote_exact_name_matches(&mut filtered_results, &query_clone);

                // Trim back to the user's requested limit after over-fetch + promotion
                filtered_results.truncate(limit_usize);

                let mut symbols: Vec<Symbol> = filtered_results
                    .into_iter()
                    .map(|result| tantivy_symbol_to_symbol(result))
                    .collect();

                // Enrich with code_context from SQLite (Tantivy doesn't store code_body)
                if let Some(db_arc) = &db_clone {
                    let db_lock = match db_arc.lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => {
                            warn!(
                                "Database mutex poisoned during code_context enrichment, recovering"
                            );
                            poisoned.into_inner()
                        }
                    };
                    enrich_symbols_from_db(&mut symbols, &db_lock);
                }

                Ok((symbols, relaxed))
            }
        } else {
            // "content" or any other value: search file content
            debug!("🔍 Searching content with Tantivy");

            // Over-fetch for post-verification (CodeTokenizer may over-split queries)
            let fetch_limit = if filter.file_pattern.is_some() {
                limit_usize.saturating_mul(100).max(500).min(1000)
            } else {
                limit_usize.saturating_mul(5).max(50)
            };
            let content_search = index.search_content(&query_clone, &filter, fetch_limit)?;
            let content_relaxed = content_search.relaxed;
            let search_results = content_search.results;

            // Post-verify: all query words must appear in file content (eliminates
            // false positives from CodeTokenizer over-splitting)
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

                    // Apply file_pattern filter BEFORE expensive content verification
                    if let Some(ref pattern) = filter.file_pattern {
                        if !matches_glob_pattern(&result.file_path, pattern) {
                            continue;
                        }
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
                            debug!("Could not verify content for {}: {}", result.file_path, e);
                            verified_symbols.push(content_result_to_symbol(result));
                        }
                    }
                }
            } else {
                // No database available — return unverified results (graceful degradation)
                debug!(
                    "No database available for content verification, returning unverified results"
                );
                for result in search_results.into_iter().take(limit_usize) {
                    verified_symbols.push(content_result_to_symbol(result));
                }
            }

            Ok((verified_symbols, content_relaxed))
        }
    })
    .await??;

    let (results, relaxed) = results;

    // Defense-in-depth: post-filter by file_pattern
    // (primary filtering now happens inside the collection loops above)
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

    Ok((filtered_results, relaxed))
}

async fn maybe_initialize_embeddings_for_nl_definitions(
    query: &str,
    search_target: &str,
    handler: &JulieServerHandler,
) {
    if search_target != "definitions" || !crate::search::scoring::is_nl_like_query(query) {
        return;
    }

    let should_attempt_init = {
        let workspace_guard = handler.workspace.read().await;
        match workspace_guard.as_ref() {
            Some(workspace) => {
                workspace.embedding_provider.is_none()
                    && workspace.embedding_runtime_status.is_none()
            }
            None => false,
        }
    };

    if !should_attempt_init {
        return;
    }

    let _single_flight_guard = NL_DEFINITION_EMBEDDING_INIT_SINGLE_FLIGHT.lock().await;

    // Double-check after acquiring the single-flight mutex: another caller may
    // have completed init while we waited.  Clone the workspace in the same
    // read-lock acquisition to avoid a redundant third lock.
    let (workspace_identity_root, workspace_for_init) = {
        let workspace_guard = handler.workspace.read().await;
        match workspace_guard.as_ref() {
            Some(workspace) => {
                if workspace.embedding_provider.is_some()
                    || workspace.embedding_runtime_status.is_some()
                {
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
    }
    if workspace.embedding_runtime_status.is_none() {
        workspace.embedding_runtime_status = initialized_runtime_status;
    }
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
