//! Variable embedding budget and scoring coverage.

use super::*;

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
    let selected =
        select_budgeted_variables(&symbols, &reference_scores, base_count, &policy, None);

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
