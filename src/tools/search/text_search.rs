//! Text-based search using Tantivy with code-aware tokenization.

use anyhow::Result;
use std::collections::HashMap;
use tracing::{debug, info, warn};

use super::query::matches_glob_pattern;
use super::target::SearchTarget;
use crate::analysis::test_roles;
use crate::extractors::{Symbol, SymbolKind};
use crate::handler::JulieServerHandler;
use crate::search::SearchFilter;
use crate::search::query_parse::parse_query;
use crate::search::reranker::{Candidate, rerank_content_score, rerank_symbol_score};
use crate::search::scoring::{
    DOC_LANGUAGES, apply_centrality_boost, apply_language_affinity_prior, apply_nl_path_prior,
    classify_role, compute_dominant_language, is_test_path, promote_exact_name_matches,
    test_subrole,
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

    // Target workspace: pooled DB (read-only) + SearchIndex via handler helpers.
    if let Some(target_id) = target_workspace_id {
        let target_embedding_provider = handler.embedding_provider().await;
        let pooled_db = handler
            .get_pooled_database_for_workspace(&target_id)
            .await?;
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

            match search_target_clone {
                SearchTarget::Definitions => definition_search_with_index(
                    &query_clone,
                    &filter,
                    limit_usize,
                    &index,
                    Some(&pooled_db),
                    target_embedding_provider.as_deref(),
                ),
                SearchTarget::Content => content_search_with_index(
                    &query_clone,
                    &filter,
                    limit_usize,
                    &index,
                    Some(&pooled_db),
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

    // Primary workspace: pooled DB (read-only) + SearchIndex via handler helpers.
    // Mirrors the target-workspace branch above. Holds NO mutex across spawn_blocking;
    // the previous version locked the legacy Arc<Mutex<SymbolDatabase>> for the full
    // search body, serializing every primary fast_search and defeating pool concurrency.
    let (pooled_db, search_index_clone, embedding_provider) = {
        let (db, search_index) = handler.primary_pooled_database_and_search_index().await?;
        (db, search_index, handler.embedding_provider().await)
    };

    let results = tokio::task::spawn_blocking(move || -> Result<(Vec<Symbol>, bool, usize)> {
        let index = search_index_clone
            .lock()
            .map_err(|e| anyhow::anyhow!("Search index lock error: {}", e))?;

        match search_target_clone {
            SearchTarget::Definitions => definition_search_with_index(
                &query_clone,
                &filter,
                limit_usize,
                &index,
                Some(&pooled_db),
                embedding_provider.as_deref(),
            ),
            SearchTarget::Content => content_search_with_index(
                &query_clone,
                &filter,
                limit_usize,
                &index,
                Some(&pooled_db),
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

// ---------------------------------------------------------------------------
// C.4: reranker wiring
// ---------------------------------------------------------------------------

/// True when the reranker is enabled for this process.
///
/// C.5: default-on. Set `JULIE_RERANKER_ENABLED=0` (or `false` / `FALSE`)
/// to disable for diagnostic comparison. Sign-off evidence in
/// `9848748f` (C.4): 33/35 dogfood pass with the flag ON, identical to
/// flag-OFF baseline. The 10-query test-helper-discoverability fixture
/// also passes with flag ON.
fn reranker_enabled() -> bool {
    !matches!(
        std::env::var("JULIE_RERANKER_ENABLED").as_deref(),
        Ok("0") | Ok("false") | Ok("FALSE")
    )
}

fn metadata_test_role(symbol: Option<&Symbol>) -> Option<String> {
    symbol
        .and_then(|symbol| symbol.metadata.as_ref())
        .and_then(|metadata| metadata.get("test_role"))
        .and_then(|value| value.as_str())
        .filter(|role| !role.is_empty())
        .map(str::to_string)
}

/// Build a [`Candidate`] from a Tantivy [`SymbolSearchResult`].
///
/// `body` is signature + doc_comment because `code_body` is indexed but
/// not returned in SymbolSearchResult — the reranker's body-term boost
/// matches against what we have. If dogfood shows we need code_body, we
/// add it to SymbolSearchResult.
fn build_symbol_candidate(
    r: &crate::search::index::SymbolSearchResult,
    db_symbol: Option<&Symbol>,
) -> Candidate {
    let kind = SymbolKind::try_from_string(&r.kind).unwrap_or(SymbolKind::Variable);
    let metadata_is_test = db_symbol.is_some_and(test_roles::is_test_related);
    // Prefer the C.3-enriched Tantivy-stored role/test_role; fall back to
    // path-derived values when the result didn't come from Tantivy (KNN/
    // task-signal/rescue paths populate these from `classify_role` themselves,
    // so the fallback also handles them correctly).
    let stored_role = if r.role.is_empty() {
        classify_role(&r.file_path, &r.language).to_string()
    } else {
        r.role.clone()
    };
    let role = if metadata_is_test {
        "test".to_string()
    } else {
        stored_role
    };
    let test_role = metadata_test_role(db_symbol).unwrap_or_else(|| {
        if r.test_role.is_empty() {
            test_subrole(&r.file_path).to_string()
        } else {
            r.test_role.clone()
        }
    });
    let is_test = metadata_is_test || role == "test";
    let is_file_doc = role == "docs";
    let is_source_language = !DOC_LANGUAGES.contains(&r.language.as_str());

    let mut body = String::with_capacity(r.signature.len() + r.doc_comment.len() + 1);
    body.push_str(&r.signature);
    if !r.signature.is_empty() && !r.doc_comment.is_empty() {
        body.push(' ');
    }
    body.push_str(&r.doc_comment);

    Candidate::builder()
        .title(r.name.clone())
        .path(r.file_path.clone())
        .body(body)
        .kind(kind)
        .role(role)
        .test_role(test_role)
        .is_test(is_test)
        .is_file_doc(is_file_doc)
        .is_source_language(is_source_language)
        .tantivy_score(r.score)
        .build()
}

/// Reweight Tantivy symbol results in place per the C.3 reranker, then
/// re-sort by the new score (descending, stable on ties).
///
/// No-op when the reranker flag is off or the list is empty.
fn apply_reranker_to_symbol_results(
    query: &str,
    results: &mut Vec<crate::search::index::SymbolSearchResult>,
    db: Option<&crate::database::SymbolDatabase>,
) {
    if !reranker_enabled() || results.is_empty() {
        return;
    }
    let parsed = parse_query(query);
    let symbols_by_id: HashMap<String, Symbol> = db
        .map(|db| {
            let ids: Vec<String> = results.iter().map(|r| r.id.clone()).collect();
            db.get_symbols_by_ids(&ids).map(|symbols| {
                symbols
                    .into_iter()
                    .map(|symbol| (symbol.id.clone(), symbol))
                    .collect()
            })
        })
        .transpose()
        .unwrap_or_else(|e| {
            warn!("Could not enrich reranker candidates from SQLite metadata: {e}");
            None
        })
        .unwrap_or_default();

    // Build candidates first so the I4 two-pass intent check can scan the
    // full batch before any scoring. Without this, a `Symbol(K)` query whose
    // batch contains no `kind == K` candidate would still apply the intent
    // boost to partial-name same-kind candidates, promoting them above
    // exact-name wrong-kind matches.
    let candidates: Vec<Candidate> = results
        .iter()
        .map(|r| build_symbol_candidate(r, symbols_by_id.get(&r.id)))
        .collect();

    let effective_query = effective_intent_query(&parsed, &candidates);

    for (r, candidate) in results.iter_mut().zip(candidates.iter()) {
        r.score = rerank_symbol_score(&effective_query, candidate);
    }
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.file_path.cmp(&b.file_path))
    });
}

/// Mirror of the in-batch intent downgrade that `reranker::rerank` performs
/// internally. We need the same decision here because this rescorer calls
/// `rerank_symbol_score` per-candidate (so the in-place sort is stable
/// without re-cloning the results vec), and per-candidate scoring can't
/// observe the rest of the batch.
fn effective_intent_query(
    query: &crate::search::query_parse::ParsedQuery,
    candidates: &[Candidate],
) -> crate::search::query_parse::ParsedQuery {
    use crate::search::query_parse::QueryIntent;
    let requested_kind = match &query.intent {
        QueryIntent::Symbol(k) => k.clone(),
        _ => return query.clone(),
    };

    let any_realizes = candidates.iter().any(|c| {
        c.kind == requested_kind && {
            let title_lc = c.title.to_lowercase();
            query.target_terms.iter().any(|t| title_lc.contains(t))
        }
    });

    if any_realizes {
        query.clone()
    } else {
        let mut downgraded = query.clone();
        downgraded.intent = QueryIntent::Free;
        downgraded
    }
}

/// File-level reranker for content search. Same flag, leaner inputs:
/// content results are file-level so `title` is the basename, `body` is
/// empty, and `kind` is the sentinel `Module`. The reranker mostly
/// contributes role/path reweighting here — useful for de-emphasizing
/// vendor / generated / test files on NL content queries.
pub(crate) fn apply_reranker_to_content_results(
    query: &str,
    results: &mut Vec<crate::search::index::ContentSearchResult>,
) {
    if !reranker_enabled() || results.is_empty() {
        return;
    }
    let parsed = parse_query(query);
    for r in results.iter_mut() {
        let basename = std::path::Path::new(&r.file_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&r.file_path)
            .to_string();
        let role = classify_role(&r.file_path, &r.language).to_string();
        let test_role = test_subrole(&r.file_path).to_string();
        let is_test = role == "test";
        let is_file_doc = role == "docs";
        let is_source_language = !DOC_LANGUAGES.contains(&r.language.as_str());

        let candidate = Candidate::builder()
            .title(basename)
            .path(r.file_path.clone())
            .body(String::new())
            .kind(SymbolKind::Module)
            .role(role)
            .test_role(test_role)
            .is_test(is_test)
            .is_file_doc(is_file_doc)
            .is_source_language(is_source_language)
            .tantivy_score(r.score)
            .build();
        // Content scoring path: empty body, Module sentinel kind. Use the
        // content-specific scorer so phrase/body/intent/kind boosts that
        // can never meaningfully fire are skipped at the type-level, not
        // just suppressed by happenstance.
        r.score = rerank_content_score(&parsed, &candidate);
    }
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.file_path.cmp(&b.file_path))
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
    let annotation_query = crate::search::query::parse_annotation_query(query);
    let has_annotation_filters = annotation_query.has_annotation_filters();
    let use_hybrid = !has_annotation_filters
        && crate::search::scoring::is_nl_like_query(query)
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

        // C2: rescale RRF base scores into the same order of magnitude as
        // BM25 (Tantivy AND/OR path scores live in ~1-30; RRF scores live
        // in ~0-0.1). Without this, the additive reranker boosts (10-460)
        // swamp the RRF base by ~3 orders of magnitude, making the
        // keyword/semantic merge functionally irrelevant on NL queries.
        // Multiplying by `RRF_TO_BM25_SCALE = 200` brings RRF top-rank
        // (~0.016 for weight=1.0) to ~3, and a hot multi-hit RRF score
        // (~0.05) to ~10, matching BM25 mid-rank. Centrality + reranker
        // boosts then layer on a base that can actually discriminate.
        const RRF_TO_BM25_SCALE: f32 = 200.0;
        for r in hybrid_results.results.iter_mut() {
            r.score *= RRF_TO_BM25_SCALE;
        }

        // Centrality is part of the base score (multiplicative on the
        // now-scaled base). The reranker then adds intent/title/role
        // boosts on top, so graph popularity cannot multiply those
        // additive bonuses.
        let symbol_ids: Vec<&str> = hybrid_results
            .results
            .iter()
            .map(|r| r.id.as_str())
            .collect();
        if let Ok(ref_scores) = db.get_reference_scores(&symbol_ids) {
            apply_centrality_boost(&mut hybrid_results.results, &ref_scores);
        }
        apply_reranker_to_symbol_results(query, &mut hybrid_results.results, Some(db));
        // NL path prior: demote test/docs/fixture paths for natural-language
        // queries so production code wins on "how does X work"-style asks.
        // No-op for identifier-like queries.
        apply_nl_path_prior(&mut hybrid_results.results, query);
        // Language affinity: demote foreign-language candidates when one
        // language dominates the workspace. No-op on mixed repos.
        let dominant_lang = compute_dominant_language(db);
        apply_language_affinity_prior(
            &mut hybrid_results.results,
            dominant_lang.as_deref(),
            query,
        );
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

        // Centrality is part of the base score. The reranker then adds
        // intent/title/role boosts on top, so graph popularity cannot
        // multiply those additive bonuses.
        if let Some(db) = db {
            let symbol_ids: Vec<&str> = filtered_results.iter().map(|r| r.id.as_str()).collect();
            if let Ok(ref_scores) = db.get_reference_scores(&symbol_ids) {
                apply_centrality_boost(&mut filtered_results, &ref_scores);
            }
        }
        apply_reranker_to_symbol_results(query, &mut filtered_results, db);
        apply_nl_path_prior(&mut filtered_results, query);
        let dominant_lang = db.and_then(compute_dominant_language);
        apply_language_affinity_prior(&mut filtered_results, dominant_lang.as_deref(), query);
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
        let all_filtered_ids: std::collections::HashSet<String> =
            symbols.iter().map(|s| s.id.clone()).collect();
        let mut pre_trunc = symbols.len();
        symbols.truncate(limit);

        // LAST STEP: Prepend high-centrality definitions from SQLite that the
        // Tantivy pipeline missed or buried. This handles qualified names like
        // "Phoenix.Router" when an agent searches just "Router".
        // Runs AFTER truncation so these are guaranteed to appear in the output.
        if let Some(db) = db.filter(|_| !has_annotation_filters) {
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
                    let additional_rescue_total = db_defs
                        .iter()
                        .filter(|s| !all_filtered_ids.contains(&s.id))
                        .count();
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
                            let role = crate::search::scoring::classify_role(
                                &s.file_path,
                                &s.language,
                            );
                            let test_role = crate::search::scoring::test_subrole(&s.file_path);
                            let sym = tantivy_symbol_to_symbol(
                                crate::search::index::SymbolSearchResult {
                                    id: s.id,
                                    name: s.name,
                                    signature: s.signature.unwrap_or_default(),
                                    doc_comment: s.doc_comment.unwrap_or_default(),
                                    file_path: s.file_path,
                                    // Use Display (snake_case via SymbolKind's fmt::Display impl),
                                    // not Debug+lowercase — the latter produces "enummember" for
                                    // SymbolKind::EnumMember while try_from_string expects
                                    // "enum_member", silently degrading rescued enum members to
                                    // SymbolKind::Variable. (Caught by Codex adversarial review.)
                                    kind: s.kind.to_string(),
                                    language: s.language,
                                    start_line: s.start_line,
                                    score,
                                    role: role.to_string(),
                                    test_role: test_role.to_string(),
                                    capability_flags: String::new(),
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
                        pre_trunc = pre_trunc
                            .saturating_add(additional_rescue_total)
                            .max(prepend.len().saturating_add(symbols.len()));
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

#[cfg(test)]
pub(crate) fn definition_search_with_index_for_test(
    query: &str,
    filter: &SearchFilter,
    limit: usize,
    index: &crate::search::index::SearchIndex,
    db: Option<&crate::database::SymbolDatabase>,
) -> Result<(Vec<Symbol>, bool, usize)> {
    definition_search_with_index(query, filter, limit, index, db, None)
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
    let mut search_results = content_search.results;
    // C.4: rerank file-level results BEFORE the verification loop so the
    // limit-truncation downstream picks role-reweighted candidates.
    // No-op when JULIE_RERANKER_ENABLED is unset.
    apply_reranker_to_content_results(query, &mut search_results);
    let candidate_total = search_results.len();

    let query_words: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| w.to_lowercase())
        .collect();
    let mut verified_symbols = Vec::with_capacity(limit);

    if let Some(db) = db {
        let mut filtered_results = Vec::with_capacity(search_results.len());
        for result in search_results {
            if let Some(ref pattern) = filter.file_pattern {
                if !matches_glob_pattern(&result.file_path, pattern) {
                    continue;
                }
            }
            filtered_results.push(result);
        }

        let file_paths: Vec<String> = filtered_results
            .iter()
            .map(|result| result.file_path.clone())
            .collect();
        match db.get_file_contents_by_paths(&file_paths) {
            Ok(contents_by_path) => {
                for result in filtered_results {
                    if verified_symbols.len() >= limit {
                        break;
                    }
                    let Some(content) = contents_by_path.get(&result.file_path) else {
                        verified_symbols.push(content_result_to_symbol(result));
                        continue;
                    };
                    let Some(content) = content else {
                        verified_symbols.push(content_result_to_symbol(result));
                        continue;
                    };

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
            }
            Err(e) => {
                debug!("Could not verify batched content search results: {}", e);
                for result in filtered_results.into_iter().take(limit) {
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
    // Resolve the kind before consuming `result` fields into the struct literal.
    // `try_from_string` returns None for unrecognised strings (e.g. from a
    // corrupt or schema-evolved Tantivy row) — degrade to Variable instead of
    // panicking and taking down the search request.
    let kind = SymbolKind::try_from_string(&result.kind).unwrap_or_else(|| {
        tracing::warn!(
            "unknown SymbolKind {:?} in tantivy row {}; degrading to Variable",
            result.kind,
            result.id
        );
        SymbolKind::Variable
    });
    Symbol {
        id: result.id,
        name: result.name,
        kind,
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
        body_span: None,
        body_hash: None,
        annotations: Vec::new(),
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
                .map(|s| {
                    (
                        s.id,
                        (
                            s.code_context,
                            s.visibility,
                            s.metadata,
                            s.body_span,
                            s.body_hash,
                        ),
                    )
                })
                .collect();
            for symbol in symbols.iter_mut() {
                if let Some((ctx, vis, meta, body_span, body_hash)) = enrichment_map.get(&symbol.id)
                {
                    symbol.code_context = ctx.clone();
                    symbol.visibility = vis.clone();
                    symbol.metadata = meta.clone();
                    symbol.body_span = *body_span;
                    symbol.body_hash = body_hash.clone();
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
        body_span: None,
        body_hash: None,
        annotations: Vec::new(),
    }
}
