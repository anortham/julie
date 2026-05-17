//! C3 — Reranker ORDERING-assertion tests.
//!
//! These tests prove the reranker actually changes ranking quality, not just
//! adds boost arithmetic. Each test sets up candidates where the EXPECTED
//! ordering differs from the raw tantivy/BM25 ordering, then asserts the
//! reranker delivers the expected ordering.
//!
//! The companion `reranker_tests.rs` covers boost-arithmetic correctness
//! per-candidate. These tests cover end-to-end "X must rank above Y" cases.
//! Both layers are required: arithmetic tests catch regressions in the
//! scoring math, ordering tests catch regressions in the policy (which
//! boosts are layered, the two-pass intent downgrade, vendor demotion).

#[cfg(test)]
mod tests {
    use crate::extractors::SymbolKind;
    use crate::search::query_parse::parse_query;
    use crate::search::reranker::{Candidate, Ranked, rerank};

    fn ranks(query: &str, candidates: Vec<Candidate>) -> Vec<Ranked> {
        let parsed = parse_query(query);
        rerank(&parsed, &candidates)
    }

    /// Candidate factory tuned for ordering tests. Sets BM25-ish base score
    /// so the reranker has real numbers to layer onto.
    fn cand(title: &str, path: &str, kind: SymbolKind, tantivy_score: f32) -> Candidate {
        let role = if path.contains("/tests/") || path.contains("_test.") {
            "test"
        } else if path.contains("node_modules/") || path.contains("vendor/") {
            "vendor"
        } else if path.contains("target/") || path.contains("/dist/") {
            "generated"
        } else if path.ends_with(".md") {
            "docs"
        } else {
            "src"
        };
        let is_test = role == "test";
        let is_file_doc = role == "docs";
        Candidate::builder()
            .title(title)
            .path(path)
            .body(format!("fn {}", title))
            .kind(kind)
            .role(role)
            .test_role(if is_test { "impl_test" } else { "" })
            .is_test(is_test)
            .is_file_doc(is_file_doc)
            .is_source_language(true)
            .tantivy_score(tantivy_score)
            .build()
    }

    // ────────────────────────────────────────────────────────────────────
    // Case 1: vendor demotion overrides exact-title boost from src partial
    // ────────────────────────────────────────────────────────────────────

    /// A vendor exact-title hit must NOT outrank a source partial-title hit.
    /// Without I1 vendor demotion: vendor gets +100 EXACT_TITLE, source gets
    /// +50 PARTIAL_TITLE → vendor wins by 50. With -70 VENDOR_PENALTY:
    /// vendor lands at +30, source at +50 → source wins.
    #[test]
    fn vendor_exact_match_loses_to_source_partial_match() {
        // Equal tantivy base so the reranker is the deciding factor.
        let vendor_exact = cand(
            "router",
            "node_modules/express/router.js",
            SymbolKind::Function,
            10.0,
        );
        let source_partial = cand(
            "router_inner",
            "src/server/router.rs",
            SymbolKind::Function,
            10.0,
        );

        let ranked = ranks("router", vec![vendor_exact, source_partial]);

        assert_eq!(
            ranked[0].candidate.title, "router_inner",
            "source partial-title match should outrank vendor exact-title match \
             (vendor demotion is the policy). got order: {:?}",
            ranked.iter().map(|r| &r.candidate.title).collect::<Vec<_>>()
        );
    }

    /// A vendor exact-title hit must still rank ABOVE a source non-match
    /// (so VENDOR_PENALTY doesn't bury vendor hits the user actually needs).
    /// vendor: +100 EXACT - 70 VENDOR = +30. source non-match: 0.
    #[test]
    fn vendor_exact_match_beats_source_no_match() {
        let vendor_exact = cand(
            "router",
            "node_modules/express/router.js",
            SymbolKind::Function,
            10.0,
        );
        let source_nomatch = cand(
            "unrelated_helper",
            "src/utils/helpers.rs",
            SymbolKind::Function,
            10.0,
        );

        let ranked = ranks("router", vec![vendor_exact, source_nomatch]);

        assert_eq!(
            ranked[0].candidate.title, "router",
            "vendor exact-title match should still beat a source non-match \
             (demotion should not bury vendor hits entirely). got order: {:?}",
            ranked.iter().map(|r| &r.candidate.title).collect::<Vec<_>>()
        );
    }

    /// Generated paths get the same demotion magnitude as vendor.
    #[test]
    fn generated_exact_match_loses_to_source_partial_match() {
        let generated_exact = cand(
            "router",
            "target/debug/build/router.rs",
            SymbolKind::Function,
            10.0,
        );
        let source_partial = cand(
            "router_inner",
            "src/server/router.rs",
            SymbolKind::Function,
            10.0,
        );

        let ranked = ranks("router", vec![generated_exact, source_partial]);

        assert_eq!(
            ranked[0].candidate.title, "router_inner",
            "source partial-title match should outrank generated exact-title match. got: {:?}",
            ranked.iter().map(|r| &r.candidate.title).collect::<Vec<_>>()
        );
    }

    // ────────────────────────────────────────────────────────────────────
    // Case 2: Symbol(K) intent promotes correct kind
    // ────────────────────────────────────────────────────────────────────

    /// "function process" should rank a `fn process` above a `struct process`
    /// of the same name. Without intent: both get EXACT_TITLE_BOOST equally.
    /// With intent: function gets the additional INTENT_TITLE + INTENT_ROLE
    /// boosts since kind matches.
    #[test]
    fn symbol_intent_function_promotes_function_above_class() {
        // Three tokens minimum for parse_query to activate Symbol intent.
        let struct_match = cand("process", "src/types/data.rs", SymbolKind::Struct, 10.0);
        let function_match = cand(
            "process",
            "src/pipeline/runner.rs",
            SymbolKind::Function,
            10.0,
        );

        let ranked = ranks(
            "function process data",
            vec![struct_match, function_match],
        );

        assert_eq!(
            ranked[0].candidate.kind,
            SymbolKind::Function,
            "Symbol(Function) intent should promote the fn over the struct of the same name. got: {:?}",
            ranked
                .iter()
                .map(|r| (&r.candidate.title, &r.candidate.kind))
                .collect::<Vec<_>>()
        );
    }

    // ────────────────────────────────────────────────────────────────────
    // Case 3: I4 two-pass intent downgrade
    // ────────────────────────────────────────────────────────────────────

    /// When intent is Symbol(Function) but no candidate has BOTH
    /// kind == Function AND title-term match, the intent boost should be
    /// downgraded — preventing a partial-name same-kind candidate
    /// (function unrelated_helper) from outranking an exact-name wrong-kind
    /// candidate (class process).
    ///
    /// Without I4: function unrelated_helper gets +0 INTENT_TITLE (title
    /// doesn't contain "process") but the class process gets +100 EXACT.
    /// Class wins. That's actually fine here — but consider:
    ///
    /// Query: "function process_data input". The class "process" has no
    /// title-term match. The fn "process_input" has both kind==Function
    /// AND a partial term match. So intent CAN realize → no downgrade.
    /// This test exercises the OTHER case: intent CANNOT realize, and we
    /// don't unfairly boost the non-realizing function.
    #[test]
    fn symbol_intent_downgrades_when_no_candidate_realizes_kind_and_term() {
        // Intent: Symbol(Function), target_terms include "missing_name".
        // No candidate has both kind=Function AND title contains
        // "missing_name", so the I4 downgrade should fire — the result
        // is scored as if intent were Free.
        //
        // class missing_name → +100 EXACT_TITLE + 50 kind_boost = +150
        //                      (intent boost suppressed by I4 since no
        //                      candidate is Function+missing_name)
        // fn helper_function → no title match, no kind boost. base 10.
        let class_exact = cand(
            "missing_name",
            "src/types.rs",
            SymbolKind::Class,
            10.0,
        );
        let fn_unrelated = cand(
            "helper_function",
            "src/util.rs",
            SymbolKind::Function,
            10.0,
        );

        let ranked = ranks(
            "function missing_name target",
            vec![class_exact, fn_unrelated],
        );

        assert_eq!(
            ranked[0].candidate.title, "missing_name",
            "I4 two-pass intent downgrade: when no candidate is both Function \
             AND title matches the term, an unrelated Function should NOT \
             outrank an exact-title-match Class via intent boost. got order: {:?}",
            ranked.iter().map(|r| &r.candidate.title).collect::<Vec<_>>()
        );
    }

    // ────────────────────────────────────────────────────────────────────
    // Case 4: Test intent promotes test files
    // ────────────────────────────────────────────────────────────────────

    /// "test render" should rank a test file's `test_render` above a
    /// production `render` of the same name with the same base score.
    /// Test intent fires INTENT_TITLE + INTENT_ROLE (since is_test).
    #[test]
    fn test_intent_promotes_test_file_above_production() {
        // 3+ tokens required for parse_query to activate Test intent.
        let production = cand("render", "src/ui/renderer.rs", SymbolKind::Function, 10.0);
        let test = cand(
            "test_render",
            "src/tests/ui/render_tests.rs",
            SymbolKind::Function,
            10.0,
        );

        let ranked = ranks("test render output", vec![production, test]);

        assert_eq!(
            ranked[0].candidate.title, "test_render",
            "Test intent should promote the test-file candidate above the \
             production candidate. got order: {:?}",
            ranked.iter().map(|r| &r.candidate.title).collect::<Vec<_>>()
        );
    }

    // ────────────────────────────────────────────────────────────────────
    // Case 5: Phrase boost on file doc swings ordering
    // ────────────────────────────────────────────────────────────────────

    /// When a 4-term phrase is present in a doc file's body, the
    /// PHRASE_BOOST + PHRASE_FILE_DOC_BOOST (260 + 120 = 380) outweighs
    /// a structural exact-title match in a source file with no phrase.
    #[test]
    fn phrase_match_in_file_doc_outranks_source_exact_title_no_phrase() {
        // Source file with exact-title match but no phrase: +100 + tantivy 10
        let source_exact_no_phrase = Candidate::builder()
            .title("plan")
            .path("src/plan.rs")
            .body("fn plan() {}")
            .kind(SymbolKind::Function)
            .role("src")
            .test_role("")
            .is_test(false)
            .is_file_doc(false)
            .is_source_language(true)
            .tantivy_score(10.0)
            .build();

        // Doc file containing the 4-term phrase in body:
        //   path doesn't contain individual terms (no PATH_BOOST)
        //   title doesn't match (no EXACT/PARTIAL title)
        //   body contains the contiguous phrase: PHRASE_BOOST (+260) +
        //   PHRASE_FILE_DOC_BOOST (+120) = +380
        let doc_phrase = Candidate::builder()
            .title("readme")
            .path("docs/readme.md")
            .body("alpha bravo charlie delta is the canonical sequence")
            .kind(SymbolKind::Module)
            .role("docs")
            .test_role("")
            .is_test(false)
            .is_file_doc(true)
            .is_source_language(false)
            .tantivy_score(5.0)
            .build();

        let ranked = ranks(
            "alpha bravo charlie delta",
            vec![source_exact_no_phrase, doc_phrase],
        );

        assert_eq!(
            ranked[0].candidate.title, "readme",
            "phrase boost in file doc should outrank a source exact-title \
             match with no phrase. got: {:?}",
            ranked
                .iter()
                .map(|r| (&r.candidate.title, r.final_score))
                .collect::<Vec<_>>()
        );
    }

    // ────────────────────────────────────────────────────────────────────
    // Case 6: Centrality / base score retains discriminating power
    // ────────────────────────────────────────────────────────────────────

    /// Two candidates with the same reranker treatment (same title, same
    /// kind, both source) but different base tantivy scores must preserve
    /// the base ordering. Validates that I1 + I4 didn't accidentally
    /// destroy the meaningful information in `tantivy_score`.
    #[test]
    fn equal_reranker_treatment_preserves_base_score_order() {
        let high_base = cand("process", "src/a.rs", SymbolKind::Function, 25.0);
        let low_base = cand("process", "src/b.rs", SymbolKind::Function, 8.0);

        let ranked = ranks("process", vec![low_base, high_base]);

        assert_eq!(
            ranked[0].candidate.path, "src/a.rs",
            "with identical reranker boosts, the higher base tantivy score \
             must win. got: {:?}",
            ranked
                .iter()
                .map(|r| (&r.candidate.path, r.final_score))
                .collect::<Vec<_>>()
        );
    }
}
