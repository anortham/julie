use super::*;

// ========================================
// CASCADE ARCHITECTURE: Phase 1 TDD Tests
// ========================================

#[test]
fn test_store_file_with_content() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    #[allow(unused_mut)]
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    db.store_file_with_content(
        "test.md",
        "markdown",
        "abc123",
        1024,
        1234567890,
        "# Test\nThis is test content",
        "test_workspace",
    )
    .unwrap();

    let content = db.get_file_content("test.md").unwrap();
    assert_eq!(content, Some("# Test\nThis is test content".to_string()));
}

#[test]
fn test_get_file_contents_by_paths_returns_requested_content_map() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    #[allow(unused_mut)]
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    db.store_file_with_content(
        "src/a.rs",
        "rust",
        "a",
        10,
        1,
        "alpha beta",
        "test_workspace",
    )
    .unwrap();
    db.store_file_with_content(
        "src/b.rs",
        "rust",
        "b",
        10,
        1,
        "beta gamma",
        "test_workspace",
    )
    .unwrap();

    let paths = vec![
        "src/a.rs".to_string(),
        "src/missing.rs".to_string(),
        "src/b.rs".to_string(),
    ];
    let contents = db.get_file_contents_by_paths(&paths).unwrap();

    assert_eq!(contents.len(), 2);
    assert_eq!(
        contents.get("src/a.rs"),
        Some(&Some("alpha beta".to_string()))
    );
    assert_eq!(
        contents.get("src/b.rs"),
        Some(&Some("beta gamma".to_string()))
    );
    assert!(!contents.contains_key("src/missing.rs"));
}

#[test]
fn test_get_file_languages_by_paths_returns_requested_languages() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    #[allow(unused_mut)]
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    db.store_file_with_content(
        "include/widget.h",
        "cpp",
        "h",
        10,
        1,
        "class Widget {};",
        "test_workspace",
    )
    .unwrap();
    db.store_file_with_content(
        "src/legacy.c",
        "c",
        "c",
        10,
        1,
        "void tick(void) {}",
        "test_workspace",
    )
    .unwrap();

    let languages = db
        .get_file_languages_by_paths(&["include/widget.h", "missing.rs", "src/legacy.c"])
        .unwrap();

    assert_eq!(languages.len(), 2);
    assert_eq!(
        languages.get("include/widget.h").map(String::as_str),
        Some("cpp")
    );
    assert_eq!(languages.get("src/legacy.c").map(String::as_str), Some("c"));
    assert!(!languages.contains_key("missing.rs"));
}

#[test]
fn test_get_identifiers_by_names_kinds_excluding_containers_filters_rows() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    #[allow(unused_mut)]
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    for (path, content) in [
        ("src/main.rs", "fn target() {}"),
        ("src/included.rs", "fn included() { target(); }"),
        ("tests/excluded.rs", "fn excluded() { target(); }"),
    ] {
        db.store_file_with_content(path, "rust", path, content.len() as u64, 1, content, "ws")
            .unwrap();
    }

    let symbols = [
        ("sym_target", "target", "src/main.rs"),
        ("sym_included", "included", "src/included.rs"),
        ("sym_excluded", "excluded", "tests/excluded.rs"),
    ]
    .into_iter()
    .map(|(id, name, path)| {
        symbol_builder(id, name, path)
            .kind(SymbolKind::Function)
            .span(1, 0, 1, 1)
            .bytes(0, 10)
            .build()
    })
    .collect::<Vec<_>>();
    db.store_symbols(&symbols).unwrap();

    let identifiers = [
        ("ident_call", IdentifierKind::Call, "sym_included"),
        ("ident_type", IdentifierKind::TypeUsage, "sym_included"),
        ("ident_member", IdentifierKind::MemberAccess, "sym_included"),
        ("ident_excluded", IdentifierKind::Call, "sym_excluded"),
    ]
    .into_iter()
    .map(|(id, kind, container_id)| {
        identifier_builder(id, "target", "src/included.rs")
            .kind(kind)
            .line(1)
            .column(0, 6)
            .bytes(0, 6)
            .containing_symbol_id(container_id)
            .target_symbol_id("sym_target")
            .build()
    })
    .collect::<Vec<_>>();
    db.bulk_store_identifiers(&identifiers, "").unwrap();

    let excluded = std::collections::HashSet::from(["sym_excluded".to_string()]);
    let identifiers = db
        .get_identifiers_by_names_kinds_excluding_containers(
            &["target".to_string()],
            &["call", "type_usage", "import"],
            &excluded,
        )
        .unwrap();

    let kinds: std::collections::HashSet<&str> = identifiers
        .iter()
        .map(|identifier| identifier.kind.as_str())
        .collect();
    let containers: std::collections::HashSet<&str> = identifiers
        .iter()
        .filter_map(|identifier| identifier.containing_symbol_id.as_deref())
        .collect();

    assert_eq!(identifiers.len(), 2);
    assert_eq!(
        kinds,
        std::collections::HashSet::from(["call", "type_usage"])
    );
    assert_eq!(
        containers,
        std::collections::HashSet::from(["sym_included"])
    );
}
