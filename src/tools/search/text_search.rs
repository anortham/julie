//! Text-based search using Tantivy with code-aware tokenization.

use anyhow::Result;
use tracing::{debug, info, warn};

use super::query::matches_glob_pattern;
use super::target::SearchTarget;
use crate::extractors::{Symbol, SymbolKind};
use crate::handler::JulieServerHandler;
use crate::search::SearchFilter;
use crate::search::scoring::{
    DOC_LANGUAGES, apply_centrality_boost, is_test_path, promote_exact_name_matches,
};

// Re-export for tests
#[cfg(test)]
pub(crate) use super::nl_embeddings::take_nl_definition_embedding_init_attempts;

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
    exclude_tests: Option<bool>,
    handler: &JulieServerHandler,
) -> Result<(Vec<Symbol>, bool, usize)> {
    let search_target = SearchTarget::parse(search_target)?;
    super::nl_embeddings::maybe_initialize_embeddings_for_nl_definitions(
        query,
        search_target.canonical_name(),
        handler,
    )
    .await;

    let current_primary_id = handler.current_workspace_id();
    let loaded_workspace_id = handler.loaded_workspace_id();

    // Determine if we're targeting an explicit non-primary workspace.
    let target_workspace_id = if let Some(ref ids) = workspace_ids {
        if let Some(id) = ids.first() {
            let loaded_startup_without_primary =
                current_primary_id.is_none() && loaded_workspace_id.as_ref() == Some(id);

            if loaded_startup_without_primary || current_primary_id.as_ref() != Some(id) {
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
        "🔍 Tantivy text search: '{}' (target: {}, workspace: {:?})",
        query,
        search_target.canonical_name(),
        target_workspace_id
    );

    // Resolve exclude_tests smart default
    let exclude_tests_resolved = exclude_tests.unwrap_or_else(|| {
        // NL queries auto-exclude tests; definition searches always include them
        search_target == SearchTarget::Content && crate::search::scoring::is_nl_like_query(query)
    });

    let filter = SearchFilter {
        language: language.clone(),
        kind: None,
        file_pattern: file_pattern.clone(),
        exclude_tests: exclude_tests_resolved,
    };

    let query_clone = query.to_string();
    let limit_usize = limit as usize;
    let search_target_clone = search_target;

    // Target workspace: use handler helpers for DB + SearchIndex access.
    if let Some(target_id) = target_workspace_id {
        let target_embedding_provider = handler.embedding_provider().await;
        let db_arc = handler.get_database_for_workspace(&target_id).await?;
        let si_arc = handler.get_search_index_for_workspace(&target_id).await?;

        let results = tokio::task::spawn_blocking(move || -> Result<(Vec<Symbol>, bool, usize)> {
            let si_arc = match si_arc {
                Some(si) => si,
                None => {
                    debug!("No search index for target workspace, returning empty");
                    return Ok((Vec::new(), false, 0));
                }
            };
            let index = si_arc
                .lock()
                .map_err(|e| anyhow::anyhow!("Search index lock error: {}", e))?;
            let db_lock = db_arc
                .lock()
                .map_err(|e| anyhow::anyhow!("Database lock error: {}", e))?;

            match search_target_clone {
                SearchTarget::Definitions => definition_search_with_index(
                    &query_clone,
                    &filter,
                    limit_usize,
                    &index,
                    Some(&db_lock),
                    target_embedding_provider.as_deref(),
                ),
                SearchTarget::Content => content_search_with_index(
                    &query_clone,
                    &filter,
                    limit_usize,
                    &index,
                    Some(&db_lock),
                ),
                SearchTarget::Files => {
                    anyhow::bail!("search_target=\"files\" is not implemented yet")
                }
            }
        })
        .await??;

        return Ok(post_filter_results(
            results,
            file_pattern,
            "Target workspace",
        ));
    }

    // Primary workspace: use the current-primary DB/search store. When current primary
    // differs from the loaded workspace, route through handler helpers instead of the
    // stale loaded workspace object.
    let (search_index_clone, db_clone, embedding_provider) = {
        let (db, search_index) = handler.primary_database_and_search_index().await?;
        (search_index, db, handler.embedding_provider().await)
    };

    let results = tokio::task::spawn_blocking(move || -> Result<(Vec<Symbol>, bool, usize)> {
        let index = search_index_clone.lock().unwrap_or_else(|p| p.into_inner());
        let db_guard = db_clone.lock().unwrap_or_else(|poisoned| {
            warn!("Database mutex poisoned, recovering");
            poisoned.into_inner()
        });

        match search_target_clone {
            SearchTarget::Definitions => definition_search_with_index(
                &query_clone,
                &filter,
                limit_usize,
                &index,
                Some(&db_guard),
                embedding_provider.as_deref(),
            ),
            SearchTarget::Content => content_search_with_index(
                &query_clone,
                &filter,
                limit_usize,
                &index,
                Some(&db_guard),
            ),
            SearchTarget::Files => anyhow::bail!("search_target=\"files\" is not implemented yet"),
        }
    })
    .await??;

    Ok(post_filter_results(results, file_pattern, "Tantivy"))
}

/// Defense-in-depth post-filter by file_pattern and log result count.
fn post_filter_results(
    (results, relaxed, pre_trunc): (Vec<Symbol>, bool, usize),
    file_pattern: &Option<String>,
    label: &str,
) -> (Vec<Symbol>, bool, usize) {
    let filtered = if let Some(pattern) = file_pattern {
        results
            .into_iter()
            .filter(|s| matches_glob_pattern(&s.file_path, pattern))
            .collect()
    } else {
        results
    };
    debug!(
        "✅ {} search returned {} results (after filtering)",
        label,
        filtered.len()
    );
    (filtered, relaxed, pre_trunc)
}

// ---------------------------------------------------------------------------
// Shared search helpers (used by both explicit-workspace and primary paths)
// ---------------------------------------------------------------------------

/// Remove test symbols when `exclude` is `true`.
///
/// Uses two complementary mechanisms:
/// 1. `metadata["is_test"]` — set by extractors for annotated test functions
///    (e.g. Rust `#[test]`, TypeScript `it()`/`test()` runner calls).
/// 2. `is_test_path()` — path-based fallback for non-function symbols in test
///    files (interfaces, types, classes in `.test.ts`, `tests/`, etc.) that
///    extractors do not annotate with `is_test`.
fn filter_test_symbols(symbols: &mut Vec<Symbol>, exclude: bool) {
    if !exclude {
        return;
    }
    symbols.retain(|s| {
        let is_test_by_metadata = s
            .metadata
            .as_ref()
            .and_then(|m| m.get("is_test"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        !(is_test_by_metadata || is_test_path(&s.file_path))
    });
}

/// Run a definition search: hybrid (keyword + semantic) if NL query with embeddings,
/// otherwise pure keyword with over-fetch + exact-name promotion.
///
/// Returns `(symbols, relaxed, pre_truncation_total)` where `pre_truncation_total` is
/// the candidate count before applying the limit — used for "showing X of Y" reporting.
fn definition_search_with_index(
    query: &str,
    filter: &SearchFilter,
    limit: usize,
    index: &crate::search::index::SearchIndex,
    db: Option<&crate::database::SymbolDatabase>,
    embedding_provider: Option<&dyn crate::embeddings::EmbeddingProvider>,
) -> Result<(Vec<Symbol>, bool, usize)> {
    let use_hybrid = crate::search::scoring::is_nl_like_query(query)
        && embedding_provider.is_some()
        && db.is_some();

    if use_hybrid {
        info!(
            "🔍 Hybrid search (keyword + semantic) for NL query: '{}'",
            query
        );
    } else {
        debug!(
            "🔍 Keyword-only definition search for: '{}' (is_nl={}, has_embeddings={}, has_db={})",
            query,
            crate::search::scoring::is_nl_like_query(query),
            embedding_provider.is_some(),
            db.is_some()
        );
    }

    if use_hybrid {
        let db = db.expect("checked is_some above");
        let profile = crate::search::weights::SearchWeightProfile::fast_search();
        let mut hybrid_results = crate::search::hybrid::hybrid_search(
            query,
            filter,
            limit,
            index,
            db,
            embedding_provider,
            Some(profile),
        )?;
        let relaxed = hybrid_results.relaxed;

        // Apply centrality boost + exact-name promotion + truncate
        let symbol_ids: Vec<&str> = hybrid_results
            .results
            .iter()
            .map(|r| r.id.as_str())
            .collect();
        if let Ok(ref_scores) = db.get_reference_scores(&symbol_ids) {
            apply_centrality_boost(&mut hybrid_results.results, &ref_scores);
        }
        promote_exact_name_matches(&mut hybrid_results.results, query);

        let mut symbols: Vec<Symbol> = hybrid_results
            .results
            .into_iter()
            .map(tantivy_symbol_to_symbol)
            .collect();
        enrich_symbols_from_db(&mut symbols, db);

        // Filter BEFORE truncating so test symbols don't consume limit slots
        filter_test_symbols(&mut symbols, filter.exclude_tests);
        let pre_trunc = symbols.len();
        symbols.truncate(limit);

        Ok((symbols, relaxed, pre_trunc))
    } else {
        // Keyword search: over-fetch so exact-name definitions aren't lost.
        // For definition search, we need a large window because:
        // 1. Qualified names (Phoenix.Router) rank low in BM25 — the definition
        //    mentions the name once vs. many reference files mentioning it repeatedly.
        // 2. Centrality boost + name promotion can rescue buried definitions,
        //    but only if they're in the candidate pool.
        let tantivy_limit = if filter.file_pattern.is_some() {
            limit.saturating_mul(50).max(500).min(5000)
        } else {
            limit.saturating_mul(20).max(500)
        };
        let search = index.search_symbols(query, filter, tantivy_limit)?;
        let relaxed = search.relaxed;

        // Apply file_pattern filter before centrality boost
        let mut filtered_results: Vec<_> = if let Some(ref pattern) = filter.file_pattern {
            search
                .results
                .into_iter()
                .filter(|r| matches_glob_pattern(&r.file_path, pattern))
                .collect()
        } else {
            search.results
        };

        // Apply centrality boost + exact-name promotion on Tantivy results
        if let Some(db) = db {
            let symbol_ids: Vec<&str> = filtered_results.iter().map(|r| r.id.as_str()).collect();
            if let Ok(ref_scores) = db.get_reference_scores(&symbol_ids) {
                apply_centrality_boost(&mut filtered_results, &ref_scores);
            }
        }
        promote_exact_name_matches(&mut filtered_results, query);

        let mut symbols: Vec<Symbol> = filtered_results
            .into_iter()
            .map(tantivy_symbol_to_symbol)
            .collect();
        if let Some(db) = db {
            enrich_symbols_from_db(&mut symbols, db);
        }

        // Filter BEFORE truncating so test symbols don't consume limit slots
        filter_test_symbols(&mut symbols, filter.exclude_tests);
        let pre_trunc = symbols.len();
        symbols.truncate(limit);

        // LAST STEP: Prepend high-centrality definitions from SQLite that the
        // Tantivy pipeline missed or buried. This handles qualified names like
        // "Phoenix.Router" when an agent searches just "Router".
        // Runs AFTER truncation so these are guaranteed to appear in the output.
        if let Some(db) = db {
            match db.find_definitions_by_name_component(query, filter.language.as_deref(), 5) {
                Err(e) => {
                    tracing::warn!(
                        "find_definitions_by_name_component('{}') error: {}",
                        query,
                        e
                    );
                }
                Ok(db_defs) => {
                    let existing_ids: std::collections::HashSet<String> =
                        symbols.iter().map(|s| s.id.clone()).collect();
                    let candidates: Vec<_> = db_defs
                        .into_iter()
                        .filter(|s| !existing_ids.contains(&s.id))
                        // Don't prepend doc-language or test-file definitions — they're
                        // never the "rescued high-centrality definitions" this step targets,
                        // and prepending them overrides promote_exact_name_matches sorting.
                        .filter(|s| {
                            !DOC_LANGUAGES.contains(&s.language.as_str())
                                && !is_test_path(&s.file_path)
                        })
                        .collect();
                    // Fetch actual reference_scores so confidence reflects real centrality
                    // rather than a hardcoded sentinel that misrepresents ranking.
                    let id_refs: Vec<&str> = candidates.iter().map(|s| s.id.as_str()).collect();
                    let ref_scores = db.get_reference_scores(&id_refs).unwrap_or_default();
                    let mut prepend: Vec<Symbol> = candidates
                        .into_iter()
                        .map(|s| {
                            let score =
                                ref_scores.get(&s.id).copied().unwrap_or(1.0).max(1.0) as f32;
                            let sym = tantivy_symbol_to_symbol(
                                crate::search::index::SymbolSearchResult {
                                    id: s.id,
                                    name: s.name,
                                    signature: s.signature.unwrap_or_default(),
                                    doc_comment: s.doc_comment.unwrap_or_default(),
                                    file_path: s.file_path,
                                    kind: format!("{:?}", s.kind).to_lowercase(),
                                    language: s.language,
                                    start_line: s.start_line,
                                    score,
                                },
                            );
                            // Enrich from DB for code_context etc.
                            let mut single = vec![sym];
                            enrich_symbols_from_db(&mut single, db);
                            single.remove(0)
                        })
                        .collect();
                    if !prepend.is_empty() {
                        // Apply same test filter as main results
                        filter_test_symbols(&mut prepend, filter.exclude_tests);
                        prepend.append(&mut symbols);
                        symbols = prepend;
                        symbols.truncate(limit);
                    }
                }
            }
        }

        Ok((symbols, relaxed, pre_trunc))
    }
}

/// Run a content search with post-verification against actual file content.
///
/// Returns `(symbols, relaxed, pre_truncation_total)`.
fn content_search_with_index(
    query: &str,
    filter: &SearchFilter,
    limit: usize,
    index: &crate::search::index::SearchIndex,
    db: Option<&crate::database::SymbolDatabase>,
) -> Result<(Vec<Symbol>, bool, usize)> {
    debug!("🔍 Searching content with Tantivy");

    let fetch_limit = if filter.file_pattern.is_some() {
        limit.saturating_mul(100).max(500).min(1000)
    } else {
        limit.saturating_mul(5).max(50)
    };
    let content_search = index.search_content(query, filter, fetch_limit)?;
    let relaxed = content_search.relaxed;
    let search_results = content_search.results;
    let candidate_total = search_results.len();

    let query_words: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| w.to_lowercase())
        .collect();
    let mut verified_symbols = Vec::with_capacity(limit);

    if let Some(db) = db {
        for result in search_results {
            if verified_symbols.len() >= limit {
                break;
            }
            if let Some(ref pattern) = filter.file_pattern {
                if !matches_glob_pattern(&result.file_path, pattern) {
                    continue;
                }
            }
            match db.get_file_content(&result.file_path) {
                Ok(Some(content)) => {
                    let content_lower = content.to_lowercase();
                    if query_words
                        .iter()
                        .all(|word| content_lower.contains(word.as_str()))
                    {
                        verified_symbols.push(content_result_to_symbol(result));
                    } else {
                        debug!(
                            "Filtered false positive: {} (missing query words for '{}')",
                            result.file_path, query
                        );
                    }
                }
                Ok(None) => {
                    verified_symbols.push(content_result_to_symbol(result));
                }
                Err(e) => {
                    debug!("Could not verify content for {}: {}", result.file_path, e);
                    verified_symbols.push(content_result_to_symbol(result));
                }
            }
        }
    } else {
        debug!("No database available for content verification, returning unverified results");
        for result in search_results.into_iter().take(limit) {
            verified_symbols.push(content_result_to_symbol(result));
        }
    }

    Ok((verified_symbols, relaxed, candidate_total))
}

// ---------------------------------------------------------------------------
// Result conversion helpers
// ---------------------------------------------------------------------------

/// Convert a Tantivy SymbolSearchResult into an extractors Symbol.
pub(crate) fn tantivy_symbol_to_symbol(result: crate::search::index::SymbolSearchResult) -> Symbol {
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

/// Enrich symbols with code_context, visibility, and metadata from a SQLite database.
fn enrich_symbols_from_db(symbols: &mut [Symbol], db: &crate::database::SymbolDatabase) {
    let ids: Vec<String> = symbols.iter().map(|s| s.id.clone()).collect();
    if ids.is_empty() {
        return;
    }
    match db.get_symbols_by_ids(&ids) {
        Ok(db_symbols) => {
            let enrichment_map: std::collections::HashMap<String, _> = db_symbols
                .into_iter()
                .map(|s| (s.id, (s.code_context, s.visibility, s.metadata)))
                .collect();
            for symbol in symbols.iter_mut() {
                if let Some((ctx, vis, meta)) = enrichment_map.get(&symbol.id) {
                    symbol.code_context = ctx.clone();
                    symbol.visibility = vis.clone();
                    symbol.metadata = meta.clone();
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
pub(crate) fn content_result_to_symbol(
    result: crate::search::index::ContentSearchResult,
) -> Symbol {
    Symbol {
        id: format!("content_{}", result.file_path.replace(['/', '\\'], "_")),
        name: result.file_path.clone(),
        kind: SymbolKind::Module,
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
