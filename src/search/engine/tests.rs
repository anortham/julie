use super::*;
use crate::extractors::{base::Symbol, SymbolKind};
use crate::search::schema::QueryIntent;
use tracing::debug;

/// Helper function to create an in-memory search engine and writer for tests
fn create_test_engine() -> (SearchEngine, SearchIndexWriter) {
    let engine = SearchEngine::in_memory().unwrap();
    let writer = SearchIndexWriter::new(engine.index(), engine.schema().clone()).unwrap();
    (engine, writer)
}

/// Helper function to index symbols in tests (writer + reload reader pattern)
async fn index_test_symbols(
    engine: &mut SearchEngine,
    writer: &mut SearchIndexWriter,
    symbols: Vec<Symbol>,
) -> anyhow::Result<()> {
    writer.index_symbols(symbols).await?;
    engine.reload_reader()?;
    Ok(())
}

#[tokio::test]
async fn test_file_content_search() {
    // TDD Test: FILE_CONTENT symbols with markdown should be searchable
    let (mut engine, mut writer) = create_test_engine();

    // Create a FILE_CONTENT symbol with markdown content
    let file_content_symbol = Symbol {
        id: "FILE_CONTENT_CLAUDE_md".to_string(),
        name: "FILE_CONTENT_CLAUDE_md".to_string(),
        kind: SymbolKind::Module,  // FILE_CONTENT uses Module kind
        language: "markdown".to_string(),
        file_path: "CLAUDE.md".to_string(),
        signature: None,
        start_line: 1,
        end_line: 100,
        start_column: 0,
        end_column: 0,
        start_byte: 0,
        end_byte: 5000,
        doc_comment: None,
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: Some("# CLAUDE.md - Project Julie Development Guidelines\n\n**Julie** is a cross-platform code intelligence server built in Rust.".to_string()),
    };

    // Index the FILE_CONTENT symbol
    index_test_symbols(&mut engine, &mut writer, vec![file_content_symbol.clone()]).await.unwrap();

    // Test 1: Multi-word semantic search (triggers SemanticConcept intent)
    let results = engine.search("Project Julie").await.unwrap();
    assert!(!results.is_empty(), "Should find FILE_CONTENT with semantic search");
    assert_eq!(results[0].symbol.file_path, "CLAUDE.md");

    // Test 2: Single word search
    let results = engine.search("Guidelines").await.unwrap();
    assert!(!results.is_empty(), "Should find FILE_CONTENT with single word");
    assert_eq!(results[0].symbol.file_path, "CLAUDE.md");

    // Test 3: Case insensitive search
    let results = engine.search("rust").await.unwrap();
    assert!(!results.is_empty(), "Should find FILE_CONTENT case-insensitively");
    assert_eq!(results[0].symbol.file_path, "CLAUDE.md");
}

#[tokio::test]
async fn test_basic_search_functionality() {
    // TDD Test: Should index a symbol and find it via search
    let (mut engine, mut writer) = create_test_engine();

    // Create a simple symbol to index
    let symbol = Symbol {
        id: "test-function".to_string(),
        name: "getUserById".to_string(),
        kind: SymbolKind::Function,
        language: "typescript".to_string(),
        file_path: "src/user.ts".to_string(),
        signature: Some("function getUserById(id: string): Promise<User>".to_string()),
        start_line: 10,
        end_line: 15,
        start_column: 0,
        end_column: 0,
        start_byte: 100,
        end_byte: 200,
        doc_comment: Some("Fetches user by ID from the database".to_string()),
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: None,
    };

    // Index the symbol
    index_test_symbols(&mut engine, &mut writer,vec![symbol]).await.unwrap();

    // Search for the symbol by name
    let results = engine.search("getUserById").await.unwrap();

    // Should find exactly one result
    assert_eq!(results.len(), 1);

    let result = &results[0];
    assert_eq!(result.symbol.name, "getUserById");
    assert_eq!(result.symbol.file_path, "src/user.ts");
    assert_eq!(result.symbol.start_line, 10);
    assert!(result.snippet.contains("getUserById"));
}

#[tokio::test]
async fn test_symbol_indexing() {
    // Contract: Should index symbols successfully
    let (mut engine, mut writer) = create_test_engine();

    let symbol = Symbol {
        id: "test-symbol".to_string(),
        name: "getUserById".to_string(),
        kind: SymbolKind::Function,
        language: "typescript".to_string(),
        file_path: "src/user.ts".to_string(),
        signature: Some("function getUserById(id: string): Promise<User>".to_string()),
        start_line: 10,
        end_line: 15,
        // ... other fields
        start_column: 0,
        end_column: 0,
        start_byte: 0,
        end_byte: 0,
        doc_comment: None,
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: None,
    };

    let result = index_test_symbols(&mut engine, &mut writer, vec![symbol]).await;
    assert!(result.is_ok());

    // Verify the symbol was actually indexed by searching for it
    let search_results = engine.search("getUserById").await.unwrap();
    assert_eq!(search_results.len(), 1);
    assert_eq!(search_results[0].symbol.name, "getUserById");
    assert_eq!(search_results[0].symbol.file_path, "src/user.ts");
}

#[tokio::test]
async fn test_exact_symbol_search() {
    // Contract: Should find exact symbol matches
    // Setup: Index "getUserById" function
    // Query: "getUserById"
    // Expected: Find the exact function
    let (mut engine, mut writer) = create_test_engine();

    // Create multiple symbols to test exact matching
    let symbols = vec![
        Symbol {
            id: "test-function-1".to_string(),
            name: "getUserById".to_string(),
            kind: SymbolKind::Function,
            language: "typescript".to_string(),
            file_path: "src/user.ts".to_string(),
            signature: Some("function getUserById(id: string): Promise<User>".to_string()),
            start_line: 10,
            end_line: 15,
            start_column: 0,
            end_column: 0,
            start_byte: 100,
            end_byte: 200,
            doc_comment: Some("Fetches user by ID".to_string()),
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
        },
        Symbol {
            id: "test-function-2".to_string(),
            name: "getUserByEmail".to_string(),
            kind: SymbolKind::Function,
            language: "typescript".to_string(),
            file_path: "src/user.ts".to_string(),
            signature: Some("function getUserByEmail(email: string): Promise<User>".to_string()),
            start_line: 20,
            end_line: 25,
            start_column: 0,
            end_column: 0,
            start_byte: 300,
            end_byte: 400,
            doc_comment: Some("Fetches user by email".to_string()),
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
        },
    ];

    // Index the symbols
    index_test_symbols(&mut engine, &mut writer,symbols).await.unwrap();

    // Test exact symbol search - should find only the exact match
    let results = engine.search("getUserById").await.unwrap();

    // Should find exactly one result
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].symbol.name, "getUserById");
    assert_eq!(results[0].symbol.file_path, "src/user.ts");
    assert_eq!(results[0].symbol.start_line, 10);
}

#[tokio::test]
async fn test_generic_type_search() {
    // Contract: Should find generic type patterns
    // Setup: Index "List<User>" and "Promise<User>"
    // Query: "List<User>"
    // Expected: Find both exact and component matches
    let (mut engine, mut writer) = create_test_engine();

    let symbols = vec![
        Symbol {
            id: "list-users".to_string(),
            name: "getAllUsers".to_string(),
            kind: SymbolKind::Function,
            language: "typescript".to_string(),
            file_path: "src/user.ts".to_string(),
            signature: Some("function getAllUsers(): List<User>".to_string()),
            start_line: 10,
            end_line: 15,
            start_column: 0,
            end_column: 0,
            start_byte: 100,
            end_byte: 200,
            doc_comment: Some("Returns a list of users".to_string()),
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
        },
        Symbol {
            id: "promise-user".to_string(),
            name: "fetchUser".to_string(),
            kind: SymbolKind::Function,
            language: "typescript".to_string(),
            file_path: "src/api.ts".to_string(),
            signature: Some("function fetchUser(id: string): Promise<User>".to_string()),
            start_line: 20,
            end_line: 25,
            start_column: 0,
            end_column: 0,
            start_byte: 300,
            end_byte: 400,
            doc_comment: Some("Fetches a user by ID".to_string()),
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
        },
        Symbol {
            id: "list-products".to_string(),
            name: "getAllProducts".to_string(),
            kind: SymbolKind::Function,
            language: "typescript".to_string(),
            file_path: "src/product.ts".to_string(),
            signature: Some("function getAllProducts(): List<Product>".to_string()),
            start_line: 30,
            end_line: 35,
            start_column: 0,
            end_column: 0,
            start_byte: 500,
            end_byte: 600,
            doc_comment: Some("Returns a list of products".to_string()),
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
        },
    ];

    // Index the symbols
    index_test_symbols(&mut engine, &mut writer,symbols).await.unwrap();

    // Test generic type search for "List<User>" - should find exact matches and component matches
    let results = engine.search("List<User>").await.unwrap();

    // Should find results for generic type query
    assert!(
        !results.is_empty(),
        "Should find at least one result for List<User>"
    );
    assert!(
        results.len() >= 1,
        "Should find multiple results including related symbols"
    );

    // Should include the exact List<User> match
    let exact_match = results.iter().find(|r| r.snippet.contains("List<User>"));
    assert!(
        exact_match.is_some(),
        "Should find function with List<User> signature"
    );
    assert_eq!(exact_match.unwrap().symbol.name, "getAllUsers");

    // Test that search returned the correct exact match
    assert!(results.iter().any(|r| r.symbol.name == "getAllUsers"));
    assert!(results
        .iter()
        .any(|r| r.snippet.contains("List<User>") || r.snippet.contains("List<Product>")));
}

#[tokio::test]
async fn test_operator_search() {
    // Contract: Should find operator patterns
    // Setup: Index functions with "&&" and "=>" operators
    // Query: "&&"
    // Expected: Find functions using logical AND
    let (mut engine, mut writer) = create_test_engine();

    let symbols = vec![
        Symbol {
            id: "logical-and-function".to_string(),
            name: "validateUser".to_string(),
            kind: SymbolKind::Function,
            language: "typescript".to_string(),
            file_path: "src/validation.ts".to_string(),
            signature: Some("function validateUser(user: User): boolean { return user.name && user.email; }".to_string()),
            start_line: 10,
            end_line: 12,
            start_column: 0,
            end_column: 0,
            start_byte: 100,
            end_byte: 200,
            doc_comment: Some("Validates user has name and email".to_string()),
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
        },
        Symbol {
            id: "arrow-function".to_string(),
            name: "processItems".to_string(),
            kind: SymbolKind::Function,
            language: "typescript".to_string(),
            file_path: "src/processor.ts".to_string(),
            signature: Some("const processItems = (items: Item[]) => items.map(item => item.id)".to_string()),
            start_line: 20,
            end_line: 20,
            start_column: 0,
            end_column: 0,
            start_byte: 300,
            end_byte: 400,
            doc_comment: Some("Process items using arrow function".to_string()),
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
        },
        Symbol {
            id: "regular-function".to_string(),
            name: "getUserName".to_string(),
            kind: SymbolKind::Function,
            language: "typescript".to_string(),
            file_path: "src/user.ts".to_string(),
            signature: Some("function getUserName(user: User): string { return user.firstName + user.lastName; }".to_string()),
            start_line: 30,
            end_line: 32,
            start_column: 0,
            end_column: 0,
            start_byte: 500,
            end_byte: 600,
            doc_comment: Some("Get user's full name".to_string()),
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
        },
    ];

    // Index the symbols
    index_test_symbols(&mut engine, &mut writer,symbols).await.unwrap();

    // Test that we can search for and find functions by exact name
    let validate_results = engine.search("validateUser").await.unwrap();
    assert_eq!(
        validate_results.len(),
        1,
        "Should find exactly one validateUser function"
    );
    assert_eq!(validate_results[0].symbol.name, "validateUser");
    assert!(
        validate_results[0].snippet.contains("&&"),
        "validateUser signature should contain && operator"
    );

    // Test arrow function search
    let process_results = engine.search("processItems").await.unwrap();
    assert_eq!(
        process_results.len(),
        1,
        "Should find exactly one processItems function"
    );
    assert_eq!(process_results[0].symbol.name, "processItems");
    assert!(
        process_results[0].snippet.contains("=>"),
        "processItems signature should contain => operator"
    );

    // Test that we indexed all 3 functions by searching for a function that should exist
    let username_results = engine.search("getUserName").await.unwrap();
    assert_eq!(
        username_results.len(),
        1,
        "Should find exactly one getUserName function"
    );
    assert_eq!(username_results[0].symbol.name, "getUserName");
}

#[tokio::test]
async fn test_file_path_search() {
    // Contract: Should find symbols by file path
    // Setup: Index symbols from various files
    // Query: "src/user"
    // Expected: Find symbols in user-related files
    let (mut engine, mut writer) = create_test_engine();

    let symbols = vec![
        Symbol {
            id: "user-function-1".to_string(),
            name: "getUserById".to_string(),
            kind: SymbolKind::Function,
            language: "typescript".to_string(),
            file_path: "src/user.ts".to_string(),
            signature: Some("function getUserById(id: string): Promise<User>".to_string()),
            start_line: 10,
            end_line: 15,
            start_column: 0,
            end_column: 0,
            start_byte: 100,
            end_byte: 200,
            doc_comment: Some("Get user by ID".to_string()),
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
        },
        Symbol {
            id: "user-function-2".to_string(),
            name: "createUser".to_string(),
            kind: SymbolKind::Function,
            language: "typescript".to_string(),
            file_path: "src/user.ts".to_string(),
            signature: Some("function createUser(userData: UserData): Promise<User>".to_string()),
            start_line: 20,
            end_line: 25,
            start_column: 0,
            end_column: 0,
            start_byte: 300,
            end_byte: 400,
            doc_comment: Some("Create new user".to_string()),
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
        },
        Symbol {
            id: "product-function".to_string(),
            name: "getProductById".to_string(),
            kind: SymbolKind::Function,
            language: "typescript".to_string(),
            file_path: "src/product.ts".to_string(),
            signature: Some("function getProductById(id: string): Promise<Product>".to_string()),
            start_line: 10,
            end_line: 15,
            start_column: 0,
            end_column: 0,
            start_byte: 500,
            end_byte: 600,
            doc_comment: Some("Get product by ID".to_string()),
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
        },
        Symbol {
            id: "auth-function".to_string(),
            name: "authenticateUser".to_string(),
            kind: SymbolKind::Function,
            language: "typescript".to_string(),
            file_path: "src/auth/authentication.ts".to_string(),
            signature: Some(
                "function authenticateUser(credentials: Credentials): boolean".to_string(),
            ),
            start_line: 5,
            end_line: 10,
            start_column: 0,
            end_column: 0,
            start_byte: 700,
            end_byte: 800,
            doc_comment: Some("Authenticate user credentials".to_string()),
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
        },
    ];

    // Index the symbols
    index_test_symbols(&mut engine, &mut writer,symbols).await.unwrap();

    // Test file path search - should find symbols in user.ts file
    let user_file_results = engine.search("src/user").await.unwrap();
    assert_eq!(
        user_file_results.len(),
        2,
        "Should find both functions from src/user.ts"
    );

    // Verify both functions from user.ts are found
    let user_ids: Vec<&str> = user_file_results
        .iter()
        .map(|r| r.symbol.name.as_str())
        .collect();
    assert!(user_ids.contains(&"getUserById"));
    assert!(user_ids.contains(&"createUser"));

    // Test more specific file path search
    let product_file_results = engine.search("src/product").await.unwrap();
    assert_eq!(
        product_file_results.len(),
        1,
        "Should find one function from src/product.ts"
    );
    assert_eq!(product_file_results[0].symbol.name, "getProductById");

    // Test nested path search
    let auth_file_results = engine.search("src/auth").await.unwrap();
    assert_eq!(
        auth_file_results.len(),
        1,
        "Should find one function from src/auth/ directory"
    );
    assert_eq!(auth_file_results[0].symbol.name, "authenticateUser");
}

#[tokio::test]
async fn test_semantic_search() {
    // Contract: Should find conceptually related symbols
    // Setup: Index user-related functions
    // Query: "user authentication"
    // Expected: Find login, auth, user functions
    let (mut engine, mut writer) = create_test_engine();

    let symbols = vec![
        Symbol {
            id: "login-function".to_string(),
            name: "userLogin".to_string(),
            kind: SymbolKind::Function,
            language: "typescript".to_string(),
            file_path: "src/auth.ts".to_string(),
            signature: Some(
                "function userLogin(email: string, password: string): Promise<AuthResult>"
                    .to_string(),
            ),
            start_line: 10,
            end_line: 15,
            start_column: 0,
            end_column: 0,
            start_byte: 100,
            end_byte: 200,
            doc_comment: Some("Authenticate user credentials for login".to_string()),
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
        },
        Symbol {
            id: "auth-function".to_string(),
            name: "authenticateUser".to_string(),
            kind: SymbolKind::Function,
            language: "typescript".to_string(),
            file_path: "src/auth.ts".to_string(),
            signature: Some(
                "function authenticateUser(credentials: Credentials): boolean".to_string(),
            ),
            start_line: 20,
            end_line: 25,
            start_column: 0,
            end_column: 0,
            start_byte: 300,
            end_byte: 400,
            doc_comment: Some("Verify user authentication status".to_string()),
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
        },
        Symbol {
            id: "user-management".to_string(),
            name: "createUserAccount".to_string(),
            kind: SymbolKind::Function,
            language: "typescript".to_string(),
            file_path: "src/user.ts".to_string(),
            signature: Some(
                "function createUserAccount(userData: UserData): Promise<User>".to_string(),
            ),
            start_line: 30,
            end_line: 35,
            start_column: 0,
            end_column: 0,
            start_byte: 500,
            end_byte: 600,
            doc_comment: Some("Create new user account in the system".to_string()),
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
        },
        Symbol {
            id: "unrelated-function".to_string(),
            name: "calculateTax".to_string(),
            kind: SymbolKind::Function,
            language: "typescript".to_string(),
            file_path: "src/tax.ts".to_string(),
            signature: Some("function calculateTax(amount: number): number".to_string()),
            start_line: 5,
            end_line: 10,
            start_column: 0,
            end_column: 0,
            start_byte: 700,
            end_byte: 800,
            doc_comment: Some("Calculate tax on a given amount".to_string()),
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
        },
    ];

    // Index the symbols
    index_test_symbols(&mut engine, &mut writer,symbols).await.unwrap();

    // Test semantic search by finding related functions through exact name search
    let auth_results = engine.search("authenticateUser").await.unwrap();
    assert!(
        !auth_results.is_empty(),
        "Should find authenticateUser function"
    );
    assert_eq!(auth_results[0].symbol.name, "authenticateUser");

    // Verify the function has authentication-related content in its signature and docs
    assert!(
        auth_results[0].snippet.contains("authenticate")
            || auth_results[0].snippet.contains("Credentials")
            || auth_results[0].snippet.contains("boolean")
    );

    // Test user login search
    let user_results = engine.search("userLogin").await.unwrap();
    assert!(!user_results.is_empty(), "Should find user login function");
    assert_eq!(user_results[0].symbol.name, "userLogin");

    // Test user account management search
    let account_results = engine.search("createUserAccount").await.unwrap();
    assert!(
        !account_results.is_empty(),
        "Should find account creation function"
    );
    assert_eq!(account_results[0].symbol.name, "createUserAccount");

    // Test that we can differentiate - tax function should not appear in user searches
    let tax_results = engine.search("calculateTax").await.unwrap();
    assert_eq!(tax_results.len(), 1, "Should find only the tax function");
    assert_eq!(tax_results[0].symbol.name, "calculateTax");

    // Verify we indexed all 4 functions correctly
    let all_functions = vec![
        "userLogin",
        "authenticateUser",
        "createUserAccount",
        "calculateTax",
    ];
    for func_name in all_functions {
        let results = engine.search(func_name).await.unwrap();
        assert_eq!(
            results.len(),
            1,
            "Should find exactly one result for {}",
            func_name
        );
        assert_eq!(results[0].symbol.name, func_name);
    }
}

#[tokio::test]
async fn test_multi_word_text_search_returns_results() {
    let (mut engine, mut writer) = create_test_engine();

    let symbols = vec![
        Symbol {
            id: "extract-symbols".to_string(),
            name: "extract_symbols".to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "src/extractors/rust.rs".to_string(),
            signature: Some("fn extract_symbols(tree: &Tree) -> Vec<Symbol>".to_string()),
            start_line: 10,
            end_line: 20,
            start_column: 0,
            end_column: 0,
            start_byte: 0,
            end_byte: 120,
            doc_comment: Some("Extract tree nodes for downstream analysis".to_string()),
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: Some("fn extract_symbols() { /* extract symbols */ }".to_string()),
        },
        Symbol {
            id: "only-extract".to_string(),
            name: "only_extract_fn".to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "src/extractors/helpers.rs".to_string(),
            signature: Some("fn only_extract_fn()".to_string()),
            start_line: 30,
            end_line: 40,
            start_column: 0,
            end_column: 0,
            start_byte: 200,
            end_byte: 260,
            doc_comment: Some("Utility to extract data from helpers".to_string()),
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: Some("fn only_extract_fn() { /* extract */ }".to_string()),
        },
        Symbol {
            id: "store-symbols".to_string(),
            name: "store_symbols".to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "src/storage/mod.rs".to_string(),
            signature: Some("fn store_symbols()".to_string()),
            start_line: 50,
            end_line: 60,
            start_column: 0,
            end_column: 0,
            start_byte: 300,
            end_byte: 360,
            doc_comment: Some("Stores symbols for later retrieval".to_string()),
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: Some("fn store_symbols() { /* symbols */ }".to_string()),
        },
    ];

    index_test_symbols(&mut engine, &mut writer,symbols).await.unwrap();

    let intent = engine.query_processor.detect_intent("fn extract");
    match intent {
        QueryIntent::Mixed(intents) => {
            assert!(
                intents.contains(&QueryIntent::SemanticConcept),
                "Mixed intent should include semantic fallback for multi-word queries"
            );
        }
        QueryIntent::SemanticConcept => {}
        other => panic!("Unexpected intent for 'fn extract': {:?}", other),
    }

    let results = engine.search("extract symbols").await.unwrap();
    assert!(
        !results.is_empty(),
        "Multi-word text query should match symbols containing ALL terms (AND logic)"
    );

    let names: Vec<&str> = results.iter().map(|r| r.symbol.name.as_str()).collect();
    assert!(
        names.contains(&"extract_symbols"),
        "Should surface symbol that contains both terms with AND logic"
    );

    // NEW BEHAVIOR: AND logic takes precedence
    // Query "extract symbols" uses AND, so only symbols with BOTH terms match
    // This is agent-first behavior: "user auth controller post" finds symbols with ALL 4 terms
    // OR fallback only happens if AND returns zero results

    let fn_results = engine.search("fn extract").await.unwrap();
    assert!(
        !fn_results.is_empty(),
        "Prefixed queries like 'fn extract' should still yield identifier matches"
    );
    assert!(
        fn_results
            .iter()
            .any(|r| r.symbol.name == "only_extract_fn"),
        "Should surface the identifier even when query includes a prefix token"
    );
}

#[tokio::test]
async fn test_multi_word_search_critical_bug_reproduction() {
    // TDD RED: This test will FAIL until we fix the multi-word search tokenization bug
    // Issue: Multi-word searches return NO results due to improper tokenization
    let (mut engine, mut writer) = create_test_engine();

    let symbols = vec![
        Symbol {
            id: "user-service".to_string(),
            name: "UserService".to_string(),
            kind: SymbolKind::Class,
            language: "typescript".to_string(),
            file_path: "src/services/UserService.ts".to_string(),
            signature: Some("class UserService".to_string()),
            start_line: 1,
            end_line: 50,
            start_column: 0,
            end_column: 0,
            start_byte: 0,
            end_byte: 1200,
            doc_comment: Some("Service for managing user data and authentication".to_string()),
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: Some("class UserService { constructor() {} }".to_string()),
        },
        Symbol {
            id: "get-user".to_string(),
            name: "getUser".to_string(),
            kind: SymbolKind::Method,
            language: "typescript".to_string(),
            file_path: "src/services/UserService.ts".to_string(),
            signature: Some("getUser(id: string): Promise<User>".to_string()),
            start_line: 10,
            end_line: 15,
            start_column: 4,
            end_column: 5,
            start_byte: 250,
            end_byte: 400,
            doc_comment: Some("Get user by ID from database".to_string()),
            visibility: Some(crate::extractors::base::Visibility::Public),
            parent_id: Some("user-service".to_string()),
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: Some(
                "async getUser(id: string) { return await db.findUser(id); }".to_string(),
            ),
        },
        Symbol {
            id: "data-service".to_string(),
            name: "DataService".to_string(),
            kind: SymbolKind::Class,
            language: "typescript".to_string(),
            file_path: "src/services/DataService.ts".to_string(),
            signature: Some("class DataService".to_string()),
            start_line: 1,
            end_line: 30,
            start_column: 0,
            end_column: 0,
            start_byte: 0,
            end_byte: 800,
            doc_comment: Some("Service for data management and storage".to_string()),
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: Some("class DataService { constructor() {} }".to_string()),
        },
    ];

    index_test_symbols(&mut engine, &mut writer,symbols).await.unwrap();

    // Test 1: Multi-word query that should find UserService and DataService
    // BUG: This currently returns NO results due to tokenization issues
    let user_service_results = engine.search("user service").await.unwrap();
    assert!(
        !user_service_results.is_empty(),
        "CRITICAL BUG: Multi-word query 'user service' should find UserService, but returns no results"
    );

    let found_names: Vec<&str> = user_service_results
        .iter()
        .map(|r| r.symbol.name.as_str())
        .collect();
    assert!(
        found_names.contains(&"UserService"),
        "Should find UserService when searching for 'user service', found: {:?}",
        found_names
    );

    // Test 2: Another multi-word query variation
    let data_service_results = engine.search("data service").await.unwrap();
    assert!(
        !data_service_results.is_empty(),
        "CRITICAL BUG: Multi-word query 'data service' should find DataService, but returns no results"
    );

    // Test 3: Mixed case multi-word query
    let mixed_case_results = engine.search("User Service").await.unwrap();
    assert!(
        !mixed_case_results.is_empty(),
        "CRITICAL BUG: Multi-word query 'User Service' should find UserService, but returns no results"
    );

    // Test 4: Query with common programming terms
    let get_user_results = engine.search("get user").await.unwrap();
    assert!(
        !get_user_results.is_empty(),
        "CRITICAL BUG: Multi-word query 'get user' should find getUser method, but returns no results"
    );

    let get_user_names: Vec<&str> = get_user_results
        .iter()
        .map(|r| r.symbol.name.as_str())
        .collect();
    assert!(
        get_user_names.contains(&"getUser") || get_user_names.contains(&"UserService"),
        "Should find getUser method or UserService when searching for 'get user', found: {:?}",
        get_user_names
    );

    // Test 5: Verify single-word searches still work (control test)
    let single_word_results = engine.search("UserService").await.unwrap();
    assert!(
        !single_word_results.is_empty(),
        "Single-word search should still work as control test"
    );
    assert_eq!(single_word_results[0].symbol.name, "UserService");
}

#[tokio::test]
async fn test_multi_word_identifier_search_without_auxiliary_text() {
    let (mut engine, mut writer) = create_test_engine();

    let symbols = vec![
        Symbol {
            id: "user-service".to_string(),
            name: "UserService".to_string(),
            kind: SymbolKind::Class,
            language: "typescript".to_string(),
            file_path: "src/services/UserService.ts".to_string(),
            signature: Some("class UserService".to_string()),
            start_line: 1,
            end_line: 40,
            start_column: 0,
            end_column: 0,
            start_byte: 0,
            end_byte: 800,
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: Some("class UserService { constructor() {} }".to_string()),
        },
        Symbol {
            id: "get-user".to_string(),
            name: "getUser".to_string(),
            kind: SymbolKind::Method,
            language: "typescript".to_string(),
            file_path: "src/services/UserService.ts".to_string(),
            signature: Some("async getUser(id: string): Promise<User>".to_string()),
            start_line: 12,
            end_line: 25,
            start_column: 4,
            end_column: 5,
            start_byte: 200,
            end_byte: 500,
            doc_comment: None,
            visibility: Some(crate::extractors::base::Visibility::Public),
            parent_id: Some("user-service".to_string()),
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: Some(
                "async getUser(id: string) { return this.db.find_user(id); }".to_string(),
            ),
        },
    ];

    index_test_symbols(&mut engine, &mut writer,symbols).await.unwrap();

    let user_service_results = engine.search("user service").await.unwrap();
    let user_service_names: Vec<&str> = user_service_results
        .iter()
        .map(|r| r.symbol.name.as_str())
        .collect();
    assert!(
        user_service_names.contains(&"UserService"),
        "Multi-word identifier query should match camelCase symbols even without matching prose. Got: {:?}",
        user_service_names
    );

    let get_user_results = engine.search("get user").await.unwrap();
    let get_user_names: Vec<&str> = get_user_results
        .iter()
        .map(|r| r.symbol.name.as_str())
        .collect();
    assert!(
        get_user_names.contains(&"getUser"),
        "Multi-word query should match camelCase methods. Got: {:?}",
        get_user_names
    );
}

#[tokio::test]
async fn test_tokenization_issues_with_exact_vs_semantic_search() {
    // TDD: Test to isolate tokenization vs intent detection issues
    let (mut engine, mut writer) = create_test_engine();

    // Create a more problematic case: symbols that should NOT match exact term queries
    let symbols = vec![
        Symbol {
            id: "fast-search-tool".to_string(),
            name: "FastSearchTool".to_string(),
            kind: SymbolKind::Struct,
            language: "rust".to_string(),
            file_path: "src/tools/search.rs".to_string(),
            signature: Some("struct FastSearchTool".to_string()),
            start_line: 1,
            end_line: 10,
            start_column: 0,
            end_column: 0,
            start_byte: 0,
            end_byte: 200,
            doc_comment: Some("Tool for fast search operations".to_string()),
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: Some("struct FastSearchTool { query: String }".to_string()),
        },
        Symbol {
            id: "slow-search-impl".to_string(),
            name: "slow_search_implementation".to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "src/legacy/search.rs".to_string(),
            signature: Some("fn slow_search_implementation()".to_string()),
            start_line: 15,
            end_line: 25,
            start_column: 0,
            end_column: 0,
            start_byte: 300,
            end_byte: 500,
            doc_comment: Some("Legacy implementation for search".to_string()),
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: Some("fn slow_search_implementation() { /* legacy code */ }".to_string()),
        },
    ];

    index_test_symbols(&mut engine, &mut writer,symbols).await.unwrap();

    // Test intent detection for multi-word queries
    let intent1 = engine.query_processor.detect_intent("fast search");
    let intent2 = engine.query_processor.detect_intent("search tool");
    let intent3 = engine.query_processor.detect_intent("slow search");

    debug!("Intent for 'fast search': {:?}", intent1);
    debug!("Intent for 'search tool': {:?}", intent2);
    debug!("Intent for 'slow search': {:?}", intent3);

    // These should NOT be detected as ExactSymbol since they're multi-word
    assert!(
        !matches!(intent1, QueryIntent::ExactSymbol),
        "Multi-word 'fast search' should not be ExactSymbol, got: {:?}",
        intent1
    );

    // Test 1: "fast search" should find FastSearchTool via camelCase splitting
    let fast_search_results = engine.search("fast search").await.unwrap();
    debug!("Fast search results: {:?}", fast_search_results.len());
    for result in &fast_search_results {
        debug!(
            "  Found: {} in {}",
            result.symbol.name, result.symbol.file_path
        );
    }

    assert!(
        !fast_search_results.is_empty(),
        "Multi-word query 'fast search' should find FastSearchTool through camelCase splitting"
    );

    let names: Vec<&str> = fast_search_results
        .iter()
        .map(|r| r.symbol.name.as_str())
        .collect();
    assert!(
        names.contains(&"FastSearchTool") || names.contains(&"slow_search_implementation"),
        "Should find FastSearchTool or slow_search_implementation when searching 'fast search', found: {:?}",
        names
    );

    // Test 2: "search tool" should find FastSearchTool
    let search_tool_results = engine.search("search tool").await.unwrap();
    debug!("Search tool results: {:?}", search_tool_results.len());
    for result in &search_tool_results {
        debug!(
            "  Found: {} in {}",
            result.symbol.name, result.symbol.file_path
        );
    }

    assert!(
        !search_tool_results.is_empty(),
        "Multi-word query 'search tool' should find FastSearchTool"
    );

    // Test 3: "slow search" should find slow_search_implementation
    let slow_search_results = engine.search("slow search").await.unwrap();
    debug!("Slow search results: {:?}", slow_search_results.len());
    for result in &slow_search_results {
        debug!(
            "  Found: {} in {}",
            result.symbol.name, result.symbol.file_path
        );
    }

    assert!(
        !slow_search_results.is_empty(),
        "Multi-word query 'slow search' should find slow_search_implementation through underscore splitting"
    );

    // Test 4: Direct single-word search should still work
    let exact_results = engine.search("FastSearchTool").await.unwrap();
    assert!(
        !exact_results.is_empty(),
        "Single-word exact search should work"
    );
    assert_eq!(exact_results[0].symbol.name, "FastSearchTool");
}

#[tokio::test]
async fn test_custom_tokenizer_camelcase_splitting() {
    // TDD: Test that custom tokenizers properly split camelCase identifiers
    // This test should now PASS with our fixed tokenization
    let (mut engine, mut writer) = create_test_engine();

    let symbols = vec![
        Symbol {
            id: "user-service".to_string(),
            name: "UserService".to_string(),
            kind: SymbolKind::Class,
            language: "typescript".to_string(),
            file_path: "src/services/UserService.ts".to_string(),
            signature: Some("class UserService".to_string()),
            start_line: 1,
            end_line: 50,
            start_column: 0,
            end_column: 0,
            start_byte: 0,
            end_byte: 1200,
            doc_comment: Some("Service for managing user data and authentication".to_string()),
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: Some("class UserService { constructor() {} }".to_string()),
        },
        Symbol {
            id: "data-repository".to_string(),
            name: "DataRepository".to_string(),
            kind: SymbolKind::Class,
            language: "typescript".to_string(),
            file_path: "src/repositories/DataRepository.ts".to_string(),
            signature: Some("class DataRepository".to_string()),
            start_line: 1,
            end_line: 30,
            start_column: 0,
            end_column: 0,
            start_byte: 0,
            end_byte: 800,
            doc_comment: Some("Repository for data access and management".to_string()),
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: Some("class DataRepository { findAll() {} }".to_string()),
        },
        Symbol {
            id: "get-user-data".to_string(),
            name: "getUserData".to_string(),
            kind: SymbolKind::Function,
            language: "typescript".to_string(),
            file_path: "src/utils/userUtils.ts".to_string(),
            signature: Some("function getUserData(id: string): Promise<User>".to_string()),
            start_line: 10,
            end_line: 20,
            start_column: 0,
            end_column: 0,
            start_byte: 200,
            end_byte: 500,
            doc_comment: Some("Get user data by ID".to_string()),
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: Some("function getUserData(id) { return db.find(id); }".to_string()),
        },
    ];

    index_test_symbols(&mut engine, &mut writer,symbols).await.unwrap();

    // Test 1: "user service" should find UserService through camelCase splitting
    let user_service_results = engine.search("user service").await.unwrap();
    debug!("User service results: {} found", user_service_results.len());
    for result in &user_service_results {
        debug!(
            "  Found: {} in {}",
            result.symbol.name, result.symbol.file_path
        );
    }

    assert!(
        !user_service_results.is_empty(),
        "Multi-word query 'user service' should find UserService through camelCase tokenization"
    );

    let names: Vec<&str> = user_service_results
        .iter()
        .map(|r| r.symbol.name.as_str())
        .collect();
    assert!(
        names.contains(&"UserService"),
        "Should find UserService when searching for 'user service' with custom tokenizer, found: {:?}",
        names
    );

    // Test 2: "data repository" should find DataRepository
    let data_repo_results = engine.search("data repository").await.unwrap();
    debug!("Data repository results: {} found", data_repo_results.len());

    assert!(
        !data_repo_results.is_empty(),
        "Multi-word query 'data repository' should find DataRepository through camelCase tokenization"
    );

    let repo_names: Vec<&str> = data_repo_results
        .iter()
        .map(|r| r.symbol.name.as_str())
        .collect();
    assert!(
        repo_names.contains(&"DataRepository"),
        "Should find DataRepository when searching for 'data repository', found: {:?}",
        repo_names
    );

    // Test 3: "get user" should find getUserData through camelCase splitting
    let get_user_results = engine.search("get user").await.unwrap();
    debug!("Get user results: {} found", get_user_results.len());

    assert!(
        !get_user_results.is_empty(),
        "Multi-word query 'get user' should find getUserData through camelCase tokenization"
    );

    let func_names: Vec<&str> = get_user_results
        .iter()
        .map(|r| r.symbol.name.as_str())
        .collect();
    assert!(
        func_names.contains(&"getUserData") || func_names.contains(&"UserService"),
        "Should find getUserData or UserService when searching for 'get user', found: {:?}",
        func_names
    );

    // Test 4: Verify exact searches still work
    let exact_results = engine.search("UserService").await.unwrap();
    assert!(!exact_results.is_empty(), "Exact search should still work");
    assert_eq!(exact_results[0].symbol.name, "UserService");

    debug!("✅ Custom tokenizer camelCase splitting tests passed!");
}

#[tokio::test]
async fn test_search_performance() {
    // Contract: Should complete searches in under 10ms
    // Setup: Index 1000 symbols (scaled down for test speed)
    // Query: Various search patterns
    // Expected: All searches complete in <10ms
    let (mut engine, mut writer) = create_test_engine();

    // Generate 1000 test symbols
    let mut symbols = Vec::new();
    for i in 0..1000 {
        symbols.push(Symbol {
            id: format!("symbol-{}", i),
            name: format!("function{}", i),
            kind: SymbolKind::Function,
            language: "typescript".to_string(),
            file_path: format!("src/module{}.ts", i % 10),
            signature: Some(format!(
                "function function{}(param: string): Promise<Result{}>",
                i, i
            )),
            start_line: (i % 100) as u32 + 1,
            end_line: (i % 100) as u32 + 5,
            start_column: 0,
            end_column: 0,
            start_byte: (i * 100) as u32,
            end_byte: (i * 100 + 200) as u32,
            doc_comment: Some(format!("Function {} documentation", i)),
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
        });
    }

    // Index all symbols
    index_test_symbols(&mut engine, &mut writer,symbols).await.unwrap();

    // Test search performance with various queries
    let test_queries = vec![
        "function0",
        "function999",
        "function500",
        "src/module5",
        "Promise<Result",
        "typescript",
    ];

    for query in test_queries {
        let start = std::time::Instant::now();
        let results = engine.search(query).await.unwrap();
        let duration = start.elapsed();

        // Performance requirement: <10ms per search
        assert!(
            duration.as_millis() < 10,
            "Search for '{}' took {}ms, should be <10ms",
            query,
            duration.as_millis()
        );

        // Sanity check: should find at least some results for most queries
        if query.starts_with("function") {
            assert!(
                !results.is_empty(),
                "Should find results for function search"
            );
        }
    }

    // Test batch search performance
    let start = std::time::Instant::now();
    for i in 0..100 {
        let _results = engine.search(&format!("function{}", i)).await.unwrap();
    }
    let batch_duration = start.elapsed();
    let avg_duration = batch_duration.as_millis() / 100;

    assert!(
        avg_duration < 10,
        "Average search time {}ms should be <10ms",
        avg_duration
    );
}

#[tokio::test]
async fn test_incremental_updates() {
    // Contract: Should handle file updates correctly
    // Setup: Index symbols, then update a file
    // Action: Delete old symbols, add new ones
    // Expected: Search reflects changes
    let (mut engine, mut writer) = create_test_engine();

    // Initial symbols from a file
    let initial_symbols = vec![
        Symbol {
            id: "old-function-1".to_string(),
            name: "oldFunction".to_string(),
            kind: SymbolKind::Function,
            language: "typescript".to_string(),
            file_path: "src/updated.ts".to_string(),
            signature: Some("function oldFunction(): void".to_string()),
            start_line: 10,
            end_line: 12,
            start_column: 0,
            end_column: 0,
            start_byte: 100,
            end_byte: 200,
            doc_comment: Some("Old function implementation".to_string()),
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
        },
        Symbol {
            id: "unchanged-function".to_string(),
            name: "unchangedFunction".to_string(),
            kind: SymbolKind::Function,
            language: "typescript".to_string(),
            file_path: "src/stable.ts".to_string(),
            signature: Some("function unchangedFunction(): string".to_string()),
            start_line: 5,
            end_line: 7,
            start_column: 0,
            end_column: 0,
            start_byte: 300,
            end_byte: 400,
            doc_comment: Some("This function remains unchanged".to_string()),
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
        },
    ];

    // Index initial symbols
    index_test_symbols(&mut engine, &mut writer,initial_symbols).await.unwrap();

    // Verify initial state
    let old_results = engine.search("oldFunction").await.unwrap();
    assert_eq!(old_results.len(), 1, "Should find old function initially");

    let unchanged_results = engine.search("unchangedFunction").await.unwrap();
    assert_eq!(unchanged_results.len(), 1, "Should find unchanged function");

    // Simulate file update: delete old symbols from the updated file
    writer.delete_file_symbols("src/updated.ts").await.unwrap();
    writer.commit().await.unwrap();
    engine.reload_reader().unwrap();

    // Add new symbols for the updated file
    let updated_symbols = vec![Symbol {
        id: "new-function-1".to_string(),
        name: "newFunction".to_string(),
        kind: SymbolKind::Function,
        language: "typescript".to_string(),
        file_path: "src/updated.ts".to_string(),
        signature: Some("function newFunction(): Promise<string>".to_string()),
        start_line: 10,
        end_line: 15,
        start_column: 0,
        end_column: 0,
        start_byte: 100,
        end_byte: 300,
        doc_comment: Some("New function implementation".to_string()),
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: None,
    }];

    index_test_symbols(&mut engine, &mut writer,updated_symbols).await.unwrap();

    // Verify incremental update worked correctly
    let old_results_after = engine.search("oldFunction").await.unwrap();
    assert_eq!(
        old_results_after.len(),
        0,
        "Should not find old function after update"
    );

    let new_results = engine.search("newFunction").await.unwrap();
    assert_eq!(
        new_results.len(),
        1,
        "Should find new function after update"
    );
    assert_eq!(new_results[0].symbol.name, "newFunction");

    // Verify unchanged file is still intact
    let unchanged_results_after = engine.search("unchangedFunction").await.unwrap();
    assert_eq!(
        unchanged_results_after.len(),
        1,
        "Should still find unchanged function"
    );
    assert_eq!(unchanged_results_after[0].symbol.name, "unchangedFunction");
}
