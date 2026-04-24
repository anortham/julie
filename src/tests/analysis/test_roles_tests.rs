//! Tests for post-extraction test role classification.

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use crate::analysis::test_roles::*;
    use crate::extractors::{AnnotationMarker, SymbolKind, TestRole};

    /// Build a minimal symbol for testing classification.
    fn make_symbol(
        kind: SymbolKind,
        language: &str,
        annotations: Vec<AnnotationMarker>,
        metadata: Option<HashMap<String, serde_json::Value>>,
    ) -> crate::extractors::Symbol {
        crate::extractors::Symbol {
            id: "test-id".to_string(),
            name: "test_fn".to_string(),
            kind,
            language: language.to_string(),
            file_path: "test.cs".to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 10,
            end_column: 0,
            start_byte: 0,
            end_byte: 100,
            signature: None,
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata,
            annotations,
            semantic_group: None,
            confidence: None,
            code_context: None,
            content_type: None,
        }
    }

    fn annotation(key: &str) -> AnnotationMarker {
        AnnotationMarker {
            annotation: key.to_string(),
            annotation_key: key.to_string(),
            raw_text: None,
            carrier: None,
        }
    }

    fn csharp_config() -> TestRoleConfig {
        TestRoleConfig {
            test_case: HashSet::from([
                "test".to_string(),
                "fact".to_string(),
                "test_method".to_string(),
            ]),
            parameterized_test: HashSet::from(["theory".to_string(), "inline_data".to_string()]),
            fixture_setup: HashSet::from(["setup".to_string(), "test_initialize".to_string()]),
            fixture_teardown: HashSet::from(["teardown".to_string(), "test_cleanup".to_string()]),
            test_container: HashSet::from(["test_class".to_string(), "test_fixture".to_string()]),
        }
    }

    // ---------------------------------------------------------------
    // classify_test_role tests
    // ---------------------------------------------------------------

    #[test]
    fn test_classify_test_role_csharp_fact_annotation() {
        let config = csharp_config();
        let symbol = make_symbol(SymbolKind::Method, "csharp", vec![annotation("fact")], None);

        let role = classify_test_role(&symbol, Some(&config));
        assert_eq!(role, Some(TestRole::TestCase));
    }

    #[test]
    fn test_classify_test_role_csharp_setup_annotation() {
        let config = csharp_config();
        let symbol = make_symbol(
            SymbolKind::Method,
            "csharp",
            vec![annotation("setup")],
            None,
        );

        let role = classify_test_role(&symbol, Some(&config));
        assert_eq!(role, Some(TestRole::FixtureSetup));
    }

    #[test]
    fn test_classify_test_role_csharp_teardown_annotation() {
        let config = csharp_config();
        let symbol = make_symbol(
            SymbolKind::Method,
            "csharp",
            vec![annotation("teardown")],
            None,
        );

        let role = classify_test_role(&symbol, Some(&config));
        assert_eq!(role, Some(TestRole::FixtureTeardown));
    }

    #[test]
    fn test_classify_test_role_csharp_theory_annotation() {
        let config = csharp_config();
        let symbol = make_symbol(
            SymbolKind::Method,
            "csharp",
            vec![annotation("theory")],
            None,
        );

        let role = classify_test_role(&symbol, Some(&config));
        assert_eq!(role, Some(TestRole::ParameterizedTest));
    }

    #[test]
    fn test_classify_test_role_container_on_class() {
        let config = csharp_config();
        let symbol = make_symbol(
            SymbolKind::Class,
            "csharp",
            vec![annotation("test_fixture")],
            None,
        );

        let role = classify_test_role(&symbol, Some(&config));
        assert_eq!(role, Some(TestRole::TestContainer));
    }

    #[test]
    fn test_classify_test_role_container_annotation_on_method_skipped() {
        // A container annotation on a Method should not match TestContainer
        let config = csharp_config();
        let symbol = make_symbol(
            SymbolKind::Method,
            "csharp",
            vec![annotation("test_fixture")],
            None,
        );

        let role = classify_test_role(&symbol, Some(&config));
        assert_eq!(role, None);
    }

    #[test]
    fn test_classify_test_role_callable_annotation_on_class_skipped() {
        // A callable-role annotation (test_case) on a Class should not match
        let config = csharp_config();
        let symbol = make_symbol(SymbolKind::Class, "csharp", vec![annotation("fact")], None);

        let role = classify_test_role(&symbol, Some(&config));
        assert_eq!(role, None);
    }

    #[test]
    fn test_classify_test_role_no_annotations_no_is_test() {
        let config = csharp_config();
        let symbol = make_symbol(SymbolKind::Method, "csharp", vec![], None);

        let role = classify_test_role(&symbol, Some(&config));
        assert_eq!(role, None);
    }

    #[test]
    fn test_classify_test_role_no_annotations_no_config() {
        let symbol = make_symbol(SymbolKind::Function, "rust", vec![], None);

        let role = classify_test_role(&symbol, None);
        assert_eq!(role, None);
    }

    #[test]
    fn test_classify_test_role_convention_fallback_is_test_true() {
        // Convention-based language: no annotations, but extractor set is_test=true
        let mut metadata = HashMap::new();
        metadata.insert("is_test".to_string(), serde_json::Value::Bool(true));
        let symbol = make_symbol(SymbolKind::Function, "rust", vec![], Some(metadata));

        let role = classify_test_role(&symbol, None);
        assert_eq!(role, Some(TestRole::TestCase));
    }

    #[test]
    fn test_classify_test_role_convention_fallback_is_test_on_class_ignored() {
        // is_test on a Class shouldn't produce a TestCase (callable role)
        let mut metadata = HashMap::new();
        metadata.insert("is_test".to_string(), serde_json::Value::Bool(true));
        let symbol = make_symbol(SymbolKind::Class, "python", vec![], Some(metadata));

        let role = classify_test_role(&symbol, None);
        assert_eq!(role, None);
    }

    #[test]
    fn test_classify_test_role_annotation_overrides_convention() {
        // If both annotation and is_test are present, annotation wins
        let config = csharp_config();
        let mut metadata = HashMap::new();
        metadata.insert("is_test".to_string(), serde_json::Value::Bool(true));
        let symbol = make_symbol(
            SymbolKind::Method,
            "csharp",
            vec![annotation("setup")],
            Some(metadata),
        );

        // Should be FixtureSetup from annotation, not TestCase from is_test
        let role = classify_test_role(&symbol, Some(&config));
        assert_eq!(role, Some(TestRole::FixtureSetup));
    }

    // ---------------------------------------------------------------
    // is_scorable_test tests
    // ---------------------------------------------------------------

    #[test]
    fn test_is_scorable_test_true_for_test_case() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "test_role".to_string(),
            serde_json::Value::String("test_case".to_string()),
        );
        let symbol = make_symbol(SymbolKind::Function, "rust", vec![], Some(metadata));

        assert!(is_scorable_test(&symbol));
    }

    #[test]
    fn test_is_scorable_test_true_for_parameterized_test() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "test_role".to_string(),
            serde_json::Value::String("parameterized_test".to_string()),
        );
        let symbol = make_symbol(SymbolKind::Method, "csharp", vec![], Some(metadata));

        assert!(is_scorable_test(&symbol));
    }

    #[test]
    fn test_is_scorable_test_false_for_fixture_setup() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "test_role".to_string(),
            serde_json::Value::String("fixture_setup".to_string()),
        );
        let symbol = make_symbol(SymbolKind::Method, "csharp", vec![], Some(metadata));

        assert!(!is_scorable_test(&symbol));
    }

    #[test]
    fn test_is_scorable_test_false_for_test_container() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "test_role".to_string(),
            serde_json::Value::String("test_container".to_string()),
        );
        let symbol = make_symbol(SymbolKind::Class, "csharp", vec![], Some(metadata));

        assert!(!is_scorable_test(&symbol));
    }

    #[test]
    fn test_is_scorable_test_false_no_metadata() {
        let symbol = make_symbol(SymbolKind::Function, "rust", vec![], None);

        assert!(!is_scorable_test(&symbol));
    }

    // ---------------------------------------------------------------
    // is_test_related tests
    // ---------------------------------------------------------------

    #[test]
    fn test_is_test_related_true_for_test_case() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "test_role".to_string(),
            serde_json::Value::String("test_case".to_string()),
        );
        let symbol = make_symbol(SymbolKind::Function, "rust", vec![], Some(metadata));

        assert!(is_test_related(&symbol));
    }

    #[test]
    fn test_is_test_related_true_for_fixture_setup() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "test_role".to_string(),
            serde_json::Value::String("fixture_setup".to_string()),
        );
        let symbol = make_symbol(SymbolKind::Method, "csharp", vec![], Some(metadata));

        assert!(is_test_related(&symbol));
    }

    #[test]
    fn test_is_test_related_true_for_legacy_is_test() {
        // Backward compat: symbols with only is_test (no test_role) are still test-related
        let mut metadata = HashMap::new();
        metadata.insert("is_test".to_string(), serde_json::Value::Bool(true));
        let symbol = make_symbol(SymbolKind::Function, "rust", vec![], Some(metadata));

        assert!(is_test_related(&symbol));
    }

    #[test]
    fn test_is_test_related_false_no_metadata() {
        let symbol = make_symbol(SymbolKind::Function, "rust", vec![], None);

        assert!(!is_test_related(&symbol));
    }

    #[test]
    fn test_is_test_related_false_is_test_false() {
        let mut metadata = HashMap::new();
        metadata.insert("is_test".to_string(), serde_json::Value::Bool(false));
        let symbol = make_symbol(SymbolKind::Function, "rust", vec![], Some(metadata));

        assert!(!is_test_related(&symbol));
    }

    // ---------------------------------------------------------------
    // classify_symbols_by_role (batch) tests
    // ---------------------------------------------------------------

    #[test]
    fn test_classify_symbols_by_role_sets_metadata() {
        let config = csharp_config();
        let mut configs = HashMap::new();
        configs.insert("csharp".to_string(), config);

        let mut symbols = vec![
            // Should get TestCase from [Fact]
            make_symbol(SymbolKind::Method, "csharp", vec![annotation("fact")], None),
            // Should get FixtureSetup from [SetUp]
            make_symbol(
                SymbolKind::Method,
                "csharp",
                vec![annotation("setup")],
                None,
            ),
            // No annotations, no is_test: should remain unchanged
            make_symbol(SymbolKind::Method, "csharp", vec![], None),
            // Convention-based: is_test=true, no annotations, no config match
            {
                let mut m = HashMap::new();
                m.insert("is_test".to_string(), serde_json::Value::Bool(true));
                make_symbol(SymbolKind::Function, "rust", vec![], Some(m))
            },
            // Container with test_fixture annotation
            make_symbol(
                SymbolKind::Class,
                "csharp",
                vec![annotation("test_fixture")],
                None,
            ),
        ];

        classify_symbols_by_role(&mut symbols, &configs);

        // Symbol 0: TestCase
        let meta0 = symbols[0].metadata.as_ref().unwrap();
        assert_eq!(
            meta0.get("test_role").unwrap().as_str().unwrap(),
            "test_case"
        );
        assert_eq!(meta0.get("is_test").unwrap().as_bool().unwrap(), true);

        // Symbol 1: FixtureSetup
        let meta1 = symbols[1].metadata.as_ref().unwrap();
        assert_eq!(
            meta1.get("test_role").unwrap().as_str().unwrap(),
            "fixture_setup"
        );
        assert_eq!(meta1.get("is_test").unwrap().as_bool().unwrap(), true);

        // Symbol 2: no role assigned
        assert!(symbols[2].metadata.is_none());

        // Symbol 3: convention fallback to TestCase
        let meta3 = symbols[3].metadata.as_ref().unwrap();
        assert_eq!(
            meta3.get("test_role").unwrap().as_str().unwrap(),
            "test_case"
        );
        assert_eq!(meta3.get("is_test").unwrap().as_bool().unwrap(), true);

        // Symbol 4: TestContainer
        let meta4 = symbols[4].metadata.as_ref().unwrap();
        assert_eq!(
            meta4.get("test_role").unwrap().as_str().unwrap(),
            "test_container"
        );
        assert_eq!(meta4.get("is_test").unwrap().as_bool().unwrap(), true);
    }

    #[test]
    fn test_classify_symbols_by_role_preserves_existing_metadata() {
        let config = csharp_config();
        let mut configs = HashMap::new();
        configs.insert("csharp".to_string(), config);

        let mut existing_meta = HashMap::new();
        existing_meta.insert(
            "custom_key".to_string(),
            serde_json::Value::String("custom_value".to_string()),
        );

        let mut symbols = vec![make_symbol(
            SymbolKind::Method,
            "csharp",
            vec![annotation("fact")],
            Some(existing_meta),
        )];

        classify_symbols_by_role(&mut symbols, &configs);

        let meta = symbols[0].metadata.as_ref().unwrap();
        // Original metadata preserved
        assert_eq!(
            meta.get("custom_key").unwrap().as_str().unwrap(),
            "custom_value"
        );
        // New metadata added
        assert_eq!(
            meta.get("test_role").unwrap().as_str().unwrap(),
            "test_case"
        );
    }

    // ---------------------------------------------------------------
    // TestRoleConfig::classify_annotation tests
    // ---------------------------------------------------------------

    #[test]
    fn test_classify_annotation_priority_order() {
        // If the same key is in both test_case and fixture_setup, test_case wins
        let config = TestRoleConfig {
            test_case: HashSet::from(["ambiguous".to_string()]),
            fixture_setup: HashSet::from(["ambiguous".to_string()]),
            ..Default::default()
        };

        assert_eq!(
            config.classify_annotation("ambiguous"),
            Some(TestRole::TestCase)
        );
    }

    #[test]
    fn test_classify_annotation_unknown_key() {
        let config = csharp_config();
        assert_eq!(config.classify_annotation("unknown_annotation"), None);
    }

    #[test]
    fn test_classify_annotation_empty_config() {
        let config = TestRoleConfig::default();
        assert_eq!(config.classify_annotation("test"), None);
    }
}
