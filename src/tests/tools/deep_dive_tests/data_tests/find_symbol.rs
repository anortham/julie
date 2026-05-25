use super::*;

// === find_symbol tests ===

#[test]
fn test_find_symbol_by_name() {
    let (_tmp, mut db) = setup_db();

    let symbols = vec![make_symbol(
        "sym-1",
        "process",
        SymbolKind::Function,
        "src/engine.rs",
        10,
        None,
        Some("pub fn process()"),
        Some(Visibility::Public),
        None,
    )];
    db.store_symbols(&symbols).unwrap();

    let found = find_symbol(&db, "process", None).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name, "process");
    assert_eq!(found[0].file_path, "src/engine.rs");
}

#[test]
fn test_find_symbol_filters_imports() {
    let (_tmp, mut db) = setup_db();

    let symbols = vec![
        make_symbol(
            "sym-def",
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
            "sym-import",
            "process",
            SymbolKind::Import,
            "src/main.rs",
            1,
            None,
            None,
            None,
            None,
        ),
    ];
    db.store_symbols(&symbols).unwrap();

    let found = find_symbol(&db, "process", None).unwrap();
    assert_eq!(found.len(), 1, "imports should be filtered out");
    assert_eq!(found[0].kind, SymbolKind::Function);
}

#[test]
fn test_find_symbol_disambiguates_by_file() {
    let (_tmp, mut db) = setup_db();

    let symbols = vec![
        make_symbol(
            "sym-1",
            "handle",
            SymbolKind::Function,
            "src/engine.rs",
            10,
            None,
            None,
            None,
            None,
        ),
        make_symbol(
            "sym-2",
            "handle",
            SymbolKind::Function,
            "src/handler.rs",
            20,
            None,
            None,
            None,
            None,
        ),
    ];
    db.store_symbols(&symbols).unwrap();

    let found = find_symbol(&db, "handle", Some("handler")).unwrap();
    assert_eq!(found.len(), 1, "should disambiguate by file");
    assert_eq!(found[0].file_path, "src/handler.rs");
}

#[test]
fn test_find_symbol_not_found() {
    let (_tmp, db) = setup_db();

    let found = find_symbol(&db, "nonexistent", None).unwrap();
    assert!(found.is_empty());
}

#[test]
fn test_find_symbol_qualified_name() {
    let (_tmp, mut db) = setup_db();

    let symbols = vec![
        make_symbol(
            "sym-parent-a",
            "Engine",
            SymbolKind::Struct,
            "src/engine.rs",
            1,
            None,
            Some("pub struct Engine"),
            Some(Visibility::Public),
            None,
        ),
        make_symbol(
            "sym-method-a",
            "process",
            SymbolKind::Method,
            "src/engine.rs",
            10,
            Some("sym-parent-a"),
            Some("pub fn process(&self)"),
            Some(Visibility::Public),
            None,
        ),
        make_symbol(
            "sym-parent-b",
            "Pipeline",
            SymbolKind::Struct,
            "src/engine.rs",
            50,
            None,
            Some("pub struct Pipeline"),
            Some(Visibility::Public),
            None,
        ),
        make_symbol(
            "sym-method-b",
            "process",
            SymbolKind::Method,
            "src/engine.rs",
            60,
            Some("sym-parent-b"),
            Some("pub fn process(&self)"),
            Some(Visibility::Public),
            None,
        ),
    ];
    db.store_symbols(&symbols).unwrap();

    // Qualified lookup should resolve to exactly one symbol
    let found = find_symbol(&db, "Engine::process", None).unwrap();
    assert_eq!(
        found.len(),
        1,
        "qualified name should resolve to exactly one symbol"
    );
    assert_eq!(found[0].file_path, "src/engine.rs");
    assert_eq!(found[0].parent_id, Some("sym-parent-a".to_string()));

    // Dot-separated also works (for Python, JS, etc.)
    let found_dot = find_symbol(&db, "Pipeline.process", None).unwrap();
    assert_eq!(found_dot.len(), 1);
    assert_eq!(found_dot[0].parent_id, Some("sym-parent-b".to_string()));

    // Unqualified still returns both
    let found_all = find_symbol(&db, "process", None).unwrap();
    assert_eq!(found_all.len(), 2, "unqualified should still return both");
}

#[test]
fn test_find_symbol_qualified_name_uses_impl_type_metadata() {
    let (_tmp, mut db) = setup_db();

    let mut metadata = HashMap::new();
    metadata.insert(
        "impl_type_name".to_string(),
        serde_json::Value::String("Worker".to_string()),
    );

    let mut method_symbol = make_symbol(
        "sym-run",
        "run",
        SymbolKind::Method,
        "src/engine.rs",
        5,
        None,
        Some("fn run(&self)"),
        Some(Visibility::Private),
        None,
    );
    method_symbol.metadata = Some(metadata);

    let symbols = vec![
        make_symbol(
            "sym-worker",
            "Worker",
            SymbolKind::Struct,
            "src/engine.rs",
            1,
            None,
            Some("pub struct Worker;"),
            Some(Visibility::Public),
            None,
        ),
        method_symbol,
    ];
    db.store_symbols(&symbols).unwrap();

    let found = find_symbol(&db, "Worker::run", None).unwrap();
    assert_eq!(
        found.len(),
        1,
        "qualified lookup should use impl_type_name metadata when parent_id is missing"
    );
    assert_eq!(found[0].id, "sym-run");
}
