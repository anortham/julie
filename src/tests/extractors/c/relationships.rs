use crate::extractors::base::RelationshipKind;
use super::parse_c;

#[test]
fn test_extract_function_call_relationships() {
    let code = r#"
int callee(void) {
    return 42;
}

int caller(int value) {
    return callee() + value;
}
"#;

    let (mut extractor, tree) = parse_c(code, "relations.c");
    let symbols = extractor.extract_symbols(&tree);
    let relationships = extractor.extract_relationships(&tree, &symbols);

    let caller = symbols
        .iter()
        .find(|s| s.name == "caller")
        .expect("caller symbol not found");
    let callee = symbols
        .iter()
        .find(|s| s.name == "callee")
        .expect("callee symbol not found");

    assert!(
        relationships.iter().any(|rel| {
            rel.kind == RelationshipKind::Calls
                && rel.from_symbol_id == caller.id
                && rel.to_symbol_id == callee.id
        }),
        "Expected caller -> callee Calls relationship, got: {:?}",
        relationships
    );
}
