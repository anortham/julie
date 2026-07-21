//! Text-based search using Tantivy with code-aware tokenization.

use anyhow::Result;

use julie_extractors::{Symbol, SymbolKind};

use julie_context::ToolContext;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// Search-pipeline fixture for unit tests.  Wraps `search_unified` so tests
/// that previously called `definition_search_with_index` keep compiling after
/// the T9 deletion of the per-target pipeline.
///
/// Returns `(symbols, relaxed, pre_truncation_total)` for API compatibility.
/// `relaxed` is always `false` (OR fallback is internal to `search_unified`).
#[cfg(any(test, feature = "test-support"))]
pub fn definition_search_with_index_for_test(
    query: &str,
    filter: &julie_index::search::SearchFilter,
    limit: usize,
    index: &julie_index::search::index::SearchIndex,
    db: Option<&julie_core::database::SymbolDatabase>,
) -> anyhow::Result<(Vec<julie_extractors::Symbol>, bool, usize)> {
    // Route through `search_symbols` so annotation queries (`@Test`,
    // `[Authorize]`, etc.) hit the annotation-aware path that filters on
    // the `annotations_exact` indexed key.  Plain queries fall through to
    // the unified search via `search_symbols`'s own dispatcher.
    //
    // Over-fetch by 20x (matching the pre-T9 production path) so that
    // post-search boosts (NL path prior, etc.) have enough candidates to
    // rescue otherwise-buried hits before the final truncation.
    let tantivy_limit = limit.saturating_mul(20).max(500);
    let mut symbol_results = index.search_symbols(query, filter, tantivy_limit)?;
    // Apply NL path prior — production code over docs/tests for natural-
    // language queries.  Pre-T9 this lived in `definition_search_with_index`;
    // moved into the test helper so callers don't have to know about it.
    julie_index::search::scoring::apply_nl_path_prior(&mut symbol_results.results, query);
    symbol_results.results.truncate(limit);
    let mut symbols: Vec<julie_extractors::Symbol> = symbol_results
        .results
        .into_iter()
        .map(|h| {
            // SymbolSearchResult doesn't carry code_body the way UnifiedHit
            // does, but the test only asserts on `code_context` for cases
            // where the symbol is hydrated from SQLite below.
            julie_extractors::Symbol {
                id: h.id,
                name: h.name,
                kind: julie_extractors::SymbolKind::try_from_string(&h.kind)
                    .unwrap_or(julie_extractors::SymbolKind::Variable),
                language: h.language,
                file_path: h.file_path,
                start_line: h.start_line,
                signature: if h.signature.is_empty() {
                    None
                } else {
                    Some(h.signature)
                },
                doc_comment: if h.doc_comment.is_empty() {
                    None
                } else {
                    Some(h.doc_comment)
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
                confidence: Some(h.score),
                code_context: None,
                content_type: None,
                body_span: None,
                body_hash: None,
                annotations: Vec::new(),
            }
        })
        .collect();
    // Hydrate code_context / visibility / metadata / body_span from SQLite
    // when a database is available — mirrors the production enrichment
    // path so tests that assert on these fields keep passing after T9.
    if let Some(db_ref) = db {
        let ids: Vec<String> = symbols.iter().map(|s| s.id.clone()).collect();
        if !ids.is_empty() {
            if let Ok(stored) = db_ref.get_symbols_by_ids(&ids) {
                let by_id: std::collections::HashMap<String, _> =
                    stored.into_iter().map(|s| (s.id.clone(), s)).collect();
                for sym in symbols.iter_mut() {
                    if let Some(s) = by_id.get(&sym.id) {
                        if sym.code_context.is_none() {
                            sym.code_context = s.code_context.clone();
                        }
                        if sym.visibility.is_none() {
                            sym.visibility = s.visibility.clone();
                        }
                        if sym.metadata.is_none() {
                            sym.metadata = s.metadata.clone();
                        }
                        if sym.body_span.is_none() {
                            sym.body_span = s.body_span.clone();
                        }
                        if sym.body_hash.is_none() {
                            sym.body_hash = s.body_hash.clone();
                        }
                    }
                }
            }
        }
    }
    let total = symbols.len();
    Ok((symbols, symbol_results.relaxed, total))
}

// ---------------------------------------------------------------------------
// Phase 2 — unified search path
// ---------------------------------------------------------------------------

/// Convert a [`julie_index::search::index::UnifiedHit`] into an extractors
/// [`Symbol`] for use with the shared `SearchHit` / formatter plumbing.
fn unified_hit_to_symbol(hit: julie_index::search::index::UnifiedHit) -> Symbol {
    let kind = SymbolKind::try_from_string(&hit.kind).unwrap_or(SymbolKind::Variable);
    Symbol {
        id: hit.id,
        name: hit.name,
        kind,
        language: hit.language,
        file_path: hit.file_path,
        start_line: hit.start_line,
        signature: if hit.signature.is_empty() {
            None
        } else {
            Some(hit.signature)
        },
        doc_comment: if hit.doc_comment.is_empty() {
            None
        } else {
            Some(hit.doc_comment)
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
        confidence: Some(hit.tantivy_score),
        code_context: None,
        content_type: None,
        body_span: None,
        body_hash: None,
        annotations: Vec::new(),
    }
}

/// Kind of result row to retain after the unified search.  Used by the
/// definition/content target distinction in the test shim and ablation
/// harness.  None = keep all (default).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnifiedKindFilter {
    /// Drop file rows (`kind == "file"`); keep symbol rows.
    SymbolsOnly,
    /// Drop symbol rows; keep only file rows.
    FilesOnly,
}

/// Workspace-routing layer for the unified BM25 search path.
///
/// Mirrors the structure of `text_search_impl` but calls
/// `SearchIndex::search_unified` instead of the per-target search methods.
/// Returns `(hits_as_symbols, relaxed, total_count)` where `relaxed` is true
/// when the AND query fell back to OR mode.
pub async fn unified_search_impl(
    query: &str,
    filter: &julie_index::search::SearchFilter,
    limit: u32,
    workspace_ids: Option<Vec<String>>,
    handler: &dyn ToolContext,
) -> Result<(Vec<Symbol>, bool, usize)> {
    // Lazy-init the workspace's embedding provider when the query looks
    // like natural language.  Single-flighted; idempotent.  Pre-T9 this
    // was triggered at the top of `text_search_impl`; we keep that
    // contract so the deferred-init test (and the hybrid search path
    // that depends on the provider) still works.
    super::nl_embeddings::maybe_initialize_embeddings_for_nl_definitions(query, handler).await;
    unified_search_impl_with_kind_filter(query, filter, limit, workspace_ids, None, handler).await
}

/// Like [`unified_search_impl`] but with an optional [`UnifiedKindFilter`] to
/// retain only symbol rows or only file rows after the unified search.  When
/// a filter is supplied the underlying search over-fetches by 5x so the
/// requested limit is honoured AFTER filtering.
pub async fn unified_search_impl_with_kind_filter(
    query: &str,
    filter: &julie_index::search::SearchFilter,
    limit: u32,
    workspace_ids: Option<Vec<String>>,
    kind_filter: Option<UnifiedKindFilter>,
    handler: &dyn ToolContext,
) -> Result<(Vec<Symbol>, bool, usize)> {
    // Lazy-init the workspace's embedding provider when the query looks
    // like natural language (mirrors the contract of `unified_search_impl`
    // for the kind-filtered entrypoint).
    super::nl_embeddings::maybe_initialize_embeddings_for_nl_definitions(query, handler).await;

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

    let query_clone = query.to_string();
    let limit_usize = limit as usize;
    let filter_clone = filter.clone();
    let files_only_flag = kind_filter.map(|kf| matches!(kf, UnifiedKindFilter::FilesOnly));

    if let Some(target_id) = target_workspace_id {
        let si_arc = handler.get_search_index_for_workspace(&target_id).await?;
        let db = handler
            .get_pooled_database_for_workspace(&target_id)
            .await
            .ok();

        let results = tokio::task::spawn_blocking(move || -> Result<(Vec<Symbol>, bool, usize)> {
            let si_arc = match si_arc {
                Some(si) => si,
                None => {
                    return Ok((Vec::new(), false, 0));
                }
            };
            let index = si_arc;

            let (hits, relaxed) = match files_only_flag {
                Some(flag) => index.search_unified_kind_filtered(
                    &query_clone,
                    &filter_clone,
                    limit_usize,
                    flag,
                )?,
                None => index.search_unified_with_meta(&query_clone, &filter_clone, limit_usize)?,
            };
            let count = hits.len();
            let mut symbols: Vec<Symbol> = hits.into_iter().map(unified_hit_to_symbol).collect();

            // Enrich symbols with code_context / visibility / metadata /
            // body_span / body_hash from the SQLite database.  Tantivy
            // only stores a truncated `code_body`; the full `code_context`
            // lives in the symbols table.  See dogfood test:
            // test_definition_search_includes_code_context.
            if let Some(db) = db.as_ref() {
                enrich_symbols_from_db(&mut symbols, db);
            }

            Ok((symbols, relaxed, count))
        })
        .await??;

        return Ok(results);
    }

    // Primary workspace path.
    let (db, search_index_clone) = handler.primary_pooled_database_and_search_index().await?;

    let results = tokio::task::spawn_blocking(move || -> Result<(Vec<Symbol>, bool, usize)> {
        let index = search_index_clone;

        let (hits, relaxed) = match files_only_flag {
            Some(flag) => index.search_unified_kind_filtered(
                &query_clone,
                &filter_clone,
                limit_usize,
                flag,
            )?,
            None => index.search_unified_with_meta(&query_clone, &filter_clone, limit_usize)?,
        };
        let count = hits.len();
        let mut symbols: Vec<Symbol> = hits.into_iter().map(unified_hit_to_symbol).collect();

        enrich_symbols_from_db(&mut symbols, &db);

        Ok((symbols, relaxed, count))
    })
    .await??;

    Ok(results)
}

/// Enrich symbols with code_context, visibility, and metadata from a SQLite
/// database.  Tantivy stores a truncated `code_body` for search indexing but
/// callers (dogfood tests, MCP responses) need the full `code_context` from
/// the symbols table.  Batched lookup by symbol ID.
fn enrich_symbols_from_db(symbols: &mut [Symbol], db: &julie_core::database::SymbolDatabase) {
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
        }
        Err(e) => {
            tracing::debug!("Could not enrich code_context from SQLite: {}", e);
        }
    }
}

/// Like [`unified_search_impl`] but returns raw [`UnifiedHit`]s instead of
/// converting them to [`Symbol`].  Used by [`execute_search_unified`] so the
/// "file" `kind` field is preserved all the way to [`SearchHit`].
pub async fn unified_search_hits(
    query: &str,
    filter: &julie_index::search::SearchFilter,
    limit: u32,
    workspace_ids: Option<Vec<String>>,
    handler: &dyn ToolContext,
) -> Result<(Vec<julie_index::search::index::UnifiedHit>, bool, usize)> {
    let current_primary_id = handler.current_workspace_id();
    let loaded_workspace_id = handler.loaded_workspace_id();

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

    let query_clone = query.to_string();
    let limit_usize = limit as usize;
    let filter_clone = filter.clone();

    if let Some(target_id) = target_workspace_id {
        let si_arc = handler.get_search_index_for_workspace(&target_id).await?;

        return tokio::task::spawn_blocking(
            move || -> Result<(Vec<julie_index::search::index::UnifiedHit>, bool, usize)> {
                let si_arc = match si_arc {
                    Some(si) => si,
                    None => return Ok((Vec::new(), false, 0)),
                };
                let index = si_arc;
                let (hits, relaxed) =
                    index.search_unified_with_meta(&query_clone, &filter_clone, limit_usize)?;
                let count = hits.len();
                Ok((hits, relaxed, count))
            },
        )
        .await?;
    }

    // Primary workspace path.
    let (_, search_index_clone) = handler.primary_pooled_database_and_search_index().await?;

    tokio::task::spawn_blocking(
        move || -> Result<(Vec<julie_index::search::index::UnifiedHit>, bool, usize)> {
            let index = search_index_clone;
            let (hits, relaxed) =
                index.search_unified_with_meta(&query_clone, &filter_clone, limit_usize)?;
            let count = hits.len();
            Ok((hits, relaxed, count))
        },
    )
    .await?
}

// ---------------------------------------------------------------------------
// Test-only shim: text_search_impl
//
// The old `text_search_impl` dispatcher was deleted in T9 (it routed to the
// per-target `definition_search_with_index` / `content_search_with_index`
// paths, both of which are gone).  Tests in `text_search_tantivy.rs`,
// `primary_workspace_bug.rs`, and the dogfood suite still call it through the
// async handler path.  This thin wrapper delegates to `unified_search_impl`
// and adapts the return type so those tests keep compiling and passing.
//
// Signature mirrors the old one:
//   text_search_impl(query, language, file_pattern, limit, workspace_ids,
//                    search_target, exclude_tests, context_lines, handler)
//   -> Result<(Vec<Symbol>, bool, usize)>
//
// `search_target` is accepted for API compat but no longer routes — everything
// goes through the unified path.
#[cfg(any(test, feature = "test-support"))]
#[allow(clippy::too_many_arguments)]
pub async fn text_search_impl(
    query: &str,
    language: &Option<String>,
    file_pattern: &Option<String>,
    limit: u32,
    workspace_ids: Option<Vec<String>>,
    search_target: &str,
    _context_lines: Option<u32>,
    exclude_tests: Option<bool>,
    handler: &dyn ToolContext,
) -> anyhow::Result<(Vec<julie_extractors::Symbol>, bool, usize)> {
    let mut filter = julie_index::search::SearchFilter::default();
    if let Some(lang) = language {
        filter.language = Some(lang.clone());
    }
    if let Some(pat) = file_pattern {
        filter.file_pattern = Some(pat.clone());
    }
    // Honour explicit `exclude_tests=Some(true)`.  `Some(false)` and `None`
    // leave the filter at default (off) so callers that want test results
    // get them; pre-T9 callers wired this through the SearchFilter the
    // same way.
    if exclude_tests == Some(true) {
        filter.exclude_tests = true;
    }

    let kind_filter = match search_target {
        "definitions" => Some(UnifiedKindFilter::SymbolsOnly),
        "content" | "files" | "paths" => Some(UnifiedKindFilter::FilesOnly),
        _ => None,
    };

    let (symbols, relaxed, total) = unified_search_impl_with_kind_filter(
        query,
        &filter,
        limit,
        workspace_ids,
        kind_filter,
        handler,
    )
    .await?;
    Ok((symbols, relaxed, total))
}
