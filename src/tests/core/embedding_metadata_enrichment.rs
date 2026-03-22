//! Tests for variable embedding budget, test symbol exclusion, callee/field
//! enrichment, and extract_doc_excerpt (split from embedding_metadata.rs).

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::embeddings::metadata::{
        VariableEmbeddingPolicy, extract_doc_excerpt, has_simple_default_literal,
        is_test_symbol_for_embedding, prepare_batch_for_embedding, select_budgeted_variables,
    };
    use crate::extractors::{Symbol, SymbolKind};

    /// Helper: create a minimal test symbol.
    fn make_symbol(
        id: &str,
        name: &str,
        kind: SymbolKind,
        signature: Option<&str>,
        doc_comment: Option<&str>,
    ) -> Symbol {
        Symbol {
            id: id.to_string(),
            name: name.to_string(),
            kind,
            language: "rust".to_string(),
            file_path: "src/lib.rs".to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 10,
            end_column: 0,
            start_byte: 0,
            end_byte: 100,
            signature: signature.map(|s| s.to_string()),
            doc_comment: doc_comment.map(|s| s.to_string()),
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
            content_type: None,
        }
    }

    fn make_symbol_with_lang(id: &str, name: &str, kind: SymbolKind, language: &str) -> Symbol {
        let mut s = make_symbol(id, name, kind, None, None);
        s.language = language.to_string();
        s
    }

    // =========================================================================
    // extract_doc_excerpt
    // =========================================================================

    #[test]
    fn test_extract_doc_excerpt_multi_line() {
        let doc = "/// Record a completed tool call.\n/// Bumps in-memory atomics synchronously, then spawns async task\n/// for source_bytes lookup + SQLite write.";
        let excerpt = extract_doc_excerpt(doc);
        assert!(
            excerpt.contains("Record a completed tool call"),
            "First sentence should be present: {excerpt}"
        );
        assert!(
            excerpt.contains("SQLite write"),
            "Later sentences should be present: {excerpt}"
        );
    }

    #[test]
    fn test_extract_doc_excerpt_strips_rust_prefixes() {
        let doc = "/// First line.\n/// Second line.";
        let excerpt = extract_doc_excerpt(doc);
        assert!(!excerpt.contains("///"), "Should strip /// prefix: {excerpt}");
        assert!(excerpt.contains("First line."));
        assert!(excerpt.contains("Second line."));
    }

    #[test]
    fn test_extract_doc_excerpt_strips_csharp_xml_tags() {
        let doc = "/// <summary>\n/// Handles authentication.\n/// </summary>\n/// <param name=\"token\">The auth token.</param>";
        let excerpt = extract_doc_excerpt(doc);
        assert!(!excerpt.contains("<summary>"), "Should strip XML tags: {excerpt}");
        assert!(!excerpt.contains("<param"), "Should strip param tags: {excerpt}");
        assert!(excerpt.contains("Handles authentication"), "Content should survive: {excerpt}");
    }

    #[test]
    fn test_extract_doc_excerpt_handles_python_docstring() {
        let doc = "# Process the input data.\n# Returns the transformed result.";
        let excerpt = extract_doc_excerpt(doc);
        assert!(excerpt.contains("Process the input data"), "Should strip # prefix: {excerpt}");
        assert!(excerpt.contains("Returns the transformed result"), "Second line should be present: {excerpt}");
    }

    #[test]
    fn test_extract_doc_excerpt_truncates_at_budget() {
        // Create a doc longer than MAX_DOC_EXCERPT_CHARS (300)
        let long_line = "/// ".to_string() + &"word ".repeat(80); // ~400 chars of content
        let excerpt = extract_doc_excerpt(&long_line);
        assert!(
            excerpt.len() <= 300,
            "Should truncate to 300 bytes: len={}, excerpt: {excerpt}",
            excerpt.len()
        );
    }

    #[test]
    fn test_extract_doc_excerpt_empty_doc() {
        assert_eq!(extract_doc_excerpt(""), "");
        assert_eq!(extract_doc_excerpt("///"), "");
        assert_eq!(extract_doc_excerpt("/// \n/// "), "");
    }

    #[test]
    fn test_extract_doc_excerpt_jsdoc_block() {
        let doc = "/**\n * Initialize the connection pool.\n * Validates config before connecting.\n */";
        let excerpt = extract_doc_excerpt(doc);
        assert!(excerpt.contains("Initialize the connection pool"), "Should handle JSDoc: {excerpt}");
        assert!(excerpt.contains("Validates config"), "Second line: {excerpt}");
    }

    // =========================================================================
    // select_budgeted_variables
    // =========================================================================

    #[test]
    fn test_select_budgeted_variables_returns_empty_when_policy_disabled_or_zero_cap() {
        let symbols = vec![make_symbol(
            "var_1",
            "customer_id",
            SymbolKind::Variable,
            Some("let customer_id = request.customer_id;"),
            None,
        )];
        let reference_scores = HashMap::from([("var_1".to_string(), 0.90_f64)]);

        let disabled = VariableEmbeddingPolicy {
            enabled: false,
            max_ratio: 1.0,
        };
        let zero_cap = VariableEmbeddingPolicy {
            enabled: true,
            max_ratio: 0.0,
        };

        assert!(
            select_budgeted_variables(&symbols, &reference_scores, 10, &disabled, None).is_empty(),
            "Disabled policy should return no variables"
        );
        assert!(
            select_budgeted_variables(&symbols, &reference_scores, 10, &zero_cap, None).is_empty(),
            "Zero cap policy should return no variables"
        );
    }

    #[test]
    fn test_select_budgeted_variables_only_considers_variable_symbols() {
        let symbols = vec![
            make_symbol("fn_1", "process_order", SymbolKind::Function, None, None),
            make_symbol(
                "var_1",
                "order_total",
                SymbolKind::Variable,
                Some("let order_total = line_items.sum();"),
                None,
            ),
            make_symbol("class_1", "OrderService", SymbolKind::Class, None, None),
        ];

        let reference_scores = HashMap::from([
            ("fn_1".to_string(), 0.99_f64),
            ("var_1".to_string(), 0.10_f64),
            ("class_1".to_string(), 0.99_f64),
        ]);

        let policy = VariableEmbeddingPolicy {
            enabled: true,
            max_ratio: 1.0,
        };

        let selected = select_budgeted_variables(&symbols, &reference_scores, 1, &policy, None);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].0, "var_1");
        assert!(selected[0].1.contains("order_total"));
    }

    #[test]
    fn test_select_budgeted_variables_includes_reference_score_contribution() {
        let symbols = vec![
            make_symbol(
                "var_low_ref",
                "customer_status",
                SymbolKind::Variable,
                Some("let customer_status = fetch_customer_status(user);"),
                None,
            ),
            make_symbol(
                "var_high_ref",
                "customer_status_cached",
                SymbolKind::Variable,
                Some("let customer_status_cached = fetch_customer_status(user);"),
                None,
            ),
        ];

        let reference_scores = HashMap::from([
            ("var_low_ref".to_string(), 0.10_f64),
            ("var_high_ref".to_string(), 0.95_f64),
        ]);

        let policy = VariableEmbeddingPolicy {
            enabled: true,
            max_ratio: 1.0,
        };

        let selected = select_budgeted_variables(&symbols, &reference_scores, 1, &policy, None);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].0, "var_high_ref");
    }

    #[test]
    fn test_select_budgeted_variables_boosts_descriptive_names() {
        let symbols = vec![
            make_symbol(
                "var_descriptive",
                "order_total",
                SymbolKind::Variable,
                Some("let order_total = line_items.sum();"),
                None,
            ),
            make_symbol(
                "var_short",
                "state",
                SymbolKind::Variable,
                Some("let state = 0;"),
                None,
            ),
        ];

        let policy = VariableEmbeddingPolicy {
            enabled: true,
            max_ratio: 1.0,
        };

        let selected = select_budgeted_variables(&symbols, &HashMap::new(), 1, &policy, None);
        assert_eq!(selected.len(), 1);
        assert_eq!(
            selected[0].0, "var_descriptive",
            "Snake_case name should receive descriptiveness boost over short name with default penalty"
        );
    }

    #[test]
    fn test_select_budgeted_variables_penalizes_noise_variables() {
        let symbols = vec![
            make_symbol(
                "var_noise",
                "i",
                SymbolKind::Variable,
                Some("let i = 0;"),
                None,
            ),
            make_symbol(
                "var_signal",
                "customer_id",
                SymbolKind::Variable,
                Some("let customer_id = request.customer_id;"),
                None,
            ),
        ];

        let reference_scores = HashMap::from([
            ("var_noise".to_string(), 0.40_f64),
            ("var_signal".to_string(), 0.40_f64),
        ]);
        let policy = VariableEmbeddingPolicy {
            enabled: true,
            max_ratio: 1.0,
        };

        let selected = select_budgeted_variables(&symbols, &reference_scores, 1, &policy, None);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].0, "var_signal");
    }

    #[test]
    fn test_select_budgeted_variables_prefers_high_signal_over_local_low_signal() {
        let local_low_signal = make_symbol(
            "var_local",
            "i",
            SymbolKind::Variable,
            Some("let i = 0;"),
            None,
        );
        let high_signal = make_symbol(
            "var_high",
            "customer_credit_score",
            SymbolKind::Variable,
            Some("let customer_credit_score = risk_model.compute(user);"),
            None,
        );

        let symbols = vec![local_low_signal, high_signal];
        let reference_scores = HashMap::from([
            ("var_local".to_string(), 0.05_f64),
            ("var_high".to_string(), 0.95_f64),
        ]);
        let policy = VariableEmbeddingPolicy {
            enabled: true,
            max_ratio: 1.0,
        };

        let selected = select_budgeted_variables(&symbols, &reference_scores, 1, &policy, None);
        assert_eq!(selected.len(), 1, "Expected exactly one selected variable");
        assert_eq!(selected[0].0, "var_high");
    }

    #[test]
    fn test_select_budgeted_variables_enforces_budget_cap() {
        let symbols = vec![
            make_symbol("var_1", "alpha", SymbolKind::Variable, None, None),
            make_symbol("var_2", "beta", SymbolKind::Variable, None, None),
            make_symbol("var_3", "gamma", SymbolKind::Variable, None, None),
            make_symbol("var_4", "delta", SymbolKind::Variable, None, None),
            make_symbol("var_5", "epsilon", SymbolKind::Variable, None, None),
        ];
        let reference_scores = HashMap::from([
            ("var_1".to_string(), 0.90_f64),
            ("var_2".to_string(), 0.80_f64),
            ("var_3".to_string(), 0.70_f64),
            ("var_4".to_string(), 0.60_f64),
            ("var_5".to_string(), 0.50_f64),
        ]);
        let policy = VariableEmbeddingPolicy {
            enabled: true,
            max_ratio: 0.20,
        };

        let base_count = 11;
        let cap = ((base_count as f64) * policy.max_ratio).floor() as usize;
        let selected = select_budgeted_variables(&symbols, &reference_scores, base_count, &policy, None);

        assert_eq!(
            selected.len(),
            cap,
            "Expected selection to fill cap when enough candidates exist"
        );
        assert!(
            selected.len() <= cap,
            "Selected {} variables but cap is {}",
            selected.len(),
            cap
        );
    }

    #[test]
    fn test_select_budgeted_variables_tie_breaks_deterministically_by_score_then_id() {
        let symbols = vec![
            make_symbol("var_b", "beta", SymbolKind::Variable, None, None),
            make_symbol("var_top", "top", SymbolKind::Variable, None, None),
            make_symbol("var_a", "alpha", SymbolKind::Variable, None, None),
        ];
        let reference_scores = HashMap::from([
            ("var_b".to_string(), 0.50_f64),
            ("var_top".to_string(), 0.90_f64),
            ("var_a".to_string(), 0.50_f64),
        ]);
        let policy = VariableEmbeddingPolicy {
            enabled: true,
            max_ratio: 1.0,
        };

        let selected = select_budgeted_variables(&symbols, &reference_scores, 3, &policy, None);
        let selected_ids: Vec<&str> = selected.iter().map(|(id, _)| id.as_str()).collect();

        assert_eq!(selected_ids, vec!["var_top", "var_a", "var_b"]);
    }

    #[test]
    fn test_select_budgeted_variables_descriptiveness_uses_name_structure() {
        let symbols = vec![
            make_symbol(
                "var_short",
                "rapidly",
                SymbolKind::Variable,
                Some("let rapidly = compute();"),
                None,
            ),
            make_symbol(
                "var_snake",
                "state_value",
                SymbolKind::Variable,
                Some("let state_value = load_state();"),
                None,
            ),
        ];

        let reference_scores = HashMap::from([
            ("var_short".to_string(), 0.40_f64),
            ("var_snake".to_string(), 0.40_f64),
        ]);

        let policy = VariableEmbeddingPolicy {
            enabled: true,
            max_ratio: 1.0,
        };

        let selected = select_budgeted_variables(&symbols, &reference_scores, 1, &policy, None);
        assert_eq!(selected.len(), 1);
        assert_eq!(
            selected[0].0, "var_snake",
            "Snake_case name gets descriptiveness boost; short single-word name does not"
        );
    }

    #[test]
    fn test_select_budgeted_variables_boosts_both_camel_and_snake_case_descriptive_names() {
        let symbols = vec![
            make_symbol(
                "var_camel",
                "connectionPool",
                SymbolKind::Variable,
                Some("let connectionPool = create_pool();"),
                None,
            ),
            make_symbol(
                "var_snake",
                "connection_pool",
                SymbolKind::Variable,
                Some("let connection_pool = create_pool();"),
                None,
            ),
            make_symbol(
                "var_short",
                "pool",
                SymbolKind::Variable,
                Some("let pool = get();"),
                None,
            ),
        ];

        let reference_scores = HashMap::from([
            ("var_camel".to_string(), 0.30_f64),
            ("var_snake".to_string(), 0.30_f64),
            ("var_short".to_string(), 0.30_f64),
        ]);

        let policy = VariableEmbeddingPolicy {
            enabled: true,
            max_ratio: 1.0,
        };

        let selected = select_budgeted_variables(&symbols, &reference_scores, 2, &policy, None);
        let selected_ids: Vec<&str> = selected.iter().map(|(id, _)| id.as_str()).collect();

        assert_eq!(selected_ids.len(), 2);
        assert!(
            selected_ids.contains(&"var_camel"),
            "camelCase name (len>=12) should get descriptiveness boost"
        );
        assert!(
            selected_ids.contains(&"var_snake"),
            "snake_case name (has _) should get descriptiveness boost"
        );
    }

    #[test]
    fn test_select_budgeted_variables_handles_non_english_identifier_and_docs() {
        let symbols = vec![
            make_symbol(
                "var_non_english",
                "estadoUsuario",
                SymbolKind::Variable,
                Some("let estadoUsuario = obtener_estado(usuario);"),
                Some("/// Devuelve el estado actual del usuario."),
            ),
            make_symbol(
                "var_ascii",
                "state_cache",
                SymbolKind::Variable,
                Some("let state_cache = load_state();"),
                Some("/// Stores cached state."),
            ),
        ];

        let reference_scores = HashMap::from([
            ("var_non_english".to_string(), 0.50_f64),
            ("var_ascii".to_string(), 0.49_f64),
        ]);

        let policy = VariableEmbeddingPolicy {
            enabled: true,
            max_ratio: 1.0,
        };

        let selected = select_budgeted_variables(&symbols, &reference_scores, 1, &policy, None);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].0, "var_non_english");
    }

    #[test]
    fn test_select_budgeted_variables_penalizes_default_values_across_signature_styles() {
        let symbols = vec![
            make_symbol(
                "var_default_equals_spaced",
                "configValue",
                SymbolKind::Variable,
                Some("config_value = false"),
                None,
            ),
            make_symbol(
                "var_default_equals_compact",
                "limitValue",
                SymbolKind::Variable,
                Some("limit_value=false"),
                None,
            ),
            make_symbol(
                "var_default_colon_equals",
                "modeValue",
                SymbolKind::Variable,
                Some("mode_value:=0"),
                None,
            ),
            make_symbol(
                "var_no_default",
                "resultValue",
                SymbolKind::Variable,
                Some("result_value: bool"),
                None,
            ),
        ];

        let reference_scores = HashMap::from([
            ("var_default_equals_spaced".to_string(), 0.40_f64),
            ("var_default_equals_compact".to_string(), 0.40_f64),
            ("var_default_colon_equals".to_string(), 0.40_f64),
            ("var_no_default".to_string(), 0.40_f64),
        ]);

        let policy = VariableEmbeddingPolicy {
            enabled: true,
            max_ratio: 1.0,
        };

        let selected = select_budgeted_variables(&symbols, &reference_scores, 1, &policy, None);
        assert_eq!(selected.len(), 1);
        assert_eq!(
            selected[0].0, "var_no_default",
            "Mixed default styles should all receive the same penalty"
        );
    }

    #[test]
    fn test_select_budgeted_variables_noise_penalty_no_double_dip_with_short_name() {
        let symbols = vec![
            make_symbol(
                "var_noise",
                "i",
                SymbolKind::Variable,
                Some("let i = get_index();"),
                None,
            ),
            make_symbol(
                "var_baseline",
                "count",
                SymbolKind::Variable,
                Some("let count = tally();"),
                None,
            ),
        ];

        let reference_scores = HashMap::from([
            ("var_noise".to_string(), 0.55_f64),
            ("var_baseline".to_string(), 0.0_f64),
        ]);
        let policy = VariableEmbeddingPolicy {
            enabled: true,
            max_ratio: 1.0,
        };

        let selected = select_budgeted_variables(&symbols, &reference_scores, 2, &policy, None);
        assert_eq!(selected.len(), 2);
        assert_eq!(
            selected[0].0, "var_noise",
            "Noise name 'i' with ref_score=0.55 should rank first (penalty=0.50, score=0.05); \
             double-dip would give penalty=0.65, score=-0.10 and push it below baseline"
        );
    }

    #[test]
    fn test_select_budgeted_variables_unknown_short_name_gets_smaller_penalty() {
        let symbols = vec![
            make_symbol(
                "var_short",
                "mx",
                SymbolKind::Variable,
                Some("let mx = compute_max();"),
                None,
            ),
            make_symbol(
                "var_baseline",
                "total",
                SymbolKind::Variable,
                Some("let total = sum();"),
                None,
            ),
        ];

        let reference_scores = HashMap::from([
            ("var_short".to_string(), 0.25_f64),
            ("var_baseline".to_string(), 0.0_f64),
        ]);
        let policy = VariableEmbeddingPolicy {
            enabled: true,
            max_ratio: 1.0,
        };

        let selected = select_budgeted_variables(&symbols, &reference_scores, 2, &policy, None);
        assert_eq!(selected.len(), 2);
        assert_eq!(
            selected[0].0, "var_short",
            "Unknown short name 'mx' should get -0.20 penalty (score=0.05), not -0.50 (score=-0.25); \
             it should rank above the zero-score baseline"
        );
    }

    // ---- has_simple_default_literal unit tests ----

    #[test]
    fn test_has_simple_default_literal_matches() {
        assert!(has_simple_default_literal("let x = 0"));
        assert!(has_simple_default_literal("let x = 1"));
        assert!(has_simple_default_literal("let x = 0;"));
        assert!(has_simple_default_literal("x = true"));
        assert!(has_simple_default_literal("x = false"));
        assert!(has_simple_default_literal("x = None"));
        assert!(has_simple_default_literal("x = null"));
        assert!(has_simple_default_literal("x = nil"));
        assert!(has_simple_default_literal("x = True"));
        assert!(has_simple_default_literal("x = FALSE"));
        assert!(has_simple_default_literal("x = \"\""));
        assert!(has_simple_default_literal("x = ''"));
        assert!(has_simple_default_literal("x = {}"));
        assert!(has_simple_default_literal("x = []"));
    }

    #[test]
    fn test_has_simple_default_literal_rejects_comparison_operators() {
        assert!(!has_simple_default_literal("x == 0"));
        assert!(!has_simple_default_literal("x != 0"));
        assert!(!has_simple_default_literal("x >= 0"));
        assert!(!has_simple_default_literal("x <= 0"));
        assert!(!has_simple_default_literal("if x == true"));
        assert!(!has_simple_default_literal("x != null"));
    }

    #[test]
    fn test_has_simple_default_literal_rejects_non_defaults() {
        assert!(!has_simple_default_literal("x = some_function()"));
        assert!(!has_simple_default_literal("x = truthy"));
        assert!(!has_simple_default_literal("x = none_value"));
        assert!(!has_simple_default_literal("x = 0x1234"));
        assert!(!has_simple_default_literal("x = 42"));
        assert!(!has_simple_default_literal("no assignment here"));
    }

    // =========================================================================
    // Test symbol exclusion from embeddings
    // =========================================================================

    #[test]
    fn test_is_test_symbol_by_metadata() {
        let mut sym = make_symbol("t1", "test_add", SymbolKind::Function, None, None);
        sym.metadata = Some(HashMap::from([(
            "is_test".to_string(),
            serde_json::Value::Bool(true),
        )]));

        assert!(is_test_symbol_for_embedding(&sym));
    }

    #[test]
    fn test_is_test_symbol_by_path() {
        let mut sym = make_symbol("t2", "MyHelper", SymbolKind::Class, None, None);
        sym.file_path = "test/helpers/my_helper.rb".to_string();

        assert!(is_test_symbol_for_embedding(&sym));
    }

    #[test]
    fn test_is_test_symbol_by_path_csharp_convention() {
        let mut sym = make_symbol("t3", "SerializerTests", SymbolKind::Class, None, None);
        sym.file_path = "MyProject.Tests/SerializerTests.cs".to_string();

        assert!(is_test_symbol_for_embedding(&sym));
    }

    #[test]
    fn test_is_not_test_symbol_for_source_code() {
        let sym = make_symbol("s1", "Router", SymbolKind::Module, None, None);
        assert!(!is_test_symbol_for_embedding(&sym));
    }

    #[test]
    fn test_is_not_test_symbol_metadata_false() {
        let mut sym = make_symbol("s2", "run", SymbolKind::Function, None, None);
        sym.metadata = Some(HashMap::from([(
            "is_test".to_string(),
            serde_json::Value::Bool(false),
        )]));

        assert!(!is_test_symbol_for_embedding(&sym));
    }

    #[test]
    fn test_prepare_batch_excludes_test_symbols() {
        let mut test_func = make_symbol("t1", "test_add", SymbolKind::Function, None, None);
        test_func.metadata = Some(HashMap::from([(
            "is_test".to_string(),
            serde_json::Value::Bool(true),
        )]));

        let mut test_class = make_symbol("t2", "RouterTest", SymbolKind::Class, None, None);
        test_class.file_path = "tests/router_test.rs".to_string();

        let source_func = make_symbol("s1", "add", SymbolKind::Function, None, None);
        let source_class = make_symbol("s2", "Router", SymbolKind::Class, None, None);

        let symbols = vec![test_func, test_class, source_func, source_class];
        let batch = prepare_batch_for_embedding(&symbols, None, &HashMap::new(), &HashMap::new());

        assert_eq!(batch.len(), 2, "Should exclude both test symbols");
        let ids: Vec<&str> = batch.iter().map(|(id, _)| id.as_str()).collect();
        assert!(ids.contains(&"s1"));
        assert!(ids.contains(&"s2"));
        assert!(!ids.contains(&"t1"));
        assert!(!ids.contains(&"t2"));
    }

    #[test]
    fn test_select_budgeted_variables_excludes_test_variables() {
        let source_var = make_symbol("v1", "config_path", SymbolKind::Variable, None, None);

        let mut test_var = make_symbol("v2", "test_config", SymbolKind::Variable, None, None);
        test_var.file_path = "tests/test_config.rs".to_string();

        let mut test_var_meta = make_symbol("v3", "mock_data", SymbolKind::Variable, None, None);
        test_var_meta.metadata = Some(HashMap::from([(
            "is_test".to_string(),
            serde_json::Value::Bool(true),
        )]));

        let symbols = vec![source_var, test_var, test_var_meta];
        let ref_scores = HashMap::new();
        let policy = VariableEmbeddingPolicy {
            enabled: true,
            max_ratio: 1.0,
        };

        let selected = select_budgeted_variables(&symbols, &ref_scores, 10, &policy, None);

        assert_eq!(selected.len(), 1, "Should only include source variable");
        assert_eq!(selected[0].0, "v1");
    }

    // =========================================================================
    // Callee enrichment for functions/methods
    // =========================================================================

    #[test]
    fn test_prepare_batch_enriches_function_with_callees() {
        let func = make_symbol(
            "f1",
            "record_tool_call",
            SymbolKind::Function,
            Some("pub fn record_tool_call(&self, tool_name: &str)"),
            Some("/// Record a completed tool call."),
        );
        let callee_func = make_symbol(
            "f2",
            "insert_tool_call",
            SymbolKind::Function,
            None,
            None,
        );
        let callee_func2 = make_symbol(
            "f3",
            "get_total_file_sizes",
            SymbolKind::Function,
            None,
            None,
        );

        let symbols = vec![func, callee_func, callee_func2];

        let mut callees_by_symbol: HashMap<String, Vec<String>> = HashMap::new();
        callees_by_symbol.insert(
            "f1".to_string(),
            vec!["insert_tool_call".to_string(), "get_total_file_sizes".to_string()],
        );

        let batch = prepare_batch_for_embedding(&symbols, None, &callees_by_symbol, &HashMap::new());
        assert_eq!(batch.len(), 3);

        let (_, text) = batch.iter().find(|(id, _)| id == "f1").unwrap();
        assert!(
            text.contains("calls:"),
            "Function should have callee enrichment: {text}"
        );
        assert!(
            text.contains("insert_tool_call"),
            "Should contain callee name: {text}"
        );
        assert!(
            text.contains("get_total_file_sizes"),
            "Should contain second callee name: {text}"
        );
    }

    #[test]
    fn test_prepare_batch_enriches_method_with_callees() {
        let method = make_symbol(
            "m1",
            "process",
            SymbolKind::Method,
            Some("pub fn process(&self)"),
            None,
        );
        let symbols = vec![method];
        let mut callees = HashMap::new();
        callees.insert("m1".to_string(), vec!["save".to_string(), "validate".to_string()]);

        let batch = prepare_batch_for_embedding(&symbols, None, &callees, &HashMap::new());
        let (_, text) = &batch[0];
        assert!(
            text.contains("calls: save, validate"),
            "Method should have sorted callee enrichment: {text}"
        );
    }

    #[test]
    fn test_prepare_batch_container_no_callee_enrichment() {
        let class = make_symbol_with_lang("c1", "MyService", SymbolKind::Class, "csharp");
        let symbols = vec![class];
        let mut callees = HashMap::new();
        callees.insert("c1".to_string(), vec!["something".to_string()]);

        let batch = prepare_batch_for_embedding(&symbols, None, &callees, &HashMap::new());
        let (_, text) = &batch[0];
        assert!(
            !text.contains("calls:"),
            "Container symbols should NOT get callee enrichment: {text}"
        );
    }

    #[test]
    fn test_enriched_function_with_callees_uses_expanded_budget() {
        let long_doc = "/// Orchestrates a complex multi-stage data processing pipeline that coordinates extraction from multiple sources. Manages transformation rules, validates intermediate results against business constraints, and loads final output into the target database system. Implements comprehensive retry logic for transient failures with exponential backoff.";
        let func = make_symbol(
            "f1",
            "orchestrate_complex_pipeline",
            SymbolKind::Function,
            Some("pub async fn orchestrate_complex_pipeline(handler: &JulieServerHandler, config: &PipelineConfig, options: &ProcessingOptions) -> Result<PipelineOutput>"),
            Some(long_doc),
        );
        let symbols = vec![func];
        let mut callees = HashMap::new();
        callees.insert("f1".to_string(), vec![
            "connect_to_source_database".to_string(),
            "extract_source_records".to_string(),
            "transform_with_business_rules".to_string(),
            "validate_intermediate_output".to_string(),
            "load_into_target_database".to_string(),
            "retry_with_exponential_backoff".to_string(),
        ]);

        let batch = prepare_batch_for_embedding(&symbols, None, &callees, &HashMap::new());
        let (_, text) = &batch[0];

        assert!(
            text.contains("retry_with_exponential_backoff"),
            "Last callee should not be truncated with expanded budget: {text}"
        );
        assert!(
            text.contains("loads final output"),
            "Multi-sentence doc should survive within budget: {text}"
        );
        assert!(
            text.len() > 600,
            "Text should exceed old 600-char limit: len={}, text: {text}",
            text.len()
        );
    }

    // =========================================================================
    // Field access enrichment
    // =========================================================================

    #[test]
    fn test_prepare_batch_enriches_function_with_field_accesses() {
        let func = make_symbol(
            "f1",
            "record_tool_call",
            SymbolKind::Function,
            Some("pub fn record_tool_call(&self, tool_name: &str)"),
            Some("/// Record a completed tool call."),
        );
        let symbols = vec![func];

        let callees_by_symbol: HashMap<String, Vec<String>> = HashMap::new();
        let mut fields_by_symbol: HashMap<String, Vec<String>> = HashMap::new();
        fields_by_symbol.insert(
            "f1".to_string(),
            vec![
                "session_metrics".to_string(),
                "db".to_string(),
                "output_bytes".to_string(),
            ],
        );

        let batch =
            prepare_batch_for_embedding(&symbols, None, &callees_by_symbol, &fields_by_symbol);
        assert_eq!(batch.len(), 1);

        let (_, text) = &batch[0];
        assert!(
            text.contains("fields:"),
            "Function should have field access enrichment: {text}"
        );
        assert!(
            text.contains("session_metrics"),
            "Should contain field name 'session_metrics': {text}"
        );
        assert!(
            text.contains("db"),
            "Should contain field name 'db': {text}"
        );
    }

    #[test]
    fn test_prepare_batch_no_field_enrichment_for_containers() {
        let class = make_symbol_with_lang("c1", "MyService", SymbolKind::Class, "csharp");
        let symbols = vec![class];

        let callees_by_symbol: HashMap<String, Vec<String>> = HashMap::new();
        let mut fields_by_symbol: HashMap<String, Vec<String>> = HashMap::new();
        fields_by_symbol.insert("c1".to_string(), vec!["some_field".to_string()]);

        let batch =
            prepare_batch_for_embedding(&symbols, None, &callees_by_symbol, &fields_by_symbol);
        let (_, text) = &batch[0];

        assert!(
            !text.contains("fields:"),
            "Containers should NOT get field access enrichment (they use properties:): {text}"
        );
    }

    #[test]
    fn test_prepare_batch_field_enrichment_combined_with_callees() {
        let func = make_symbol(
            "f1",
            "process_data",
            SymbolKind::Method,
            Some("pub fn process_data(&self)"),
            None,
        );
        let symbols = vec![func];

        let mut callees_by_symbol: HashMap<String, Vec<String>> = HashMap::new();
        callees_by_symbol.insert("f1".to_string(), vec!["save".to_string()]);

        let mut fields_by_symbol: HashMap<String, Vec<String>> = HashMap::new();
        fields_by_symbol.insert("f1".to_string(), vec!["config".to_string()]);

        let batch =
            prepare_batch_for_embedding(&symbols, None, &callees_by_symbol, &fields_by_symbol);
        let (_, text) = &batch[0];

        assert!(
            text.contains("calls:") && text.contains("fields:"),
            "Should have both callee and field enrichment: {text}"
        );
    }
}
