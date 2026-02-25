//! Tests for get_context pivot scoring — namespace/module de-boost behavior.
//!
//! Verifies that structural declarations (namespace, module, export) are penalized
//! so real functions/structs/classes rank above them.

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::search::index::SymbolSearchResult;
    use crate::tools::get_context::pipeline::{select_pivots, Pivot};

    /// Helper to create a SymbolSearchResult with a specific kind.
    fn make_result_with_kind(
        id: &str,
        name: &str,
        kind: &str,
        score: f32,
    ) -> SymbolSearchResult {
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

        assert!(
            !pivots.is_empty(),
            "should return at least one pivot"
        );
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

        assert!(
            !pivots.is_empty(),
            "should return at least one pivot"
        );
        assert_eq!(
            pivots[0].result.name, "Pivot",
            "struct should rank above module after de-boost"
        );
    }

    /// A sole namespace result should still be returned (penalized but not filtered).
    #[test]
    fn test_select_pivots_namespace_still_shown_if_only_result() {
        let results = vec![
            make_result_with_kind("ns", "token_estimation", "namespace", 8.0),
        ];
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

        assert!(
            !pivots.is_empty(),
            "should return at least one pivot"
        );
        // namespace in test file: 8.0 * 0.3 * 0.2 = 0.48
        // function in src: 1.0 * 1.0 * 1.0 = 1.0
        // function should win despite much lower raw score
        assert_eq!(
            pivots[0].result.name, "real_function",
            "function (score 1.0) should beat namespace-in-test-file (8.0 * 0.06 = 0.48)"
        );
    }
}
