use super::*;

// =========================================================================
// Tests: reference_kind filter
// =========================================================================

#[test]
fn test_target_workspace_reference_kind_filters_relationships() {
    let files = &["src/lib.rs", "src/caller.rs", "src/importer.rs"];
    let (_tmp, mut db) = setup_db(files);

    let target = make_symbol("sym-widget", "Widget", "src/lib.rs", 1);
    db.store_symbols(&[target]).unwrap();

    // Store caller symbols for FK constraints
    store_caller_symbol(&mut db, "caller_fn", "src/caller.rs", 15);
    store_caller_symbol(&mut db, "importer", "src/importer.rs", 3);

    // Store a "call" relationship
    let call_rel = make_relationship(
        "rel_call",
        "caller_fn",
        "sym-widget",
        "src/caller.rs",
        15,
        RelationshipKind::Calls,
        0.9,
    );
    // Store an "import" relationship
    let import_rel = make_relationship(
        "rel_import",
        "importer",
        "sym-widget",
        "src/importer.rs",
        3,
        RelationshipKind::Imports,
        1.0,
    );
    db.store_relationships(&[call_rel, import_rel]).unwrap();

    // Insert corresponding identifiers for the kind filtering to work
    // (get_relationships_to_symbols_filtered_by_kind joins with identifiers table)
    insert_identifier_with_target(
        &db,
        "Widget",
        "call",
        "src/caller.rs",
        15,
        Some("caller_fn"),
        Some("sym-widget"),
        0.9,
    );
    insert_identifier_with_target(
        &db,
        "Widget",
        "import",
        "src/importer.rs",
        3,
        Some("importer"),
        Some("sym-widget"),
        1.0,
    );

    // Filter by "call" -- should only get the call reference
    let (_, refs_call) = find_refs_in_db(&db, "Widget", 100, Some("call"));
    assert!(
        refs_call
            .iter()
            .all(|r| r.file_path != "src/importer.rs" || r.id.starts_with("ident_")),
        "call filter should not return unfiltered import relationships"
    );
    // Should include the call ref
    assert!(
        refs_call.iter().any(|r| r.file_path == "src/caller.rs"),
        "call filter should include src/caller.rs call ref"
    );
}

#[test]
fn test_target_workspace_reference_kind_filters_identifiers() {
    let files = &["src/lib.rs", "src/user.rs", "src/types.rs"];
    let (_tmp, mut db) = setup_db(files);

    let target = make_symbol("sym-config", "Config", "src/lib.rs", 1);
    db.store_symbols(&[target]).unwrap();

    // No relationships -- only identifiers of different kinds
    insert_identifier(&db, "Config", "call", "src/user.rs", 20, None, 0.8);
    insert_identifier(&db, "Config", "type_usage", "src/types.rs", 5, None, 0.7);

    // Filter by "call" -- should only get the call identifier
    let (_, refs_call) = find_refs_in_db(&db, "Config", 100, Some("call"));
    assert_eq!(
        refs_call.len(),
        1,
        "should find exactly 1 call identifier, got {}",
        refs_call.len()
    );
    assert_eq!(refs_call[0].file_path, "src/user.rs");

    // Filter by "type_usage" -- should only get the type_usage identifier
    let (_, refs_type) = find_refs_in_db(&db, "Config", 100, Some("type_usage"));
    assert_eq!(
        refs_type.len(),
        1,
        "should find exactly 1 type_usage identifier"
    );
    assert_eq!(refs_type[0].file_path, "src/types.rs");

    // No filter -- should get both
    let (_, refs_all) = find_refs_in_db(&db, "Config", 100, None);
    assert_eq!(
        refs_all.len(),
        2,
        "without filter, should find 2 identifier refs"
    );
}

#[test]
fn test_relationship_kind_filter_uses_matching_identifier_target() {
    let files = &[
        "src/lib.rs",
        "src/caller.rs",
        "src/other.rs",
        "src/shared.rs",
    ];
    let (_tmp, mut db) = setup_db(files);

    let widget = make_symbol("sym-widget", "Widget", "src/lib.rs", 1);
    let gadget = make_symbol("sym-gadget", "Gadget", "src/lib.rs", 2);
    db.store_symbols(&[widget, gadget]).unwrap();

    store_caller_symbol(&mut db, "caller_fn", "src/caller.rs", 12);
    store_caller_symbol(&mut db, "other_fn", "src/other.rs", 12);

    let call_rel = make_relationship(
        "rel_call",
        "caller_fn",
        "sym-widget",
        "src/shared.rs",
        8,
        RelationshipKind::Calls,
        0.9,
    );
    let unrelated_rel = make_relationship(
        "rel_type",
        "other_fn",
        "sym-gadget",
        "src/shared.rs",
        8,
        RelationshipKind::References,
        0.8,
    );
    db.store_relationships(&[call_rel, unrelated_rel]).unwrap();

    insert_identifier_with_target(
        &db,
        "Widget",
        "call",
        "src/shared.rs",
        8,
        Some("caller_fn"),
        Some("sym-widget"),
        0.9,
    );
    insert_identifier_with_target(
        &db,
        "Gadget",
        "type_usage",
        "src/shared.rs",
        8,
        Some("other_fn"),
        Some("sym-gadget"),
        0.8,
    );

    let refs = db
        .get_relationships_to_symbols_filtered_by_kind(
            &["sym-widget".to_string(), "sym-gadget".to_string()],
            "call",
        )
        .unwrap();

    assert_eq!(
        refs.len(),
        1,
        "kind filter should only return the call relationship, got {:?}",
        refs.iter().map(|r| r.id.as_str()).collect::<Vec<_>>()
    );
    assert_eq!(refs[0].id, "rel_call");
    assert_eq!(refs[0].to_symbol_id, "sym-widget");
}

#[test]
fn test_relationship_kind_filter_scopes_to_identifier_occurrence() {
    let files = &["src/lib.rs", "src/call_site.rs", "src/type_site.rs"];
    let (_tmp, mut db) = setup_db(files);

    let widget = make_symbol("sym-widget", "Widget", "src/lib.rs", 1);
    db.store_symbols(&[widget]).unwrap();

    store_caller_symbol(&mut db, "call_site", "src/call_site.rs", 12);
    store_caller_symbol(&mut db, "type_site", "src/type_site.rs", 27);

    let call_rel = make_relationship(
        "rel_call",
        "call_site",
        "sym-widget",
        "src/call_site.rs",
        12,
        RelationshipKind::Calls,
        0.95,
    );
    let type_rel = make_relationship(
        "rel_type",
        "type_site",
        "sym-widget",
        "src/type_site.rs",
        27,
        RelationshipKind::References,
        0.75,
    );
    db.store_relationships(&[call_rel, type_rel]).unwrap();

    insert_identifier_with_target(
        &db,
        "Widget",
        "call",
        "src/call_site.rs",
        12,
        Some("call_site"),
        Some("sym-widget"),
        0.95,
    );
    insert_identifier_with_target(
        &db,
        "Widget",
        "type_usage",
        "src/type_site.rs",
        27,
        Some("type_site"),
        Some("sym-widget"),
        0.75,
    );

    let refs = db
        .get_relationships_to_symbols_filtered_by_kind(&["sym-widget".to_string()], "call")
        .unwrap();

    assert_eq!(
        refs.len(),
        1,
        "kind filter should only return the call occurrence, got {:?}",
        refs.iter().map(|r| r.id.as_str()).collect::<Vec<_>>()
    );
    assert_eq!(refs[0].id, "rel_call");
    assert_eq!(refs[0].file_path, "src/call_site.rs");
    assert_eq!(refs[0].line_number, 12);
}
