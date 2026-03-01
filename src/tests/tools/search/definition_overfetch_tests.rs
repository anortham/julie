//! Tests for definition search over-fetch and exact-name promotion.
//!
//! When `search_target="definitions"`, Julie should:
//! 1. Over-fetch candidates from Tantivy (5x the user's limit)
//! 2. Promote results whose `name` exactly matches the query (case-insensitive) to the top
//! 3. Truncate to the user's requested limit
//!
//! This ensures that actual definitions (which may rank low in Tantivy because
//! they're mentioned once vs. many references) appear in the results.

#[cfg(test)]
mod tests {
    use crate::search::index::SymbolSearchResult;
    use crate::search::scoring::promote_exact_name_matches;

    /// Helper to create a SymbolSearchResult with the given name, file_path, and score.
    fn make_result(name: &str, file_path: &str, score: f32) -> SymbolSearchResult {
        make_result_with_kind(name, file_path, score, "function")
    }

    fn make_result_with_kind(
        name: &str,
        file_path: &str,
        score: f32,
        kind: &str,
    ) -> SymbolSearchResult {
        SymbolSearchResult {
            id: format!("id_{}_{}", name, file_path.replace('/', "_")),
            name: name.to_string(),
            signature: String::new(),
            doc_comment: String::new(),
            file_path: file_path.to_string(),
            kind: kind.to_string(),
            language: "rust".to_string(),
            start_line: 1,
            score,
        }
    }

    #[test]
    fn test_exact_match_promoted_to_top() {
        let mut results = vec![
            make_result("use_my_service", "src/consumer.rs", 5.0),
            make_result("my_service_helper", "src/helper.rs", 4.0),
            make_result("MyService", "src/service.rs", 2.0), // actual definition, lower score
        ];

        promote_exact_name_matches(&mut results, "MyService");

        assert_eq!(
            results[0].name, "MyService",
            "Exact match should be promoted to the top"
        );
        // The other two should follow in their original relative order
        assert_eq!(results[1].name, "use_my_service");
        assert_eq!(results[2].name, "my_service_helper");
    }

    #[test]
    fn test_case_insensitive_promotion() {
        let mut results = vec![
            make_result("ref_to_userservice", "src/ref.rs", 5.0),
            make_result("UserService", "src/service.rs", 2.0),
        ];

        // Query in all-lowercase should still match "UserService"
        promote_exact_name_matches(&mut results, "userservice");

        assert_eq!(
            results[0].name, "UserService",
            "Case-insensitive match should be promoted"
        );
    }

    #[test]
    fn test_preserves_order_among_non_matches() {
        let mut results = vec![
            make_result("alpha", "src/a.rs", 5.0),
            make_result("beta", "src/b.rs", 4.0),
            make_result("gamma", "src/c.rs", 3.0),
        ];

        // Query doesn't match any name exactly
        promote_exact_name_matches(&mut results, "delta");

        // Order should be completely unchanged
        assert_eq!(results[0].name, "alpha");
        assert_eq!(results[1].name, "beta");
        assert_eq!(results[2].name, "gamma");
    }

    #[test]
    fn test_multiple_exact_matches_all_promoted() {
        let mut results = vec![
            make_result("other_fn", "src/other.rs", 10.0),
            make_result("Config", "src/config/mod.rs", 3.0),
            make_result("unrelated", "src/unrelated.rs", 7.0),
            make_result("Config", "src/tests/config.rs", 1.0),
        ];

        promote_exact_name_matches(&mut results, "Config");

        // Both Config entries should be at the top, preserving their relative order
        assert_eq!(results[0].name, "Config");
        assert_eq!(results[0].file_path, "src/config/mod.rs");
        assert_eq!(results[1].name, "Config");
        assert_eq!(results[1].file_path, "src/tests/config.rs");

        // Non-matches follow in original order
        assert_eq!(results[2].name, "other_fn");
        assert_eq!(results[3].name, "unrelated");
    }

    #[test]
    fn test_empty_results_no_panic() {
        let mut results: Vec<SymbolSearchResult> = vec![];
        promote_exact_name_matches(&mut results, "anything");
        assert!(results.is_empty());
    }

    #[test]
    fn test_exact_match_already_at_top_stays() {
        let mut results = vec![
            make_result("SearchIndex", "src/search/index.rs", 10.0),
            make_result("search_in_index", "src/search.rs", 5.0),
        ];

        promote_exact_name_matches(&mut results, "SearchIndex");

        // Already at top, order should be unchanged
        assert_eq!(results[0].name, "SearchIndex");
        assert_eq!(results[1].name, "search_in_index");
    }

    #[test]
    fn test_promotion_is_stable_partition() {
        // The key property: among exact matches, relative order is preserved.
        // Among non-matches, relative order is preserved.
        // This is a stable partition, not a sort.
        let mut results = vec![
            make_result("ref1", "src/a.rs", 10.0),
            make_result("Foo", "src/foo1.rs", 5.0), // exact match #1
            make_result("ref2", "src/b.rs", 8.0),
            make_result("Foo", "src/foo2.rs", 3.0), // exact match #2
            make_result("ref3", "src/c.rs", 6.0),
        ];

        promote_exact_name_matches(&mut results, "Foo");

        // Exact matches first, in original order
        assert_eq!(results[0].file_path, "src/foo1.rs");
        assert_eq!(results[1].file_path, "src/foo2.rs");
        // Non-matches next, in original order
        assert_eq!(results[2].file_path, "src/a.rs");
        assert_eq!(results[3].file_path, "src/b.rs");
        assert_eq!(results[4].file_path, "src/c.rs");
    }

    // ── Kind-aware promotion tests ──────────────────────────────────────

    #[test]
    fn test_definition_kind_promoted_over_import_kind() {
        // Scenario: 5 imports of "UserService" rank higher in Tantivy than
        // the actual class definition because references mention the name more often.
        let mut results = vec![
            make_result_with_kind("UserService", "src/handler.rs", 8.0, "import"),
            make_result_with_kind("UserService", "src/routes.rs", 7.5, "import"),
            make_result_with_kind("UserService", "src/middleware.rs", 7.0, "import"),
            make_result_with_kind("UserService", "src/tests/auth.rs", 6.5, "import"),
            make_result_with_kind("UserService", "src/di.rs", 6.0, "import"),
            make_result_with_kind("UserService", "src/services/user.rs", 3.0, "class"),
        ];

        promote_exact_name_matches(&mut results, "UserService");

        // The class definition should be first among exact matches
        assert_eq!(
            results[0].kind, "class",
            "Definition kind (class) should be promoted above import kind"
        );
        assert_eq!(results[0].file_path, "src/services/user.rs");

        // Imports should follow (still exact matches, but lower tier)
        for i in 1..=5 {
            assert_eq!(results[i].kind, "import");
        }
    }

    #[test]
    fn test_definition_kinds_all_promoted() {
        // All "definition-like" kinds should be in the top tier
        let definition_kinds = vec![
            "class",
            "struct",
            "interface",
            "trait",
            "enum",
            "function",
            "method",
            "constructor",
        ];

        for def_kind in &definition_kinds {
            let mut results = vec![
                make_result_with_kind("Foo", "src/ref.rs", 10.0, "import"),
                make_result_with_kind("Foo", "src/def.rs", 2.0, def_kind),
            ];

            promote_exact_name_matches(&mut results, "Foo");

            assert_eq!(
                results[0].kind, *def_kind,
                "Kind '{}' should be promoted above 'import'",
                def_kind
            );
        }
    }

    #[test]
    fn test_three_tier_stable_order() {
        // Tier 1: exact name + definition kind
        // Tier 2: exact name + non-definition kind
        // Tier 3: non-exact matches
        let mut results = vec![
            make_result_with_kind("other_fn", "src/other.rs", 10.0, "function"), // tier 3
            make_result_with_kind("Config", "src/import1.rs", 8.0, "import"),    // tier 2
            make_result_with_kind("Config", "src/config.rs", 3.0, "struct"),     // tier 1
            make_result_with_kind("ConfigHelper", "src/helper.rs", 7.0, "function"), // tier 3
            make_result_with_kind("Config", "src/import2.rs", 5.0, "import"),    // tier 2
            make_result_with_kind("Config", "src/trait.rs", 2.0, "trait"),       // tier 1
        ];

        promote_exact_name_matches(&mut results, "Config");

        // Tier 1: definitions, in original order
        assert_eq!(results[0].file_path, "src/config.rs"); // struct
        assert_eq!(results[1].file_path, "src/trait.rs"); // trait

        // Tier 2: non-definition exact matches, in original order
        assert_eq!(results[2].file_path, "src/import1.rs"); // import
        assert_eq!(results[3].file_path, "src/import2.rs"); // import

        // Tier 3: non-matches, in original order
        assert_eq!(results[4].file_path, "src/other.rs");
        assert_eq!(results[5].file_path, "src/helper.rs");
    }
}
