/// Filter Pipeline Tests
///
/// Tests for the symbol filtering functions in src/tools/symbols/filtering.rs.
/// Written BEFORE the index-based refactor to capture current behavior (TDD RED phase).
///
/// Covers:
/// - max_depth filtering (depth 0, 1, 2+)
/// - target filtering (partial match, case-insensitive, with descendants)
/// - limit filtering (top-level counting, child inclusion, truncation flag)
/// - Combined filters via apply_all_filters
/// - Empty input edge cases
#[cfg(test)]
mod tests {
    use crate::extractors::base::types::SymbolKind;
    use crate::extractors::base::Symbol;
    use crate::tools::symbols::filtering::{
        apply_all_filters, apply_limit_filter, apply_max_depth_filter, apply_target_filter,
    };

    /// Helper to build a minimal Symbol for testing.
    /// Only sets fields that filtering actually inspects: id, name, parent_id, kind.
    fn make_symbol(id: &str, name: &str, parent_id: Option<&str>) -> Symbol {
        Symbol {
            id: id.to_string(),
            name: name.to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "test.rs".to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 10,
            end_column: 0,
            start_byte: 0,
            end_byte: 100,
            signature: None,
            doc_comment: None,
            visibility: None,
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
            content_type: None,
        }
    }

    /// Build a realistic symbol hierarchy:
    ///
    /// ClassA (top-level, kind=Class)
    ///   ├── method_one (child of ClassA)
    ///   └── method_two (child of ClassA)
    /// ClassB (top-level, kind=Class)
    ///   └── helper (child of ClassB)
    ///       └── nested_fn (child of helper) -- depth 2
    /// standalone_fn (top-level)
    fn build_test_hierarchy() -> Vec<Symbol> {
        let mut class_a = make_symbol("a", "ClassA", None);
        class_a.kind = SymbolKind::Class;

        let method_one = make_symbol("a1", "method_one", Some("a"));
        let method_two = make_symbol("a2", "method_two", Some("a"));

        let mut class_b = make_symbol("b", "ClassB", None);
        class_b.kind = SymbolKind::Class;

        let helper = make_symbol("b1", "helper", Some("b"));
        let nested_fn = make_symbol("b1n", "nested_fn", Some("b1"));

        let standalone = make_symbol("s", "standalone_fn", None);

        vec![class_a, method_one, method_two, class_b, helper, nested_fn, standalone]
    }

    // ================================================================
    // max_depth filtering
    // ================================================================

    #[test]
    fn test_max_depth_0_returns_only_top_level() {
        let symbols = build_test_hierarchy();
        let result = apply_max_depth_filter(&symbols, 0);

        let names: Vec<&str> = result.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["ClassA", "ClassB", "standalone_fn"]);
    }

    #[test]
    fn test_max_depth_1_includes_direct_children() {
        let symbols = build_test_hierarchy();
        let result = apply_max_depth_filter(&symbols, 1);

        let names: Vec<&str> = result.iter().map(|s| s.name.as_str()).collect();
        // depth 0: ClassA, ClassB, standalone_fn
        // depth 1: method_one, method_two (children of ClassA), helper (child of ClassB)
        // NOT included: nested_fn (depth 2, child of helper)
        assert!(names.contains(&"ClassA"));
        assert!(names.contains(&"method_one"));
        assert!(names.contains(&"method_two"));
        assert!(names.contains(&"ClassB"));
        assert!(names.contains(&"helper"));
        assert!(names.contains(&"standalone_fn"));
        assert!(!names.contains(&"nested_fn"));
        assert_eq!(result.len(), 6);
    }

    #[test]
    fn test_max_depth_2_includes_all() {
        let symbols = build_test_hierarchy();
        let result = apply_max_depth_filter(&symbols, 2);

        // All 7 symbols should be included (max depth in hierarchy is 2)
        assert_eq!(result.len(), 7);
        let names: Vec<&str> = result.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"nested_fn"));
    }

    #[test]
    fn test_max_depth_large_value_includes_all() {
        let symbols = build_test_hierarchy();
        let result = apply_max_depth_filter(&symbols, 100);
        assert_eq!(result.len(), 7);
    }

    // ================================================================
    // target filtering
    // ================================================================

    #[test]
    fn test_target_filter_partial_match() {
        let symbols = build_test_hierarchy();
        let result = apply_target_filter(&symbols, "method");

        let names: Vec<&str> = result.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"method_one"));
        assert!(names.contains(&"method_two"));
        // Neither ClassA nor ClassB should be included (they don't match "method")
        assert!(!names.contains(&"ClassA"));
        assert!(!names.contains(&"ClassB"));
    }

    #[test]
    fn test_target_filter_case_insensitive() {
        let symbols = build_test_hierarchy();
        let result = apply_target_filter(&symbols, "classa");

        let names: Vec<&str> = result.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"ClassA"));
    }

    #[test]
    fn test_target_filter_includes_descendants() {
        let symbols = build_test_hierarchy();
        // "ClassB" should match ClassB, and include its descendants: helper and nested_fn
        let result = apply_target_filter(&symbols, "ClassB");

        let names: Vec<&str> = result.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"ClassB"));
        assert!(names.contains(&"helper"));
        assert!(names.contains(&"nested_fn"));
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_target_filter_no_match_returns_empty() {
        let symbols = build_test_hierarchy();
        let result = apply_target_filter(&symbols, "nonexistent");
        assert!(result.is_empty());
    }

    #[test]
    fn test_target_filter_matching_child_includes_its_descendants() {
        let symbols = build_test_hierarchy();
        // "helper" matches the child of ClassB; it should also include nested_fn
        let result = apply_target_filter(&symbols, "helper");

        let names: Vec<&str> = result.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"helper"));
        assert!(names.contains(&"nested_fn"));
        assert!(!names.contains(&"ClassB")); // parent is NOT included
        assert_eq!(result.len(), 2);
    }

    // ================================================================
    // limit filtering
    // ================================================================

    #[test]
    fn test_limit_filter_no_truncation_when_under_limit() {
        let symbols = build_test_hierarchy();
        let (result, was_truncated) = apply_limit_filter(&symbols, 10);

        assert!(!was_truncated);
        assert_eq!(result.len(), symbols.len());
    }

    #[test]
    fn test_limit_filter_counts_top_level_only() {
        let symbols = build_test_hierarchy();
        // We have 3 top-level symbols: ClassA, ClassB, standalone_fn
        // Limit of 2 should keep 2 top-level + their children
        let (result, was_truncated) = apply_limit_filter(&symbols, 2);

        assert!(was_truncated);
        // ClassA (top-level #1) + method_one + method_two = 3
        // ClassB (top-level #2) + helper + nested_fn = 3
        // standalone_fn excluded (top-level #3, over limit)
        // Total: 6
        let names: Vec<&str> = result.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"ClassA"));
        assert!(names.contains(&"ClassB"));
        assert!(!names.contains(&"standalone_fn"));
        assert_eq!(result.len(), 6);
    }

    #[test]
    fn test_limit_filter_limit_1_keeps_first_top_level_and_children() {
        let symbols = build_test_hierarchy();
        let (result, was_truncated) = apply_limit_filter(&symbols, 1);

        assert!(was_truncated);
        let names: Vec<&str> = result.iter().map(|s| s.name.as_str()).collect();
        // ClassA is the first top-level symbol
        assert!(names.contains(&"ClassA"));
        assert!(names.contains(&"method_one"));
        assert!(names.contains(&"method_two"));
        assert!(!names.contains(&"ClassB"));
        assert_eq!(result.len(), 3);
    }

    // ================================================================
    // apply_all_filters (combined)
    // ================================================================

    #[test]
    fn test_all_filters_no_target_no_limit() {
        let symbols = build_test_hierarchy();
        let (result, was_truncated, total) = apply_all_filters(symbols.clone(), 1, None, None);

        assert_eq!(total, 7);
        assert!(!was_truncated);
        // max_depth=1 removes nested_fn
        assert_eq!(result.len(), 6);
    }

    #[test]
    fn test_all_filters_with_target_and_depth() {
        let symbols = build_test_hierarchy();
        // max_depth=1, target="Class"
        // Step 1: depth filter keeps ClassA, method_one, method_two, ClassB, helper, standalone_fn
        // Step 2: target "Class" matches ClassA and ClassB, plus their descendants in the filtered set
        let (result, was_truncated, total) =
            apply_all_filters(symbols.clone(), 1, Some("Class"), None);

        assert_eq!(total, 7);
        assert!(!was_truncated);
        let names: Vec<&str> = result.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"ClassA"));
        assert!(names.contains(&"ClassB"));
        // Children of ClassA and ClassB that survived depth filter
        assert!(names.contains(&"method_one"));
        assert!(names.contains(&"method_two"));
        assert!(names.contains(&"helper"));
    }

    #[test]
    fn test_all_filters_with_limit() {
        let symbols = build_test_hierarchy();
        // max_depth=100 (all), no target, limit=1
        let (result, was_truncated, total) = apply_all_filters(symbols.clone(), 100, None, Some(1));

        assert_eq!(total, 7);
        assert!(was_truncated);
        // Limit 1 top-level: ClassA + its children
        let names: Vec<&str> = result.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"ClassA"));
        assert!(!names.contains(&"ClassB"));
    }

    #[test]
    fn test_all_filters_target_no_match_returns_empty() {
        let symbols = build_test_hierarchy();
        let (result, was_truncated, total) =
            apply_all_filters(symbols.clone(), 1, Some("zzz_nonexistent"), None);

        assert_eq!(total, 7);
        assert!(!was_truncated);
        assert!(result.is_empty());
    }

    // ================================================================
    // Edge cases
    // ================================================================

    #[test]
    fn test_empty_input_max_depth() {
        let result = apply_max_depth_filter(&[], 1);
        assert!(result.is_empty());
    }

    #[test]
    fn test_empty_input_target() {
        let result = apply_target_filter(&[], "anything");
        assert!(result.is_empty());
    }

    #[test]
    fn test_empty_input_limit() {
        let (result, was_truncated) = apply_limit_filter(&[], 10);
        assert!(result.is_empty());
        assert!(!was_truncated);
    }

    #[test]
    fn test_empty_input_all_filters() {
        let (result, was_truncated, total) = apply_all_filters(vec![], 1, None, None);
        assert!(result.is_empty());
        assert!(!was_truncated);
        assert_eq!(total, 0);
    }

    #[test]
    fn test_flat_symbols_no_parents() {
        // All symbols are top-level (no parent hierarchy)
        let symbols = vec![
            make_symbol("a", "alpha", None),
            make_symbol("b", "beta", None),
            make_symbol("c", "gamma", None),
        ];

        // max_depth=0 should still return all (they're all top-level)
        let result = apply_max_depth_filter(&symbols, 0);
        assert_eq!(result.len(), 3);

        // limit=2 should truncate
        let (result, truncated) = apply_limit_filter(&symbols, 2);
        assert!(truncated);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_single_symbol() {
        let symbols = vec![make_symbol("x", "lone_wolf", None)];

        let (result, was_truncated, total) = apply_all_filters(symbols, 0, None, Some(10));
        assert_eq!(result.len(), 1);
        assert!(!was_truncated);
        assert_eq!(total, 1);
        assert_eq!(result[0].name, "lone_wolf");
    }
}
