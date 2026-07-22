mod execution;
mod files;
mod terms;

#[cfg(any(test, feature = "test-support"))]
pub(crate) use files::classify_file_match;
pub use files::compact_alnum_lc;
#[cfg(any(test, feature = "test-support"))]
pub use files::{apply_reranker_to_content_results, apply_symbol_title_boost_to_file_results};
pub(super) use files::{basename_for_path, normalize_file_path};
use terms::build_annotation_symbol_query;

use tantivy::collector::TopDocs;
use tantivy::schema::TantivyDocument;

#[cfg(any(test, feature = "test-support"))]
use super::{ContentSearchResult, ContentSearchResults, FileSearchResult, FileSearchResults};
use super::{
    SearchFilter, SearchIndex, SymbolSearchResult, SymbolSearchResults, UnifiedHit,
    is_test_symbol_result,
};
use crate::search::error::Result;
use crate::search::expansion::expand_query_terms;
use crate::search::query::parse_annotation_query;
use crate::search::scoring::apply_important_patterns_boost;
use julie_core::glob::matches_glob_pattern;

const NL_RERANK_OVERFETCH_FACTOR: usize = 4;

impl SearchIndex {
    pub fn search_symbols_via_unified(
        &self,
        query_str: &str,
        filter: &SearchFilter,
        limit: usize,
    ) -> Result<SymbolSearchResults> {
        // Annotation queries (`@SomeAttr`, `[Authorize]`, `#[tokio::test]`,
        // `app.route("/")`, etc.) need the dedicated annotation pipeline
        // that filters on the `annotations_exact` indexed key.  We detect
        // them here and route to `search_annotation_symbols`; everything
        // else flows through the unified search path.
        let parsed_annotation = parse_annotation_query(query_str);
        if parsed_annotation.has_annotation_filters() {
            return self.search_annotation_symbols(query_str, filter, limit);
        }

        // Use the kind-filtered variant so the Tantivy BM25 candidate pool
        // contains only symbol rows.  Without this filter, queries like
        // "format" pull in 1000s of file rows that match the body content
        // and starve symbol candidates out of the over-fetch window.
        let (hits, relaxed) = self.search_unified_kind_filtered(query_str, filter, limit, false)?;
        let mut results: Vec<SymbolSearchResult> = hits
            .into_iter()
            .map(|h| SymbolSearchResult {
                id: h.id,
                name: h.name,
                signature: h.signature,
                doc_comment: h.doc_comment,
                file_path: h.file_path,
                kind: h.kind,
                language: h.language,
                start_line: h.start_line,
                score: h.tantivy_score,
                role: h.role,
                test_role: h.test_role,
            })
            .collect();

        // Apply language-specific important_patterns boost on top of the
        // unified reranker output.  Pre-T9 this lived inside `search_symbols`;
        // moved here so the adapter preserves the same scoring layer.
        // (The NL path prior is owned by the assembly layer, not here.)
        if let Some(configs) = &self.language_configs {
            apply_important_patterns_boost(&mut results, configs);
        }
        Ok(SymbolSearchResults { results, relaxed })
    }

    /// Annotation-aware symbol search.  Dispatched from
    /// [`search_symbols_via_unified`] when the query has annotation
    /// filters; uses [`build_annotation_symbol_query`] with the
    /// `annotations_exact` STRING field plus optional context terms from
    /// the surrounding query text.
    fn search_annotation_symbols(
        &self,
        query_str: &str,
        filter: &SearchFilter,
        limit: usize,
    ) -> Result<SymbolSearchResults> {
        let f = &self.schema_fields;
        let parsed = parse_annotation_query(query_str);
        let term_query = parsed.remaining_query.as_str();
        let expanded = expand_query_terms(term_query);
        let original_terms = self.annotation_context_terms(term_query);
        let alias_terms = Self::filter_compound_tokens(self.tokenize_terms(&expanded.alias_terms));
        let normalized_terms =
            Self::filter_compound_tokens(self.tokenize_terms(&expanded.normalized_terms));

        let query = build_annotation_symbol_query(
            &original_terms,
            &alias_terms,
            &normalized_terms,
            &parsed.annotation_keys,
            f,
            filter,
            true,
        );

        let searcher = self.reader.searcher();
        let candidate_limit = limit.saturating_mul(NL_RERANK_OVERFETCH_FACTOR).max(500);
        let top_docs = searcher.search(
            &query,
            &TopDocs::with_limit(candidate_limit).order_by_score(),
        )?;

        let user_word_count = term_query.split_whitespace().count();
        let (top_docs, relaxed) = if top_docs.is_empty() && user_word_count > 1 {
            let or_query = build_annotation_symbol_query(
                &original_terms,
                &alias_terms,
                &normalized_terms,
                &parsed.annotation_keys,
                f,
                filter,
                false,
            );
            let or_top = searcher.search(
                &or_query,
                &TopDocs::with_limit(candidate_limit).order_by_score(),
            )?;
            (or_top, true)
        } else {
            (top_docs, false)
        };

        let mut results: Vec<SymbolSearchResult> = Vec::with_capacity(top_docs.len());
        for (score, doc_address) in top_docs {
            let doc: TantivyDocument = searcher.doc(doc_address)?;
            results.push(SymbolSearchResult {
                id: Self::get_text_field(&doc, f.id),
                name: Self::get_text_field(&doc, f.name),
                signature: Self::get_text_field(&doc, f.signature),
                doc_comment: Self::get_text_field(&doc, f.doc_comment),
                file_path: Self::get_text_field(&doc, f.file_path),
                kind: Self::get_text_field(&doc, f.kind),
                language: Self::get_text_field(&doc, f.language),
                start_line: Self::get_u64_field(&doc, f.start_line) as u32,
                score,
                role: Self::get_text_field(&doc, f.role),
                test_role: Self::get_text_field(&doc, f.test_role),
            });
        }
        if let Some(pattern) = filter.file_pattern.as_deref() {
            results.retain(|result| matches_glob_pattern(&result.file_path, pattern));
        }
        if filter.exclude_tests {
            results.retain(|result| !is_test_symbol_result(&result.file_path, &result.role));
        }
        results.truncate(limit);
        Ok(SymbolSearchResults { results, relaxed })
    }

    /// `search_symbols` adapter — routes through [`search_unified`].
    ///
    /// The old `search_symbols` method was deleted in T9; this wrapper keeps
    /// existing callers (tests, hybrid.rs) compiling without per-file changes.
    pub fn search_symbols(
        &self,
        query_str: &str,
        filter: &SearchFilter,
        limit: usize,
    ) -> Result<SymbolSearchResults> {
        self.search_symbols_via_unified(query_str, filter, limit)
    }

    /// `search_symbols_relaxed` adapter — routes through [`search_unified`].
    ///
    /// The legacy `search_symbols_relaxed` semantics were "always return
    /// relaxed=true" (it was the OR-mode entry point with no AND first).
    /// We preserve that flag for callers that still rely on it; the actual
    /// AND/OR behaviour is now decided inside `search_unified` based on the
    /// query shape and result count.
    pub fn search_symbols_relaxed(
        &self,
        query_str: &str,
        filter: &SearchFilter,
        limit: usize,
    ) -> Result<SymbolSearchResults> {
        let mut out = self.search_symbols_via_unified(query_str, filter, limit)?;
        out.relaxed = true;
        Ok(out)
    }

    /// `search_content` test adapter — routes through [`search_unified`], returns file hits.
    ///
    /// The old `search_content` production method was deleted in T9; this wrapper
    /// keeps existing tests compiling while they migrate to unified search.
    #[cfg(any(test, feature = "test-support"))]
    pub fn search_content(
        &self,
        query_str: &str,
        filter: &SearchFilter,
        limit: usize,
    ) -> Result<ContentSearchResults> {
        let (hits, relaxed, and_candidate_count, or_candidate_count) =
            self.search_unified_with_stage_counts(query_str, filter, limit, true)?;
        let results: Vec<ContentSearchResult> = hits
            .into_iter()
            .map(|h| ContentSearchResult {
                file_path: h.file_path,
                language: h.language,
                score: h.tantivy_score,
            })
            .collect();
        Ok(ContentSearchResults {
            results,
            relaxed,
            and_candidate_count,
            or_candidate_count,
        })
    }

    /// `search_files` test adapter — routes through [`search_unified`], returns file hits.
    ///
    /// The old `search_files` production method was deleted in T9; this wrapper keeps
    /// existing tests compiling.  The `match_kind` field is derived
    /// from the query and file_path via [`classify_file_match`].
    #[cfg(any(test, feature = "test-support"))]
    pub fn search_files(
        &self,
        query_str: &str,
        filter: &SearchFilter,
        limit: usize,
    ) -> Result<FileSearchResults> {
        let (hits, relaxed) = self.search_unified_kind_filtered(query_str, filter, limit, true)?;
        let normalized_query = normalize_file_path(query_str.trim());
        let results: Vec<FileSearchResult> = hits
            .into_iter()
            .map(|h| {
                let match_kind = classify_file_match(query_str, &normalized_query, &h.file_path);
                FileSearchResult {
                    file_path: h.file_path,
                    language: h.language,
                    score: h.tantivy_score,
                    match_kind,
                }
            })
            .collect();
        Ok(FileSearchResults { results, relaxed })
    }

    /// Single BM25 sweep across all seven core FTS fields, returning mixed-kind
    /// [`UnifiedHit`]s.  No `doc_type` filter — symbol rows, file rows, and any
    /// other document type are all eligible.
    ///
    /// Over-fetches by `NL_RERANK_OVERFETCH_FACTOR` before applying post-filters
    /// and reranking.
    /// Falls back to OR mode when AND returns zero results on a multi-term query.
    pub fn search_unified(
        &self,
        query_str: &str,
        filter: &SearchFilter,
        limit: usize,
    ) -> Result<Vec<UnifiedHit>> {
        let (hits, _relaxed) = self.search_unified_with_meta(query_str, filter, limit)?;
        Ok(hits)
    }

    /// Variant of [`search_unified`] that also reports whether the AND query
    /// fell back to OR mode.  Used by the workspace-routing layer so the
    /// tantivy upgrade-report snapshot tooling and dogfood helpers can record
    /// `relaxed` in their per-query telemetry.
    ///
    /// Honours `JULIE_RERANKER_ENABLED=0` to disable the rerank pass for the
    /// baseline-comparison harness.
    pub fn search_unified_with_meta(
        &self,
        query_str: &str,
        filter: &SearchFilter,
        limit: usize,
    ) -> Result<(Vec<UnifiedHit>, bool)> {
        self.search_unified_internal(query_str, filter, limit, None)
    }

    /// Variant that also accepts an optional kind filter applied BEFORE the
    /// rerank step (so the candidate set is properly pruned before scoring).
    /// `None` = no kind filtering; `Some(true)` = files only; `Some(false)` =
    /// symbols only.
    pub fn search_unified_kind_filtered(
        &self,
        query_str: &str,
        filter: &SearchFilter,
        limit: usize,
        files_only: bool,
    ) -> Result<(Vec<UnifiedHit>, bool)> {
        self.search_unified_internal(query_str, filter, limit, Some(files_only))
    }

    /// Variant of [`search_unified`] that also reports per-stage candidate
    /// counts (AND-mode hit count and OR-mode hit count) alongside the
    /// `relaxed` flag.  Used by the `search_content` adapter and the
    /// `line_mode_or_fallback_tests` stage-count assertions.
    pub fn search_unified_with_stage_counts(
        &self,
        query_str: &str,
        filter: &SearchFilter,
        limit: usize,
        files_only: bool,
    ) -> Result<(Vec<UnifiedHit>, bool, usize, usize)> {
        self.search_unified_full(query_str, filter, limit, Some(files_only))
    }

    fn search_unified_internal(
        &self,
        query_str: &str,
        filter: &SearchFilter,
        limit: usize,
        files_only: Option<bool>,
    ) -> Result<(Vec<UnifiedHit>, bool)> {
        let (hits, relaxed, _and, _or) =
            self.search_unified_full(query_str, filter, limit, files_only)?;
        Ok((hits, relaxed))
    }
}
