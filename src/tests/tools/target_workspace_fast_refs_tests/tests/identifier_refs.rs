use super::*;

// =========================================================================
// Tests: identifier-based reference discovery (Strategy 4)
// =========================================================================

#[test]
fn test_target_workspace_includes_identifier_refs() {
    let files = &["src/lib.rs", "src/main.rs", "src/handler.rs"];
    let (_tmp, mut db) = setup_db(files);

    let target = make_symbol("sym-process", "process", "src/lib.rs", 10);
    db.store_symbols(&[target]).unwrap();

    // No relationships -- only identifiers
    insert_identifier(&db, "process", "call", "src/main.rs", 25, None, 0.9);
    insert_identifier(&db, "process", "call", "src/handler.rs", 42, None, 0.85);

    let (defs, refs) = find_refs_in_db(&db, "process", 100, None);
    assert_eq!(defs.len(), 1);
    assert_eq!(
        refs.len(),
        2,
        "should find 2 identifier-based refs, got {}",
        refs.len()
    );

    // Verify they're converted to Relationship correctly
    let ref_files: HashSet<&str> = refs.iter().map(|r| r.file_path.as_str()).collect();
    assert!(ref_files.contains("src/main.rs"));
    assert!(ref_files.contains("src/handler.rs"));
}

#[test]
fn test_target_workspace_identifier_dedup_against_relationships() {
    let files = &["src/lib.rs", "src/caller.rs"];
    let (_tmp, mut db) = setup_db(files);

    let target = make_symbol("sym-foo", "foo", "src/lib.rs", 1);
    db.store_symbols(&[target]).unwrap();

    // Store caller symbol for FK constraint
    store_caller_symbol(&mut db, "some_caller", "src/caller.rs", 15);

    // Store a relationship at src/caller.rs:15
    let rel = make_relationship(
        "rel_1",
        "some_caller",
        "sym-foo",
        "src/caller.rs",
        15,
        RelationshipKind::Calls,
        0.9,
    );
    db.store_relationships(&[rel]).unwrap();

    // Also insert an identifier at the SAME location (src/caller.rs:15)
    insert_identifier(
        &db,
        "foo",
        "call",
        "src/caller.rs",
        15,
        Some("some_caller"),
        0.9,
    );

    let (_, refs) = find_refs_in_db(&db, "foo", 100, None);
    assert_eq!(
        refs.len(),
        1,
        "should deduplicate: only 1 ref for same file:line, got {}",
        refs.len()
    );
}

#[test]
fn test_target_workspace_identifier_dedup_against_definitions() {
    let files = &["src/lib.rs", "src/other.rs"];
    let (_tmp, mut db) = setup_db(files);

    let target = make_symbol("sym-bar", "bar", "src/lib.rs", 10);
    db.store_symbols(&[target]).unwrap();

    // Insert an identifier at the definition site (same file:line as definition)
    insert_identifier(&db, "bar", "call", "src/lib.rs", 10, None, 1.0);
    // Insert an identifier at a different location
    insert_identifier(&db, "bar", "call", "src/other.rs", 30, None, 0.8);

    let (defs, refs) = find_refs_in_db(&db, "bar", 100, None);
    assert_eq!(defs.len(), 1);
    assert_eq!(
        refs.len(),
        1,
        "should not include identifier at definition site, got {} refs",
        refs.len()
    );
    assert_eq!(refs[0].file_path, "src/other.rs");
}

#[test]
fn test_target_workspace_identifier_kind_conversion() {
    let files = &["src/lib.rs", "src/a.rs", "src/b.rs", "src/c.rs"];
    let (_tmp, mut db) = setup_db(files);

    let target = make_symbol("sym-thing", "Thing", "src/lib.rs", 1);
    db.store_symbols(&[target]).unwrap();

    insert_identifier(&db, "Thing", "call", "src/a.rs", 10, None, 0.9);
    insert_identifier(&db, "Thing", "import", "src/b.rs", 2, None, 1.0);
    insert_identifier(&db, "Thing", "type_usage", "src/c.rs", 15, None, 0.7);

    let (_, refs) = find_refs_in_db(&db, "Thing", 100, None);
    assert_eq!(refs.len(), 3, "should find 3 identifier refs");

    // Check kind conversion
    let call_ref = refs.iter().find(|r| r.file_path == "src/a.rs").unwrap();
    assert_eq!(call_ref.kind, RelationshipKind::Calls);

    let import_ref = refs.iter().find(|r| r.file_path == "src/b.rs").unwrap();
    assert_eq!(import_ref.kind, RelationshipKind::Imports);

    let type_ref = refs.iter().find(|r| r.file_path == "src/c.rs").unwrap();
    assert_eq!(type_ref.kind, RelationshipKind::Uses);
}

// =========================================================================
// Tests: combined limit + reference_kind + identifiers
// =========================================================================

#[test]
fn test_target_workspace_combined_limit_and_kind_filter() {
    let files = &["src/lib.rs", "src/a.rs", "src/b.rs", "src/c.rs", "src/d.rs"];
    let (_tmp, mut db) = setup_db(files);

    let target = make_symbol("sym-handler", "handler", "src/lib.rs", 1);
    db.store_symbols(&[target]).unwrap();

    // Insert 3 "call" identifiers and 1 "type_usage" identifier
    insert_identifier(&db, "handler", "call", "src/a.rs", 10, None, 0.9);
    insert_identifier(&db, "handler", "call", "src/b.rs", 20, None, 0.8);
    insert_identifier(&db, "handler", "call", "src/c.rs", 30, None, 0.7);
    insert_identifier(&db, "handler", "type_usage", "src/d.rs", 5, None, 0.95);

    // Filter by "call" with limit=2 -- should get top 2 call refs by confidence
    let (_, refs) = find_refs_in_db(&db, "handler", 2, Some("call"));
    assert_eq!(refs.len(), 2, "should get exactly 2 call refs with limit=2");
    // All should be calls (no type_usage)
    for r in &refs {
        assert_eq!(
            r.kind,
            RelationshipKind::Calls,
            "all refs should be Calls kind"
        );
    }
    // Should be sorted by confidence descending
    assert!(refs[0].confidence >= refs[1].confidence);
}
