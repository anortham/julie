//! Tests for get_context pivot scoring — namespace/module de-boost behavior.
//!
//! Verifies that structural declarations (namespace, module, export) are penalized
//! so real functions/structs/classes rank above them.

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::search::index::SymbolSearchResult;
    use crate::tools::get_context::pipeline::select_pivots;
    use crate::tools::get_context::scoring::select_pivots_for_query;

    /// Helper to create a SymbolSearchResult with a specific kind.
    fn make_result_with_kind(id: &str, name: &str, kind: &str, score: f32) -> SymbolSearchResult {
        SymbolSearchResult {
            id: id.to_string(),
            name: name.to_string(),
            signature: format!("fn {}()", name),
            doc_comment: String::new(),
            file_path: format!("src/{}.rs", name),
            kind: kind.to_string(),
            language: "rust".to_string(),
            start_line: 1,
            score,
        }
    }

    /// Helper to create a SymbolSearchResult with a specific kind and file path.
    fn make_result_with_kind_and_path(
        id: &str,
        name: &str,
        kind: &str,
        file_path: &str,
        score: f32,
    ) -> SymbolSearchResult {
        SymbolSearchResult {
            id: id.to_string(),
            name: name.to_string(),
            signature: format!("fn {}()", name),
            doc_comment: String::new(),
            file_path: file_path.to_string(),
            kind: kind.to_string(),
            language: "rust".to_string(),
            start_line: 1,
            score,
        }
    }

    /// Namespace with higher raw score should rank below a function after de-boost.
    /// namespace score 8.0 * 0.2 = 1.6, function score 5.0 * 1.0 = 5.0
    /// → function should be the first pivot.
    #[test]
    fn test_select_pivots_deboosts_namespace() {
        let results = vec![
            make_result_with_kind("ns", "token_estimation", "namespace", 8.0),
            make_result_with_kind("fn", "estimate_tokens", "function", 5.0),
        ];
        let ref_scores = HashMap::new();
        let pivots = select_pivots(results, &ref_scores);

        assert!(!pivots.is_empty(), "should return at least one pivot");
        assert_eq!(
            pivots[0].result.name, "estimate_tokens",
            "function should rank above namespace after de-boost"
        );
    }

    /// Module with higher raw score should rank below a struct after de-boost.
    /// module score 8.0 * 0.2 = 1.6, struct score 5.0 * 1.0 = 5.0
    /// → struct should be the first pivot.
    #[test]
    fn test_select_pivots_deboosts_module() {
        let results = vec![
            make_result_with_kind("mod", "scoring", "module", 8.0),
            make_result_with_kind("st", "Pivot", "struct", 5.0),
        ];
        let ref_scores = HashMap::new();
        let pivots = select_pivots(results, &ref_scores);

        assert!(!pivots.is_empty(), "should return at least one pivot");
        assert_eq!(
            pivots[0].result.name, "Pivot",
            "struct should rank above module after de-boost"
        );
    }

    /// A sole namespace result should still be returned (penalized but not filtered).
    #[test]
    fn test_select_pivots_namespace_still_shown_if_only_result() {
        let results = vec![make_result_with_kind(
            "ns",
            "token_estimation",
            "namespace",
            8.0,
        )];
        let ref_scores = HashMap::new();
        let pivots = select_pivots(results, &ref_scores);

        assert_eq!(pivots.len(), 1, "sole namespace should still be returned");
        assert_eq!(pivots[0].result.name, "token_estimation");
    }

    /// Namespace in a test file gets both penalties multiplied:
    /// TEST_FILE_PENALTY (0.3) * STRUCTURAL_KIND_PENALTY (0.2) = 0.06
    /// score 8.0 * 0.06 = 0.48
    #[test]
    fn test_select_pivots_namespace_in_test_file_double_penalty() {
        let results = vec![
            make_result_with_kind_and_path(
                "ns",
                "test_helpers",
                "namespace",
                "src/tests/helpers.rs",
                8.0,
            ),
            make_result_with_kind("fn", "real_function", "function", 1.0),
        ];
        let ref_scores = HashMap::new();
        let pivots = select_pivots(results, &ref_scores);

        assert!(!pivots.is_empty(), "should return at least one pivot");
        // namespace in test file: 8.0 * 0.3 * 0.2 = 0.48
        // function in src: 1.0 * 1.0 * 1.0 = 1.0
        // function should win despite much lower raw score
        assert_eq!(
            pivots[0].result.name, "real_function",
            "function (score 1.0) should beat namespace-in-test-file (8.0 * 0.06 = 0.48)"
        );
    }

    /// When the query exactly matches a test function name, the test should rank first
    /// despite TEST_FILE_PENALTY, because the user is clearly looking for that specific test.
    #[test]
    fn test_exact_test_name_match_overcomes_test_penalty() {
        let test_result = make_result_with_kind_and_path(
            "t1",
            "test_compute_security_risk",
            "function",
            "src/tests/analysis/security_risk_tests.rs",
            0.9,
        );
        let prod_result = make_result_with_kind_and_path(
            "p1",
            "compute_security_risk",
            "function",
            "src/analysis/security_risk.rs",
            0.8,
        );
        let results = vec![test_result, prod_result];
        let ref_scores = HashMap::new();

        let pivots = select_pivots_for_query("test_compute_security_risk", results, &ref_scores);
        assert!(!pivots.is_empty());
        assert_eq!(
            pivots[0].result.name, "test_compute_security_risk",
            "exact test-name match should rank first despite TEST_FILE_PENALTY"
        );
    }

    /// Noise names (e.g. `fmt`) should NOT receive centrality boost in select_pivots.
    /// `format_output` (non-noise) with same text score and ref_score should rank above `fmt`.
    #[test]
    fn test_select_pivots_centrality_skips_noise_names() {
        let results = vec![
            make_result_with_kind("sym_fmt", "fmt", "function", 5.0),
            make_result_with_kind("sym_format", "format_output", "function", 5.0),
        ];

        let mut ref_scores = HashMap::new();
        // Both have high reference counts
        ref_scores.insert("sym_fmt".to_string(), 500.0);
        ref_scores.insert("sym_format".to_string(), 500.0);

        let pivots = select_pivots(results, &ref_scores);

        assert!(!pivots.is_empty(), "should return at least one pivot");

        // fmt is noise — should not get centrality boost
        // format_output is not noise — should get centrality boost
        // With same base score, format_output should rank above fmt
        assert_eq!(
            pivots[0].result.name, "format_output",
            "non-noise name should rank above noise name when both have high ref_scores"
        );

        // Verify the scores are actually different (format_output boosted, fmt not)
        if pivots.len() >= 2 {
            assert!(
                pivots[0].combined_score > pivots[1].combined_score,
                "format_output ({:.4}) should have higher score than fmt ({:.4})",
                pivots[0].combined_score,
                pivots[1].combined_score
            );
        }
    }
}
