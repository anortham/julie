//! Batch embedding enrichment coverage.

use super::*;

#[test]
fn test_prepare_batch_enriches_function_with_callees() {
    let func = make_symbol(
        "f1",
        "record_tool_call",
        SymbolKind::Function,
        Some("pub fn record_tool_call(&self, tool_name: &str)"),
        Some("/// Record a completed tool call."),
    );
    let callee_func = make_symbol("f2", "insert_tool_call", SymbolKind::Function, None, None);
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
        vec![
            "insert_tool_call".to_string(),
            "get_total_file_sizes".to_string(),
        ],
    );

    let batch = prepare_batch_for_embedding(
        &symbols,
        None,
        &callees_by_symbol,
        &HashMap::new(),
        &HashMap::new(),
    );
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
    callees.insert(
        "m1".to_string(),
        vec!["save".to_string(), "validate".to_string()],
    );

    let batch =
        prepare_batch_for_embedding(&symbols, None, &callees, &HashMap::new(), &HashMap::new());
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

    let batch =
        prepare_batch_for_embedding(&symbols, None, &callees, &HashMap::new(), &HashMap::new());
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
        Some(
            "pub async fn orchestrate_complex_pipeline(handler: &JulieServerHandler, config: &PipelineConfig, options: &ProcessingOptions) -> Result<PipelineOutput>",
        ),
        Some(long_doc),
    );
    let symbols = vec![func];
    let mut callees = HashMap::new();
    callees.insert(
        "f1".to_string(),
        vec![
            "connect_to_source_database".to_string(),
            "extract_source_records".to_string(),
            "transform_with_business_rules".to_string(),
            "validate_intermediate_output".to_string(),
            "load_into_target_database".to_string(),
            "retry_with_exponential_backoff".to_string(),
        ],
    );

    let batch =
        prepare_batch_for_embedding(&symbols, None, &callees, &HashMap::new(), &HashMap::new());
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

    let batch = prepare_batch_for_embedding(
        &symbols,
        None,
        &callees_by_symbol,
        &fields_by_symbol,
        &HashMap::new(),
    );
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

    let batch = prepare_batch_for_embedding(
        &symbols,
        None,
        &callees_by_symbol,
        &fields_by_symbol,
        &HashMap::new(),
    );
    let (_, text) = &batch[0];

    assert!(
        !text.contains("fields:"),
        "Containers should NOT get field access enrichment from fields_by_symbol (no child fields in this test): {text}"
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

    let batch = prepare_batch_for_embedding(
        &symbols,
        None,
        &callees_by_symbol,
        &fields_by_symbol,
        &HashMap::new(),
    );
    let (_, text) = &batch[0];

    assert!(
        text.contains("calls:") && text.contains("fields:"),
        "Should have both callee and field enrichment: {text}"
    );
}

#[test]
fn test_prepare_batch_enriches_trait_with_implementors() {
    let trait_sym = make_symbol_with_lang("t1", "EmbeddingProvider", SymbolKind::Trait, "rust");
    let mut method1 = make_symbol_with_lang("m1", "embed_query", SymbolKind::Method, "rust");
    method1.parent_id = Some("t1".to_string());
    let mut method2 = make_symbol_with_lang("m2", "embed_batch", SymbolKind::Method, "rust");
    method2.parent_id = Some("t1".to_string());

    let symbols = vec![trait_sym, method1, method2];
    let mut implementors: HashMap<String, Vec<String>> = HashMap::new();
    implementors.insert(
        "t1".to_string(),
        vec![
            "SidecarEmbeddingProvider".to_string(),
            "PartialProvider".to_string(),
        ],
    );

    let batch = prepare_batch_for_embedding(
        &symbols,
        None,
        &HashMap::new(),
        &HashMap::new(),
        &implementors,
    );
    // trait + 2 methods are all embeddable kinds
    assert_eq!(batch.len(), 3);
    let (_, text) = batch.iter().find(|(id, _)| id == "t1").unwrap();
    assert!(
        text.contains("implemented_by: SidecarEmbeddingProvider, PartialProvider"),
        "Expected implementor names in trait embedding text, got: {text}"
    );
    assert!(
        text.contains("methods: embed_query, embed_batch"),
        "Expected child methods preserved, got: {text}"
    );
}

#[test]
fn test_prepare_batch_enriches_interface_with_implementors() {
    let iface = make_symbol_with_lang("i1", "ISearchService", SymbolKind::Interface, "csharp");
    let mut method = make_symbol_with_lang("m1", "Search", SymbolKind::Method, "csharp");
    method.parent_id = Some("i1".to_string());

    let symbols = vec![iface, method];
    let mut implementors: HashMap<String, Vec<String>> = HashMap::new();
    implementors.insert("i1".to_string(), vec!["LuceneSearchService".to_string()]);

    let batch = prepare_batch_for_embedding(
        &symbols,
        None,
        &HashMap::new(),
        &HashMap::new(),
        &implementors,
    );
    // interface + method are both embeddable
    assert_eq!(batch.len(), 2);
    let (_, text) = batch.iter().find(|(id, _)| id == "i1").unwrap();
    assert!(
        text.contains("implemented_by: LuceneSearchService"),
        "Expected implementor name, got: {text}"
    );
}

#[test]
fn test_prepare_batch_no_implementor_enrichment_for_class() {
    let class = make_symbol_with_lang("c1", "MyService", SymbolKind::Class, "rust");
    let symbols = vec![class];
    let mut implementors: HashMap<String, Vec<String>> = HashMap::new();
    implementors.insert("c1".to_string(), vec!["SubService".to_string()]);

    let batch = prepare_batch_for_embedding(
        &symbols,
        None,
        &HashMap::new(),
        &HashMap::new(),
        &implementors,
    );
    assert_eq!(batch.len(), 1);
    let (_, text) = &batch[0];
    assert!(
        !text.contains("implemented_by:"),
        "Classes should not get implementor enrichment: {text}"
    );
}

#[test]
fn test_prepare_batch_enriches_struct_with_field_signatures() {
    let struct_sym = make_symbol_with_lang("s1", "UserRecord", SymbolKind::Struct, "rust");

    let mut field1 = make_symbol_with_lang("f1", "name", SymbolKind::Field, "rust");
    field1.parent_id = Some("s1".to_string());
    field1.signature = Some("pub name: String".to_string());

    let mut field2 = make_symbol_with_lang("f2", "age", SymbolKind::Field, "rust");
    field2.parent_id = Some("s1".to_string());
    field2.signature = Some("pub age: u32".to_string());

    let mut field3 = make_symbol_with_lang("f3", "active", SymbolKind::Field, "rust");
    field3.parent_id = Some("s1".to_string());
    field3.signature = None;

    let symbols = vec![struct_sym, field1, field2, field3];
    let batch = prepare_batch_for_embedding(
        &symbols,
        None,
        &HashMap::new(),
        &HashMap::new(),
        &HashMap::new(),
    );

    // Only the struct is embeddable (Field is not in EMBEDDABLE_KINDS)
    assert_eq!(batch.len(), 1);

    let (_, text) = batch.iter().find(|(id, _)| id == "s1").unwrap();
    assert!(
        text.contains("fields: pub name: String, pub age: u32, active"),
        "Expected field signatures in embedding, got: {text}"
    );
}
