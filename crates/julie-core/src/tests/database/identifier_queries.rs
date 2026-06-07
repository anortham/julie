use super::*;

#[test]
fn test_identifier_presence_uses_temp_table_join_not_cross_product_chunks() {
    let source = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/database/identifiers.rs"),
    )
    .expect("identifier query source should be readable");

    assert!(
        source.contains("CREATE TEMP TABLE IF NOT EXISTS _julie_identifier_presence_files"),
        "identifier presence should stage file paths in a temp table"
    );
    assert!(
        source.contains("CREATE TEMP TABLE IF NOT EXISTS _julie_identifier_presence_names"),
        "identifier presence should stage names in a temp table"
    );
    assert!(
        source.contains("JOIN _julie_identifier_presence_files"),
        "identifier presence should join identifiers to staged file paths"
    );
    assert!(
        source.contains("JOIN _julie_identifier_presence_names"),
        "identifier presence should join identifiers to staged names"
    );
    assert!(
        !source.contains("for name_chunk in names.chunks"),
        "identifier presence must not run file-chunk x name-chunk query loops on large workspaces"
    );
}

#[test]
fn test_scoped_identifier_presence_has_exact_scope_name_query() {
    let source = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/database/identifiers.rs"),
    )
    .expect("identifier query source should be readable");

    assert!(
        source.contains("pub fn get_scoped_identifier_presence"),
        "resolver needs an exact scoped identifier presence query"
    );
    assert!(
        source.contains("CREATE TEMP TABLE IF NOT EXISTS _julie_identifier_presence_scopes"),
        "scoped identifier presence should stage scope ids in a temp table"
    );
    assert!(
        source.contains("i.containing_symbol_id = s.value"),
        "scoped identifier presence should constrain by containing_symbol_id in SQL"
    );
    assert!(
        source.contains("i.name = n.value"),
        "scoped identifier presence should use exact name matches"
    );
}

#[test]
fn test_get_scoped_identifier_presence_returns_exact_scope_name_pairs() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("scoped_identifier_presence.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    db.store_file_info(&file_info_builder("src/main.rs").build())
        .unwrap();
    db.store_symbols_transactional(&[
        symbol_builder("scope_a", "caller_a", "src/main.rs").build(),
        symbol_builder("scope_b", "caller_b", "src/main.rs").build(),
    ])
    .unwrap();
    db.bulk_store_identifiers(
        &[
            identifier_builder("id-1", "ApiResponse", "src/main.rs")
                .containing_symbol_id("scope_a")
                .build(),
            identifier_builder("id-2", "ApiResponse::new", "src/main.rs")
                .containing_symbol_id("scope_a")
                .build(),
            identifier_builder("id-3", "ApiResponse", "src/main.rs")
                .containing_symbol_id("scope_b")
                .build(),
            identifier_builder("id-4", "Other", "src/main.rs")
                .containing_symbol_id("scope_a")
                .build(),
        ],
        "test_workspace",
    )
    .unwrap();

    let refs = db
        .get_scoped_identifier_presence(&["scope_a"], &["ApiResponse"])
        .unwrap();

    assert_eq!(refs.len(), 1);
    assert!(refs.contains(&("scope_a".to_string(), "ApiResponse".to_string())));
}
