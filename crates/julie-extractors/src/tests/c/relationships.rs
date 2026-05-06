use super::parse_c;
use crate::base::{RelationshipKind, SymbolKind};

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

#[test]
fn test_c_declarations_emit_type_use_relationships() {
    let code = r#"
typedef struct widget {
    int id;
} widget_t;

struct service {
    widget_t current;
};

void service_init(widget_t *widget) {
    (void)widget;
}
"#;

    let (mut extractor, tree) = parse_c(code, "relations.c");
    let symbols = extractor.extract_symbols(&tree);
    let relationships = extractor.extract_relationships(&tree, &symbols);

    let widget_type = symbols
        .iter()
        .find(|symbol| symbol.name == "widget_t" && symbol.kind == SymbolKind::Struct)
        .expect("widget_t typedef struct should be extracted");
    let service = symbols
        .iter()
        .find(|symbol| symbol.name == "service" && symbol.kind == SymbolKind::Struct)
        .expect("service struct should be extracted");
    let service_init = symbols
        .iter()
        .find(|symbol| symbol.name == "service_init" && symbol.kind == SymbolKind::Function)
        .expect("service_init function should be extracted");

    assert!(
        relationships.iter().any(|relationship| {
            relationship.kind == RelationshipKind::Uses
                && relationship.from_symbol_id == service.id
                && relationship.to_symbol_id == widget_type.id
        }),
        "service should use widget_t through its field declaration. Relationships: {:?}",
        relationships
    );
    assert!(
        relationships.iter().any(|relationship| {
            relationship.kind == RelationshipKind::Uses
                && relationship.from_symbol_id == service_init.id
                && relationship.to_symbol_id == widget_type.id
        }),
        "service_init should use widget_t through its parameter type. Relationships: {:?}",
        relationships
    );
}

#[test]
fn test_c_indirect_call_emits_low_confidence_pending_relationship() {
    let code = r#"
typedef int (*callback_t)(int);

int run_callback(callback_t callback, int value) {
    return (*callback)(value);
}
"#;

    let (mut extractor, tree) = parse_c(code, "relations.c");
    let symbols = extractor.extract_symbols(&tree);
    let relationships = extractor.extract_relationships(&tree, &symbols);
    let structured_pending = extractor.get_structured_pending_relationships();

    assert!(
        relationships
            .iter()
            .all(|relationship| relationship.kind != RelationshipKind::Calls),
        "indirect callback should not become a resolved direct call. Relationships: {:?}",
        relationships
    );
    assert!(
        structured_pending.iter().any(|pending| {
            pending.pending.kind == RelationshipKind::Calls
                && pending.target.terminal_name == "callback"
                && pending.pending.confidence < 0.7
        }),
        "indirect callback should emit a low-confidence pending call. Pending: {:?}",
        structured_pending
    );
}
