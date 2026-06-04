//! Test-symbol exclusion coverage for embedding inputs.

use super::*;

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
    let batch = prepare_batch_for_embedding(
        &symbols,
        None,
        &HashMap::new(),
        &HashMap::new(),
        &HashMap::new(),
    );

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
