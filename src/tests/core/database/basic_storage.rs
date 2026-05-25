use super::*;

#[tokio::test]
async fn test_database_creation() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    #[allow(unused_mut)]
    let mut db = SymbolDatabase::new(&db_path).unwrap();
    let stats = db.get_stats().unwrap();

    assert_eq!(stats.total_symbols, 0);
    assert_eq!(stats.total_relationships, 0);
    assert_eq!(stats.total_files, 0);
}

#[test]
fn test_minimal_database_creation() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("minimal.db");

    // Test just the SQLite connection
    let conn = open_test_connection(&db_path).unwrap();

    // Test a simple table creation
    let result = conn.execute("CREATE TABLE test (id TEXT PRIMARY KEY, name TEXT)", []);

    // This should work without "Execute returned results" error
    assert!(result.is_ok());

    // Test a simple insert
    let insert_result = conn.execute("INSERT INTO test VALUES ('1', 'test')", []);
    assert!(insert_result.is_ok());
}

#[tokio::test]
async fn test_debug_foreign_key_constraint() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("debug.db");
    #[allow(unused_mut)]
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Create a temporary file
    let test_file = temp_dir.path().join("test.ts");
    std::fs::write(&test_file, "// test content").unwrap();

    // Store file info
    let file_info =
        crate::database::create_file_info(&test_file, "typescript", temp_dir.path()).unwrap();
    println!("File path in file_info: {}", file_info.path);
    db.store_file_info(&file_info).unwrap();

    // Create a symbol with the same file path (relative to match file_info)
    let file_path =
        crate::utils::paths::to_relative_unix_style(&test_file, temp_dir.path()).unwrap();
    println!("File path in symbol: {}", file_path);

    let symbol = Symbol {
        id: "test-symbol".to_string(),
        name: "testFunction".to_string(),
        kind: SymbolKind::Function,
        language: "typescript".to_string(),
        file_path: file_path,
        start_line: 1,
        start_column: 0,
        end_line: 1,
        end_column: 10,
        start_byte: 0,
        end_byte: 10,
        signature: None,
        doc_comment: None,
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: None,
        content_type: None,
        body_span: None,
        body_hash: None,
        annotations: Vec::new(),
    };

    // This should work without foreign key constraint error
    let result = db.store_symbols_transactional(&[symbol]);
    assert!(
        result.is_ok(),
        "Foreign key constraint failed: {:?}",
        result
    );
}

#[test]
fn test_individual_table_creation() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("individual.db");

    // Create a SymbolDatabase instance manually to test each table individually
    let conn = open_test_connection(&db_path).unwrap();
    let db = SymbolDatabase {
        conn: crate::database::SymbolDatabaseConn::Owned(conn),
        file_path: db_path,
    };

    // Test files table creation
    let files_result = db.create_files_table();
    assert!(
        files_result.is_ok(),
        "Files table creation failed: {:?}",
        files_result
    );

    // Test symbols table creation
    let symbols_result = db.create_symbols_table();
    assert!(
        symbols_result.is_ok(),
        "Symbols table creation failed: {:?}",
        symbols_result
    );

    // Test relationships table creation
    let relationships_result = db.create_relationships_table();
    assert!(
        relationships_result.is_ok(),
        "Relationships table creation failed: {:?}",
        relationships_result
    );
}

#[tokio::test]
async fn test_file_info_storage() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    #[allow(unused_mut)]
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    let file_info = FileInfo {
        path: "test.rs".to_string(),
        language: "rust".to_string(),
        hash: "abcd1234".to_string(),
        size: 1024,
        last_modified: 1234567890,
        last_indexed: 0,
        symbol_count: 5,
        line_count: 0,
        content: None,
    };

    db.store_file_info(&file_info).unwrap();

    let hash = db.get_file_hash("test.rs").unwrap();
    assert_eq!(hash, Some("abcd1234".to_string()));
}

#[tokio::test]
async fn test_symbol_storage_and_retrieval() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    let symbol = Symbol {
        id: "test-symbol-1".to_string(),
        name: "test_function".to_string(),
        kind: SymbolKind::Function,
        language: "rust".to_string(),
        file_path: "test.rs".to_string(),
        start_line: 10,
        start_column: 0,
        end_line: 15,
        end_column: 1,
        start_byte: 0,
        end_byte: 0,
        signature: Some("fn test_function()".to_string()),
        doc_comment: None,
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: None,
        content_type: None,
        body_span: None,
        body_hash: None,
        annotations: Vec::new(),
    };

    // Following foreign key contract: store file record first
    let file_info = FileInfo {
        path: "test.rs".to_string(),
        language: "rust".to_string(),
        hash: "test-hash".to_string(),
        size: 100,
        last_modified: 1234567890,
        last_indexed: 0,
        symbol_count: 1,
        line_count: 0,
        content: None,
    };
    db.store_file_info(&file_info).unwrap();

    db.store_symbols_transactional(&[symbol.clone()]).unwrap();

    let retrieved = db.get_symbol_by_id("test-symbol-1").unwrap();
    assert!(retrieved.is_some());

    let retrieved_symbol = retrieved.unwrap();
    assert_eq!(retrieved_symbol.name, "test_function");
    assert_eq!(retrieved_symbol.language, "rust");
}

#[test]
fn test_bulk_store_symbols_for_existing_file_paths() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("bulk.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Use a real Go fixture to mirror the production failure scenario
    let fixture_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/real-world/go/main.go");
    let fixture_content = std::fs::read_to_string(&fixture_path).unwrap();

    let workspace_root = fixture_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("/tmp/test"));
    let file_info = crate::database::create_file_info(&fixture_path, "go", workspace_root).unwrap();
    db.bulk_store_files(&[file_info]).unwrap();

    let mut parser = Parser::new();
    let go_lang = crate::language::get_tree_sitter_language("go").unwrap();
    parser.set_language(&go_lang).unwrap();
    let tree = parser.parse(&fixture_content, None).unwrap();
    let mut extractor = crate::extractors::go::GoExtractor::new(
        "go".to_string(),
        fixture_path.to_string_lossy().to_string(),
        fixture_content,
        workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);

    assert!(!symbols.is_empty(), "Expected fixture to produce symbols");

    let result = db.bulk_store_symbols(&symbols, "test_workspace");
    assert!(
        result.is_ok(),
        "Bulk store should succeed without foreign key violations: {:?}",
        result
    );
}

#[tokio::test]
async fn test_symbol_with_metadata_and_semantic_fields() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Create a temporary file for the test
    let test_file = temp_dir.path().join("user.ts");
    std::fs::write(&test_file, "// test file content").unwrap();

    // Create symbol with all new fields populated
    let mut metadata = HashMap::new();
    metadata.insert("isAsync".to_string(), serde_json::Value::Bool(true));
    metadata.insert(
        "returnType".to_string(),
        serde_json::Value::String("Promise<User>".to_string()),
    );

    let symbol = Symbol {
        id: "test-symbol-complex".to_string(),
        name: "getUserAsync".to_string(),
        kind: SymbolKind::Function,
        language: "typescript".to_string(),
        file_path: crate::utils::paths::to_relative_unix_style(&test_file, temp_dir.path())
            .unwrap(),
        start_line: 20,
        start_column: 4,
        end_line: 30,
        end_column: 1,
        start_byte: 500,
        end_byte: 800,
        signature: Some("async getUserAsync(id: string): Promise<User>".to_string()),
        doc_comment: Some("Fetches user data asynchronously".to_string()),
        visibility: Some(crate::extractors::base::Visibility::Public),
        parent_id: None, // No parent for this test
        metadata: Some(metadata.clone()),
        semantic_group: Some("user-data-access".to_string()),
        confidence: Some(0.95),
        code_context: None,
        content_type: None,
        body_span: None,
        body_hash: None,
        annotations: Vec::new(),
    };

    // First, store the file record (required due to foreign key constraint)
    let file_info =
        crate::database::create_file_info(&test_file, "typescript", temp_dir.path()).unwrap();
    println!("DEBUG: File path in file_info: {}", file_info.path);
    println!("DEBUG: Symbol file path: {}", symbol.file_path);
    db.store_file_info(&file_info).unwrap();

    // Store the symbol
    db.store_symbols_transactional(&[symbol.clone()]).unwrap();

    // Retrieve and verify all fields are preserved
    let retrieved = db.get_symbol_by_id("test-symbol-complex").unwrap().unwrap();

    assert_eq!(retrieved.name, "getUserAsync");
    assert_eq!(
        retrieved.semantic_group,
        Some("user-data-access".to_string())
    );
    assert_eq!(retrieved.confidence, Some(0.95));

    // Verify metadata is properly stored and retrieved
    let retrieved_metadata = retrieved.metadata.unwrap();
    assert_eq!(
        retrieved_metadata
            .get("isAsync")
            .unwrap()
            .as_bool()
            .unwrap(),
        true
    );
    assert_eq!(
        retrieved_metadata
            .get("returnType")
            .unwrap()
            .as_str()
            .unwrap(),
        "Promise<User>"
    );
}
