use super::*;

// ---------------------------------------------------------------------------
// Type arguments (Miller bridge Phase 2): re-index must clean stale rows
// ---------------------------------------------------------------------------
#[test]
fn test_incremental_update_atomic_cleans_and_replaces_type_arguments() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    let files_v1 = vec![make_file("file_a.rs")];
    let idents_v1 = vec![make_identifier("id_dict", "Dictionary", "file_a.rs")];
    let rows_v1 = type_argument_rows(
        "id_dict",
        "file_a.rs",
        vec![
            leaf_arg(0, "string"),
            TypeArgument {
                ordinal: 1,
                type_name: "List".to_string(),
                children: vec![leaf_arg(0, "int")],
            },
        ],
    );
    let write_set_v1 = CanonicalWriteSet {
        files: &files_v1,
        symbols: &[],
        relationships: &[],
        identifiers: &idents_v1,
        types: &[],
        type_arguments: &rows_v1,
        literals: &[],
    };
    db.incremental_update_atomic_with_metadata(
        &[],
        &write_set_v1,
        "ws_test",
        AtomicPersistenceMetadata::default(),
    )
    .expect("round 1 type-argument write should succeed");

    assert_eq!(
        count_rows(&db, "type_arguments"),
        3,
        "Dictionary<string, List<int>> flattens to 3 type-argument rows"
    );

    let list_id: String = db
        .conn
        .query_row(
            "SELECT id FROM type_arguments WHERE type_name = 'List'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let int_parent: Option<String> = db
        .conn
        .query_row(
            "SELECT parent_arg_id FROM type_arguments WHERE type_name = 'int'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        int_parent.as_deref(),
        Some(list_id.as_str()),
        "nested int row must point at its List parent"
    );
    assert_eq!(
        count_rows_where(&db, "type_arguments", "identifier_id = 'id_dict'"),
        3,
        "every round-1 row belongs to the id_dict use site"
    );

    let files_v2 = vec![make_file("file_a.rs")];
    let idents_v2 = vec![make_identifier("id_list", "List", "file_a.rs")];
    let rows_v2 = type_argument_rows("id_list", "file_a.rs", vec![leaf_arg(0, "int")]);
    let write_set_v2 = CanonicalWriteSet {
        files: &files_v2,
        symbols: &[],
        relationships: &[],
        identifiers: &idents_v2,
        types: &[],
        type_arguments: &rows_v2,
        literals: &[],
    };
    db.incremental_update_atomic_with_metadata(
        &["file_a.rs".to_string()],
        &write_set_v2,
        "ws_test",
        AtomicPersistenceMetadata::default(),
    )
    .expect("round 2 type-argument write should succeed");

    assert_eq!(
        count_rows(&db, "type_arguments"),
        1,
        "re-index must clean the 3 old rows and leave only the 1 new row"
    );
    assert_eq!(
        count_rows_where(&db, "type_arguments", "identifier_id = 'id_dict'"),
        0,
        "stale rows from the previous extraction must be gone"
    );
    let surviving: String = db
        .conn
        .query_row("SELECT identifier_id FROM type_arguments", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(
        surviving, "id_list",
        "the only surviving row must belong to the new use site"
    );
}

#[test]
fn test_replace_workspace_data_atomic_clears_stale_type_arguments() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    let files_v1 = vec![make_file("a.rs"), make_file("b.rs")];
    let idents_v1 = vec![
        make_identifier("id_a", "List", "a.rs"),
        make_identifier("id_b", "List", "b.rs"),
    ];
    let mut rows_v1 = type_argument_rows("id_a", "a.rs", vec![leaf_arg(0, "int")]);
    rows_v1.extend(type_argument_rows(
        "id_b",
        "b.rs",
        vec![leaf_arg(0, "string")],
    ));
    let write_set_v1 = CanonicalWriteSet {
        files: &files_v1,
        symbols: &[],
        relationships: &[],
        identifiers: &idents_v1,
        types: &[],
        type_arguments: &rows_v1,
        literals: &[],
    };
    db.incremental_update_atomic_with_metadata(
        &[],
        &write_set_v1,
        "ws_test",
        AtomicPersistenceMetadata::default(),
    )
    .expect("seed write should succeed");
    assert_eq!(
        count_rows(&db, "type_arguments"),
        2,
        "precondition: two seeded type-argument rows across two files"
    );

    let files_v2 = vec![make_file("a.rs")];
    let idents_v2 = vec![make_identifier("id_a2", "Span", "a.rs")];
    let rows_v2 = type_argument_rows("id_a2", "a.rs", vec![leaf_arg(0, "byte")]);
    let write_set_v2 = CanonicalWriteSet {
        files: &files_v2,
        symbols: &[],
        relationships: &[],
        identifiers: &idents_v2,
        types: &[],
        type_arguments: &rows_v2,
        literals: &[],
    };
    db.replace_workspace_data_atomic(
        &write_set_v2,
        "ws_test",
        AtomicPersistenceMetadata::default(),
    )
    .expect("full-replace rebuild should succeed");

    assert_eq!(
        count_rows(&db, "type_arguments"),
        1,
        "full-replace rebuild must clear every prior type-argument row"
    );
    let surviving: String = db
        .conn
        .query_row("SELECT type_name FROM type_arguments", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        surviving, "byte",
        "only the rebuilt batch's row may survive a full-replace rebuild"
    );
}

#[test]
fn test_delete_workspace_data_clears_all_owned_tables() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    let files = vec![make_file("src/lib.rs")];
    let symbols = vec![
        make_symbol("sym_a", "do_stuff", "src/lib.rs"),
        make_symbol("sym_b", "helper", "src/lib.rs"),
    ];
    let relationships = vec![make_relationship("rel_1", "sym_a", "sym_b", "src/lib.rs")];
    let identifiers = vec![make_identifier("ident_1", "helper", "src/lib.rs")];
    let types = vec![make_type_info("sym_a", "Result<(), Error>")];

    db.incremental_update_atomic(
        &[],
        &files,
        &symbols,
        &relationships,
        &identifiers,
        &types,
        "ws_test",
    )
    .expect("incremental_update_atomic should succeed");

    db.store_embeddings(&[("sym_a".to_string(), vec![0.1f32; 384])])
        .expect("store_embeddings should succeed");

    db.record_indexing_repair("src/lib.rs", "tantivy_dirty", Some("test"))
        .expect("record_indexing_repair should succeed");

    let ta_idents = vec![make_identifier("id_ta", "List", "src/lib.rs")];
    let ta_rows = type_argument_rows("id_ta", "src/lib.rs", vec![leaf_arg(0, "int")]);
    let ta_write_set = CanonicalWriteSet {
        files: &[],
        symbols: &[],
        relationships: &[],
        identifiers: &ta_idents,
        types: &[],
        type_arguments: &ta_rows,
        literals: &[],
    };
    db.incremental_update_atomic_with_metadata(
        &[],
        &ta_write_set,
        "ws_test",
        AtomicPersistenceMetadata::default(),
    )
    .expect("seeding type_arguments should succeed");

    let lit_rows = vec![make_literal(
        "lit_ws",
        "/api/health",
        LiteralKind::Url,
        "fetch",
        "src/lib.rs",
    )];
    let lit_write_set = CanonicalWriteSet {
        files: &[],
        symbols: &[],
        relationships: &[],
        identifiers: &[],
        types: &[],
        type_arguments: &[],
        literals: &lit_rows,
    };
    db.incremental_update_atomic_with_metadata(
        &[],
        &lit_write_set,
        "ws_test",
        AtomicPersistenceMetadata::default(),
    )
    .expect("seeding literals should succeed");

    assert!(count_rows(&db, "symbols") > 0, "precondition: symbols");
    assert!(count_rows(&db, "files") > 0, "precondition: files");
    assert!(
        count_rows(&db, "identifiers") > 0,
        "precondition: identifiers"
    );
    assert!(count_rows(&db, "types") > 0, "precondition: types");
    assert!(
        count_rows(&db, "symbol_vectors") > 0,
        "precondition: symbol_vectors"
    );
    assert!(
        count_rows(&db, "indexing_repairs") > 0,
        "precondition: indexing_repairs"
    );
    assert!(
        count_rows(&db, "canonical_revisions") > 0,
        "precondition: canonical_revisions"
    );
    assert!(
        count_rows(&db, "type_arguments") > 0,
        "precondition: type_arguments"
    );
    assert!(count_rows(&db, "literals") > 0, "precondition: literals");

    db.delete_workspace_data()
        .expect("workspace cleanup should succeed");

    assert_eq!(count_rows(&db, "symbols"), 0, "symbols must be cleared");
    assert_eq!(count_rows(&db, "files"), 0, "files must be cleared");
    assert_eq!(
        count_rows(&db, "relationships"),
        0,
        "relationships must be cleared"
    );
    assert_eq!(
        count_rows(&db, "identifiers"),
        0,
        "identifiers must be cleared"
    );
    assert_eq!(count_rows(&db, "types"), 0, "types must be cleared");
    assert_eq!(
        count_rows(&db, "symbol_vectors"),
        0,
        "symbol_vectors must be cleared"
    );
    assert_eq!(
        count_rows(&db, "indexing_repairs"),
        0,
        "indexing_repairs must be cleared"
    );
    assert_eq!(
        count_rows(&db, "canonical_revisions"),
        0,
        "canonical_revisions must be cleared"
    );
    assert_eq!(
        count_rows(&db, "projection_states"),
        0,
        "projection_states must be cleared"
    );
    assert_eq!(
        count_rows(&db, "type_arguments"),
        0,
        "type_arguments must be cleared"
    );
    assert_eq!(count_rows(&db, "literals"), 0, "literals must be cleared");
}

#[test]
fn test_literals_roundtrip_persists_all_columns() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    let files = vec![make_file("api.ts")];
    let literals = vec![make_literal(
        "lit_url",
        "/api/users/{}",
        LiteralKind::Url,
        "fetch",
        "api.ts",
    )];
    let write_set = CanonicalWriteSet {
        files: &files,
        symbols: &[],
        relationships: &[],
        identifiers: &[],
        types: &[],
        type_arguments: &[],
        literals: &literals,
    };
    db.incremental_update_atomic_with_metadata(
        &[],
        &write_set,
        "ws_test",
        AtomicPersistenceMetadata::default(),
    )
    .expect("literal write should succeed");

    let (text, kind, carrier, arg_position, language, file_path): (
        String,
        String,
        Option<String>,
        i64,
        String,
        String,
    ) = db
        .conn
        .query_row(
            "SELECT literal_text, kind, carrier, arg_position, language, file_path \
             FROM literals WHERE id = 'lit_url'",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .expect("the literal row must be readable");
    assert_eq!(text, "/api/users/{}", "decoded text must roundtrip");
    assert_eq!(kind, "url", "kind must persist as its db string");
    assert_eq!(carrier.as_deref(), Some("fetch"), "carrier must persist");
    assert_eq!(arg_position, 0);
    assert_eq!(language, "rust");
    assert_eq!(file_path, "api.ts");
}

#[test]
fn test_incremental_update_atomic_cleans_and_replaces_literals() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    let files_v1 = vec![make_file("api.ts")];
    let literals_v1 = vec![
        make_literal("lit_a", "/api/users", LiteralKind::Url, "fetch", "api.ts"),
        make_literal(
            "lit_b",
            "/api/orders",
            LiteralKind::Url,
            "axios.get",
            "api.ts",
        ),
    ];
    let write_set_v1 = CanonicalWriteSet {
        files: &files_v1,
        symbols: &[],
        relationships: &[],
        identifiers: &[],
        types: &[],
        type_arguments: &[],
        literals: &literals_v1,
    };
    db.incremental_update_atomic_with_metadata(
        &[],
        &write_set_v1,
        "ws_test",
        AtomicPersistenceMetadata::default(),
    )
    .expect("round 1 literal write should succeed");
    assert_eq!(
        count_rows(&db, "literals"),
        2,
        "two round-1 literals stored"
    );

    let files_v2 = vec![make_file("api.ts")];
    let literals_v2 = vec![make_literal(
        "lit_c",
        "/api/products",
        LiteralKind::Url,
        "fetch",
        "api.ts",
    )];
    let write_set_v2 = CanonicalWriteSet {
        files: &files_v2,
        symbols: &[],
        relationships: &[],
        identifiers: &[],
        types: &[],
        type_arguments: &[],
        literals: &literals_v2,
    };
    db.incremental_update_atomic_with_metadata(
        &["api.ts".to_string()],
        &write_set_v2,
        "ws_test",
        AtomicPersistenceMetadata::default(),
    )
    .expect("round 2 literal write should succeed");

    assert_eq!(
        count_rows(&db, "literals"),
        1,
        "re-index must clean the 2 stale literals and leave only the 1 new row"
    );
    let surviving: String = db
        .conn
        .query_row("SELECT literal_text FROM literals", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        surviving, "/api/products",
        "only the re-indexed literal may survive"
    );
}

#[test]
fn test_replace_workspace_data_atomic_clears_stale_literals() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    let files_v1 = vec![make_file("a.ts"), make_file("b.ts")];
    let literals_v1 = vec![
        make_literal("la", "/a", LiteralKind::Url, "fetch", "a.ts"),
        make_literal("lb", "/b", LiteralKind::Url, "fetch", "b.ts"),
    ];
    let write_set_v1 = CanonicalWriteSet {
        files: &files_v1,
        symbols: &[],
        relationships: &[],
        identifiers: &[],
        types: &[],
        type_arguments: &[],
        literals: &literals_v1,
    };
    db.incremental_update_atomic_with_metadata(
        &[],
        &write_set_v1,
        "ws_test",
        AtomicPersistenceMetadata::default(),
    )
    .expect("seed write should succeed");
    assert_eq!(count_rows(&db, "literals"), 2, "precondition: two literals");

    let files_v2 = vec![make_file("a.ts")];
    let literals_v2 = vec![make_literal(
        "la2",
        "/a2",
        LiteralKind::Url,
        "fetch",
        "a.ts",
    )];
    let write_set_v2 = CanonicalWriteSet {
        files: &files_v2,
        symbols: &[],
        relationships: &[],
        identifiers: &[],
        types: &[],
        type_arguments: &[],
        literals: &literals_v2,
    };
    db.replace_workspace_data_atomic(
        &write_set_v2,
        "ws_test",
        AtomicPersistenceMetadata::default(),
    )
    .expect("full-replace rebuild should succeed");

    assert_eq!(
        count_rows(&db, "literals"),
        1,
        "full-replace rebuild must clear every prior literal"
    );
    let surviving: String = db
        .conn
        .query_row("SELECT literal_text FROM literals", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        surviving, "/a2",
        "only the rebuilt batch's literal may survive"
    );
}
