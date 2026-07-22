use tantivy::Term;
use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, Occur, TermQuery};
use tantivy::schema::{IndexRecordOption, TantivyDocument, Value};

use super::super::{SearchFilter, SearchIndex, UnifiedHit, is_test_symbol_result};
use super::NL_RERANK_OVERFETCH_FACTOR;
use super::files::promote_exact_unified_hits;
use crate::search::error::Result;
use crate::search::expansion::expand_query_terms;
use crate::search::query::{UnifiedQueryFieldSet, build_unified_query, parse_annotation_query};
use julie_core::glob::matches_glob_pattern;

impl SearchIndex {
    pub(super) fn search_unified_full(
        &self,
        query_str: &str,
        filter: &SearchFilter,
        limit: usize,
        files_only: Option<bool>,
    ) -> Result<(Vec<UnifiedHit>, bool, usize, usize)> {
        use crate::search::query_parse::parse_query;
        use crate::search::reranker::{Candidate, rerank_unified};
        use crate::search::scoring::{classify_role, is_source_language, test_subrole};
        use julie_extractors::SymbolKind;

        let f = &self.schema_fields;

        if files_only != Some(true) {
            let parsed_annotation = parse_annotation_query(query_str);
            if parsed_annotation.has_annotation_filters() {
                let symbol_results = self.search_annotation_symbols(query_str, filter, limit)?;
                let relaxed = symbol_results.relaxed;
                let hits: Vec<UnifiedHit> = symbol_results
                    .results
                    .into_iter()
                    .map(|symbol| {
                        let basename = symbol
                            .file_path
                            .rsplit('/')
                            .next()
                            .unwrap_or(&symbol.file_path)
                            .to_string();
                        UnifiedHit {
                            id: symbol.id,
                            kind: symbol.kind,
                            name: symbol.name,
                            path_text: symbol.file_path.clone(),
                            file_path: symbol.file_path,
                            basename,
                            signature: symbol.signature,
                            doc_comment: symbol.doc_comment,
                            code_body: String::new(),
                            pretokenized_code: String::new(),
                            relationship_text: String::new(),
                            language: symbol.language,
                            start_line: symbol.start_line,
                            role: symbol.role,
                            test_role: symbol.test_role,
                            tantivy_score: symbol.score,
                        }
                    })
                    .collect();
                let count = hits.len();
                return Ok((hits, relaxed, count, 0));
            }
        }

        let expanded = expand_query_terms(query_str);
        // Two-tier original-term shape:
        //  * `original_terms` keeps only the split parts (compound stripped
        //    via `filter_compound_tokens`).  These are the AND-required
        //    constraints so that a query like "marker_abc" still matches a
        //    file containing both "marker" and "abc" separately.
        //  * `alias_terms` absorbs the compound tokens (`marker_abc`,
        //    `files_by_language`, etc.) as optional Should clauses.  Files
        //    that DO contain the compound get a BM25 boost from those
        //    clauses; files that only have the parts still match.
        let raw_original = self.tokenize_terms(&expanded.original_terms);
        let raw_alias = self.tokenize_terms(&expanded.alias_terms);
        let raw_normalized = self.tokenize_terms(&expanded.normalized_terms);

        let original_terms = Self::filter_compound_tokens(raw_original.clone());
        // The compound tokens themselves: tokens that were in raw_original
        // but got stripped by `filter_compound_tokens` (i.e. snake_case
        // compounds whose parts are all present).  Add them to alias_terms
        // so they contribute as `Should` clauses (scoring boost, not AND
        // requirement).
        let compound_overflow: Vec<String> = raw_original
            .into_iter()
            .filter(|t| !original_terms.contains(t))
            .collect();
        let mut alias_terms = Self::filter_compound_tokens(raw_alias);
        alias_terms.extend(compound_overflow);
        let normalized_terms = Self::filter_compound_tokens(raw_normalized);

        if original_terms.is_empty() {
            return Ok((Vec::new(), false, 0, 0));
        }

        // Over-fetch generously so exact-name promotion has a real pool to
        // partition from.  BM25 alone often buries an exact-name function
        // (e.g. `format_results`) under partial-match files that mention the
        // tokenised form (`format`, `result`) many times in their bodies.
        // The deleted per-target symbol path used `limit.saturating_mul(20).max(500)`;
        // the unified path matches that floor so the exact-name partitioner
        // can find the canonical symbol in the candidate set.
        let candidate_limit = limit.saturating_mul(NL_RERANK_OVERFETCH_FACTOR).max(500);

        // Optional doc_type filter applied at the Tantivy query level — only
        // documents of the requested type contribute to BM25 candidate
        // selection.  This is the right place for the filter (vs post-fetch)
        // because the candidate set is otherwise dominated by file rows for
        // common terms like "format", starving symbol queries.
        let wrap_with_doc_type =
            |inner: Box<dyn tantivy::query::Query>| -> Box<dyn tantivy::query::Query> {
                match files_only {
                    Some(want_file) => {
                        let dt = if want_file { "file" } else { "symbol" };
                        let dt_query = TermQuery::new(
                            Term::from_field_text(f.doc_type, dt),
                            IndexRecordOption::Basic,
                        );
                        Box::new(BooleanQuery::new(vec![
                            (Occur::Must, inner),
                            (Occur::Must, Box::new(dt_query)),
                        ]))
                    }
                    None => inner,
                }
            };

        // Field-set follows the kind filter: when the caller restricts to
        // file rows, search only content/path_text; when restricted to
        // symbol rows, search the seven symbol fields; when mixed, search
        // all eight.  This keeps BM25 IDF from being skewed by empty fields
        // on the side of the union we don't care about.
        let unified_field_set = match files_only {
            Some(true) => UnifiedQueryFieldSet::FilesOnly,
            Some(false) => UnifiedQueryFieldSet::SymbolsOnly,
            None => UnifiedQueryFieldSet::Mixed,
        };

        let and_inner = build_unified_query(
            &original_terms,
            &alias_terms,
            &normalized_terms,
            f.name,
            f.path_text,
            f.signature,
            f.doc_comment,
            f.relationship_text,
            f.code_body,
            f.pretokenized_code,
            f.content,
            unified_field_set,
            true, // require_all_terms — AND mode
        );
        let and_query = wrap_with_doc_type(Box::new(and_inner));

        let searcher = self.reader.searcher();
        let top_docs = searcher.search(
            &*and_query,
            &TopDocs::with_limit(candidate_limit).order_by_score(),
        )?;
        let and_candidate_count = top_docs.len();
        // Auto-fallback to OR when AND returns nothing.
        //
        // Two trigger conditions:
        //   1. Multi-word query (`user_word_count > 1`) — original guard
        //   2. Single-word query whose code-tokenizer split it into multiple
        //      tokens (e.g. "formatting" → ["f","or","matting"] because Lua's
        //      preserve_patterns include "or"). In this case AND across the
        //      derived tokens almost never matches a real symbol's indexed
        //      name, so we fall back to OR to surface meaningful candidates.
        let user_word_count = query_str.split_whitespace().count();
        // Derived-overflow: a single-word query whose tokenizer-produced
        // tokens don't include the lowercased word as-is means the code
        // tokenizer shredded it via preserve_patterns (e.g. "formatting"
        // → ["f","or","matting"] because Lua's preserve_patterns include
        // "or").  In that case the AND query across the derived tokens
        // almost never matches a real symbol's indexed name and we want
        // OR-fallback to surface meaningful candidates.
        //
        // Conversely, "nonexistent_symbol_xyz" tokenizes to ["nonexistent_symbol_xyz",
        // "nonexistent", "symbol", "xyz"] — the compound token IS present,
        // so it's a legitimate AND-miss for a nonexistent identifier and
        // OR-fallback should NOT fire.
        let query_lower = query_str.trim().to_lowercase();
        // Compound check uses `alias_terms` because compound tokens (those
        // present in the raw tokenizer output but stripped from
        // `original_terms` because their parts are also present) live there
        // after the two-tier shuffle above.  If either group contains the
        // query verbatim, the tokenizer didn't shred it — this is a
        // legitimate compound miss, not a `derived_overflow` situation.
        let compound_in_tokens = original_terms.iter().any(|t| t == &query_lower)
            || alias_terms.iter().any(|t| t == &query_lower);
        let derived_overflow =
            user_word_count == 1 && original_terms.len() > 1 && !compound_in_tokens;
        let mut relaxed = false;
        let mut or_candidate_count: usize = 0;
        let top_docs = if top_docs.is_empty() && (user_word_count > 1 || derived_overflow) {
            relaxed = true;
            let or_inner = build_unified_query(
                &original_terms,
                &alias_terms,
                &normalized_terms,
                f.name,
                f.path_text,
                f.signature,
                f.doc_comment,
                f.relationship_text,
                f.code_body,
                f.pretokenized_code,
                f.content,
                unified_field_set,
                false, // OR mode
            );
            let or_query = wrap_with_doc_type(Box::new(or_inner));
            let or_top = searcher.search(
                &*or_query,
                &TopDocs::with_limit(candidate_limit).order_by_score(),
            )?;
            or_candidate_count = or_top.len();
            or_top
        } else {
            top_docs
        };

        // Materialize hits.
        let mut hits = Vec::with_capacity(top_docs.len());
        for (score, doc_address) in top_docs {
            let doc: TantivyDocument = searcher.doc(doc_address)?;
            hits.push(UnifiedHit {
                id: Self::get_text_field(&doc, f.id),
                kind: Self::get_text_field(&doc, f.kind),
                name: Self::get_text_field(&doc, f.name),
                path_text: Self::get_text_field(&doc, f.path_text),
                file_path: Self::get_text_field(&doc, f.file_path),
                basename: Self::get_text_field(&doc, f.basename),
                signature: Self::get_text_field(&doc, f.signature),
                doc_comment: Self::get_text_field(&doc, f.doc_comment),
                code_body: Self::get_text_field(&doc, f.code_body),
                pretokenized_code: Self::get_text_field(&doc, f.pretokenized_code),
                relationship_text: Self::get_text_field(&doc, f.relationship_text),
                language: Self::get_text_field(&doc, f.language),
                start_line: Self::get_u64_field(&doc, f.start_line) as u32,
                role: Self::get_text_field(&doc, f.role),
                test_role: Self::get_text_field(&doc, f.test_role),
                tantivy_score: score,
            });
        }
        // Post-fetch filters (language / kind / file_pattern / exclude_tests).
        if let Some(ref lang) = filter.language {
            hits.retain(|h| &h.language == lang);
        }
        if let Some(ref kind) = filter.kind {
            hits.retain(|h| &h.kind == kind);
        }
        if let Some(ref pattern) = filter.file_pattern {
            hits.retain(|h| matches_glob_pattern(&h.file_path, pattern));
        }
        if filter.exclude_tests {
            hits.retain(|h| !is_test_symbol_result(&h.file_path, &h.role));
        }
        // Note: doc_type filtering for symbol-vs-file partition is applied
        // at the Tantivy query level above via `wrap_with_doc_type`.

        // Reranker toggle: honours `JULIE_RERANKER_ENABLED=0` (default-on so
        // any other value, missing var, or "1" keeps it enabled).  When off,
        // candidates retain raw Tantivy BM25 ordering — used by the c4
        // discoverability baseline test and the ablation harness.
        let reranker_enabled = !matches!(
            std::env::var("JULIE_RERANKER_ENABLED").as_deref(),
            Ok("0") | Ok("false") | Ok("FALSE")
        );

        // T6 unified reranking — builds Candidate structs for every hit and
        // delegates to `rerank_unified` which handles both symbol rows
        // (`is_file_doc == false`) and file rows (`is_file_doc == true`) in a
        // single pass with Eros-recipe field-score boosts.
        if reranker_enabled && !hits.is_empty() {
            let parsed = parse_query(query_str);
            let candidates: Vec<Candidate> = hits
                .iter()
                .map(|hit| {
                    let kind =
                        SymbolKind::try_from_string(&hit.kind).unwrap_or(SymbolKind::Variable);
                    let role = if hit.role.is_empty() {
                        classify_role(&hit.file_path, &hit.language).to_string()
                    } else {
                        hit.role.clone()
                    };
                    let test_role = if hit.test_role.is_empty() {
                        test_subrole(&hit.file_path).to_string()
                    } else {
                        hit.test_role.clone()
                    };
                    let is_test = role == "test";
                    // is_file_doc == true for file rows (kind field == "file"),
                    // not just for doc-role rows. The kind field is the
                    // authoritative discriminator.
                    let is_file_doc = hit.kind == "file";
                    let source_language = is_source_language(&hit.language);

                    let mut body = String::with_capacity(
                        hit.signature.len() + hit.doc_comment.len() + hit.code_body.len() + 2,
                    );
                    body.push_str(&hit.signature);
                    if !body.is_empty() && !hit.doc_comment.is_empty() {
                        body.push(' ');
                    }
                    body.push_str(&hit.doc_comment);
                    if !body.is_empty() && !hit.code_body.is_empty() {
                        body.push(' ');
                    }
                    body.push_str(&hit.code_body);

                    Candidate::builder()
                        .title(hit.name.clone())
                        .path(hit.file_path.clone())
                        .body(body)
                        .kind(kind)
                        .role(role)
                        .test_role(test_role)
                        .is_test(is_test)
                        .is_file_doc(is_file_doc)
                        .is_source_language(source_language)
                        .tantivy_score(hit.tantivy_score)
                        .build()
                })
                .collect();

            // rerank_unified returns sorted output; write scores back to the
            // original hits using the ordinal index carried in Ranked::original_index.
            // Index-based writeback is collision-free: the old (path, title) key
            // aliased file rows (name = basename stem, e.g. "foo" for src/foo.rs)
            // with same-named symbols in the same file, causing one candidate's
            // score to overwrite the other's and silently flip their ranks.
            let ranked = rerank_unified(&parsed, &candidates);

            let mut reranked_scores: Vec<Option<f32>> = vec![None; candidates.len()];
            for r in &ranked {
                reranked_scores[r.original_index] = Some(r.final_score);
            }
            for (hit, score_opt) in hits.iter_mut().zip(reranked_scores.iter()) {
                if let Some(&s) = score_opt.as_ref() {
                    hit.tantivy_score = s;
                }
            }

            hits.sort_by(|a, b| {
                b.tantivy_score
                    .partial_cmp(&a.tantivy_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.name.cmp(&b.name))
                    .then_with(|| a.file_path.cmp(&b.file_path))
            });
        }

        // Exact-name promotion: partition hits into (definitions, other_exact,
        // rest) where "exact" means symbol name matches the query (full or
        // last-component-of-qualified).  This runs regardless of reranker
        // state because BM25 alone often buries exact-name hits beneath
        // partial matches with more body content.  Ported from the deleted
        // per-target `promote_exact_name_matches`; the assertion is that the
        // exact-name symbol must surface to the top of definition searches
        // (c4_test_helper_discoverability) and qualified-name searches
        // (Phoenix.Router style).
        promote_exact_unified_hits(&mut hits, query_str);

        hits.truncate(limit);
        Ok((hits, relaxed, and_candidate_count, or_candidate_count))
    }
    pub(super) fn get_text_field(doc: &TantivyDocument, field: tantivy::schema::Field) -> String {
        doc.get_first(field)
            .and_then(|value| value.as_str().map(ToOwned::to_owned))
            .unwrap_or_default()
    }

    pub(super) fn get_u64_field(doc: &TantivyDocument, field: tantivy::schema::Field) -> u64 {
        doc.get_first(field)
            .and_then(|value| value.as_u64())
            .unwrap_or(0)
    }
}
