use super::*;

// === build_symbol_context tests ===

#[test]
fn test_build_context_with_incoming_relationships() {
    let (_tmp, mut db) = setup_db();

    let symbols = vec![
        make_symbol(
            "sym-target",
            "process",
            SymbolKind::Function,
            "src/engine.rs",
            10,
            None,
            None,
            None,
            None,
        ),
        make_symbol(
            "sym-caller",
            "main",
            SymbolKind::Function,
            "src/main.rs",
            5,
            None,
            Some("fn main()"),
            None,
            None,
        ),
    ];
    db.store_symbols(&symbols).unwrap();

    let rels = vec![make_rel(
        "rel-1",
        "sym-caller",
        "sym-target",
        RelationshipKind::Calls,
        "src/main.rs",
        8,
    )];
    db.store_relationships(&rels).unwrap();

    let ctx = build_symbol_context(&db, &symbols[0], "overview", 10, 10).unwrap();

    assert_eq!(ctx.incoming.len(), 1);
    assert_eq!(ctx.incoming_total, 1);
    assert_eq!(ctx.incoming[0].file_path, "src/main.rs");
    assert_eq!(ctx.incoming[0].line_number, 8);
    // Overview depth: still enriched (name is always useful)
    assert!(
        ctx.incoming[0].symbol.is_some(),
        "overview should still enrich refs for symbol names"
    );
    assert_eq!(ctx.incoming[0].symbol.as_ref().unwrap().name, "main");
}

#[test]
fn test_build_context_enriches_at_context_depth() {
    let (_tmp, mut db) = setup_db();

    let symbols = vec![
        make_symbol(
            "sym-target",
            "process",
            SymbolKind::Function,
            "src/engine.rs",
            10,
            None,
            None,
            None,
            None,
        ),
        make_symbol(
            "sym-caller",
            "main",
            SymbolKind::Function,
            "src/main.rs",
            5,
            None,
            Some("fn main()"),
            None,
            Some("fn main() { process(); }"),
        ),
    ];
    db.store_symbols(&symbols).unwrap();

    let rels = vec![make_rel(
        "rel-1",
        "sym-caller",
        "sym-target",
        RelationshipKind::Calls,
        "src/main.rs",
        8,
    )];
    db.store_relationships(&rels).unwrap();

    let ctx = build_symbol_context(&db, &symbols[0], "context", 15, 15).unwrap();

    assert_eq!(ctx.incoming.len(), 1);
    // Context depth: should enrich with symbol data
    assert!(
        ctx.incoming[0].symbol.is_some(),
        "context depth should enrich refs"
    );
    let enriched = ctx.incoming[0].symbol.as_ref().unwrap();
    assert_eq!(enriched.name, "main");
    assert_eq!(enriched.signature.as_deref(), Some("fn main()"));
}

#[test]
fn test_build_context_with_outgoing_relationships() {
    let (_tmp, mut db) = setup_db();

    let symbols = vec![
        make_symbol(
            "sym-source",
            "process",
            SymbolKind::Function,
            "src/engine.rs",
            10,
            None,
            None,
            None,
            None,
        ),
        make_symbol(
            "sym-callee",
            "validate",
            SymbolKind::Function,
            "src/engine.rs",
            50,
            None,
            Some("fn validate()"),
            None,
            None,
        ),
    ];
    db.store_symbols(&symbols).unwrap();

    let rels = vec![make_rel(
        "rel-1",
        "sym-source",
        "sym-callee",
        RelationshipKind::Calls,
        "src/engine.rs",
        15,
    )];
    db.store_relationships(&rels).unwrap();

    let ctx = build_symbol_context(&db, &symbols[0], "overview", 10, 10).unwrap();

    assert_eq!(ctx.outgoing.len(), 1);
    assert_eq!(ctx.outgoing_total, 1);
    assert_eq!(ctx.outgoing[0].file_path, "src/engine.rs");
}

#[test]
fn test_build_context_with_children() {
    let (_tmp, mut db) = setup_db();

    let symbols = vec![
        make_symbol(
            "sym-parent",
            "UserService",
            SymbolKind::Class,
            "src/engine.rs",
            1,
            None,
            Some("pub struct UserService"),
            Some(Visibility::Public),
            None,
        ),
        make_symbol(
            "sym-field",
            "users",
            SymbolKind::Property,
            "src/engine.rs",
            3,
            Some("sym-parent"),
            Some("users: Vec<User>"),
            None,
            None,
        ),
        make_symbol(
            "sym-method",
            "get_user",
            SymbolKind::Method,
            "src/engine.rs",
            10,
            Some("sym-parent"),
            Some("pub fn get_user(&self) -> Option<&User>"),
            Some(Visibility::Public),
            None,
        ),
    ];
    db.store_symbols(&symbols).unwrap();

    let ctx = build_symbol_context(&db, &symbols[0], "overview", 10, 10).unwrap();

    assert_eq!(ctx.children.len(), 2, "should have 2 children");
    // Children ordered by start_line
    assert_eq!(ctx.children[0].name, "users");
    assert_eq!(ctx.children[1].name, "get_user");
}

#[test]
fn test_build_context_non_container_has_no_children() {
    let (_tmp, mut db) = setup_db();

    let symbols = vec![make_symbol(
        "sym-func",
        "process",
        SymbolKind::Function,
        "src/engine.rs",
        10,
        None,
        None,
        None,
        None,
    )];
    db.store_symbols(&symbols).unwrap();

    let ctx = build_symbol_context(&db, &symbols[0], "overview", 10, 10).unwrap();

    assert!(
        ctx.children.is_empty(),
        "functions should not query for children"
    );
}
