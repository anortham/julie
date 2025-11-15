// Tests extracted from src/embeddings/mod.rs
// These were previously inline tests that have been moved to follow project standards

use crate::database::SymbolDatabase;
use crate::embeddings::{CodeContext, EmbeddingEngine, cosine_similarity};
use crate::extractors::base::{Symbol, SymbolKind, Visibility};
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

// Helper: Create a test database for embedding tests
fn create_test_db() -> Arc<Mutex<SymbolDatabase>> {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = SymbolDatabase::new(&db_path).unwrap();
    Arc::new(Mutex::new(db))
}

#[cfg_attr(
    not(feature = "network_models"),
    ignore = "requires downloadable embedding model"
)]
#[tokio::test]
async fn test_embedding_engine_creation() {
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = temp_dir.path().to_path_buf();
    let db = create_test_db();

    // Test creating with different models
    let engine = EmbeddingEngine::new("bge-small", cache_dir, db)
        .await
        .unwrap();
    assert_eq!(engine.dimensions(), 384);
    assert_eq!(engine.model_name(), "bge-small");
}

#[cfg_attr(
    not(feature = "network_models"),
    ignore = "requires downloadable embedding model"
)]
#[tokio::test]
async fn test_symbol_embedding_generation() {
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = temp_dir.path().to_path_buf();
    let db = create_test_db();

    let mut engine = EmbeddingEngine::new("bge-small", cache_dir, db)
        .await
        .unwrap();

    // Create a test symbol
    let symbol = Symbol {
        id: "test-id".to_string(),
        name: "getUserData".to_string(),
        kind: SymbolKind::Function,
        language: "typescript".to_string(),
        file_path: "/test/user.ts".to_string(),
        start_line: 10,
        start_column: 0,
        end_line: 15,
        end_column: 1,
        start_byte: 200,
        end_byte: 350,
        signature: Some("function getUserData(): Promise<User>".to_string()),
        doc_comment: None,
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: None,
        content_type: None,
    };

    let context = CodeContext::from_symbol(&symbol);
    let embedding = engine.embed_symbol(&symbol, &context).unwrap();

    // Should generate embedding with correct dimensions
    assert_eq!(embedding.len(), 384);

    // Should be normalized (roughly)
    let magnitude: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
    assert!(magnitude > 0.0);
}

#[cfg_attr(
    not(feature = "network_models"),
    ignore = "requires downloadable embedding model"
)]
#[tokio::test]
async fn test_text_embedding_generation() {
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = temp_dir.path().to_path_buf();
    let db = create_test_db();

    let mut engine = EmbeddingEngine::new("bge-small", cache_dir, db)
        .await
        .unwrap();

    let embedding1 = engine.embed_text("function getUserData").unwrap();
    let embedding2 = engine.embed_text("function getUserData").unwrap();
    let embedding3 = engine.embed_text("class UserRepository").unwrap();

    // Same text should produce identical embeddings
    assert_eq!(embedding1, embedding2);

    // Different text should produce different embeddings
    assert_ne!(embedding1, embedding3);

    // Should have correct dimensions
    assert_eq!(embedding1.len(), 384);
}

#[cfg_attr(
    not(feature = "network_models"),
    ignore = "requires downloadable embedding model"
)]
#[tokio::test]
async fn test_cross_language_similarity() {
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = temp_dir.path().to_path_buf();
    let db = create_test_db();

    let mut engine = EmbeddingEngine::new("bge-small", cache_dir, db)
        .await
        .unwrap();

    // Test similar concepts in different languages
    let ts_embedding = engine
        .embed_text("interface User { id: string; name: string; }")
        .unwrap();
    let cs_embedding = engine
        .embed_text("class User { public string Id; public string Name; }")
        .unwrap();
    let sql_embedding = engine
        .embed_text("CREATE TABLE users (id VARCHAR, name VARCHAR)")
        .unwrap();

    // Should have high similarity for same concept
    let ts_cs_similarity = cosine_similarity(&ts_embedding, &cs_embedding);
    let ts_sql_similarity = cosine_similarity(&ts_embedding, &sql_embedding);

    // Should be reasonably similar (>0.5) for same concept across languages
    assert!(
        ts_cs_similarity > 0.5,
        "TypeScript and C# similarity: {}",
        ts_cs_similarity
    );
    assert!(
        ts_sql_similarity > 0.3,
        "TypeScript and SQL similarity: {}",
        ts_sql_similarity
    );
}

#[test]
fn test_cosine_similarity() {
    let vec_a = vec![1.0, 0.0, 0.0];
    let vec_b = vec![1.0, 0.0, 0.0];
    let vec_c = vec![0.0, 1.0, 0.0];

    // Identical vectors should have similarity of 1.0
    assert!((cosine_similarity(&vec_a, &vec_b) - 1.0).abs() < f32::EPSILON);

    // Orthogonal vectors should have similarity of 0.0
    assert!((cosine_similarity(&vec_a, &vec_c) - 0.0).abs() < f32::EPSILON);

    // Different lengths should return 0.0
    let vec_d = vec![1.0, 0.0];
    assert_eq!(cosine_similarity(&vec_a, &vec_d), 0.0);
}

#[test]
fn test_code_context_creation() {
    let context = CodeContext::new();
    assert!(context.parent_symbol.is_none());
    assert!(context.surrounding_code.is_none());
    assert!(context.file_context.is_none());

    // Test context from symbol
    let symbol = Symbol {
        id: "test".to_string(),
        name: "test".to_string(),
        kind: SymbolKind::Function,
        language: "rust".to_string(),
        file_path: "/test.rs".to_string(),
        start_line: 1,
        start_column: 1,
        end_line: 1,
        end_column: 1,
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
    };

    let context = CodeContext::from_symbol(&symbol);
    assert_eq!(context.file_context, Some("/test.rs".to_string()));
}

#[cfg_attr(
    not(feature = "network_models"),
    ignore = "requires downloadable embedding model"
)]
#[tokio::test]
async fn test_build_embedding_text() {
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = temp_dir.path().to_path_buf();
    let db = create_test_db();

    let engine = EmbeddingEngine::new("bge-small", cache_dir, db)
        .await
        .unwrap();

    let symbol = Symbol {
        id: "test".to_string(),
        name: "getUserData".to_string(),
        kind: SymbolKind::Function,
        language: "typescript".to_string(),
        file_path: "/src/services/user.ts".to_string(),
        start_line: 10,
        start_column: 0,
        end_line: 15,
        end_column: 1,
        start_byte: 200,
        end_byte: 350,
        signature: Some("function getUserData(): Promise<User>".to_string()),
        doc_comment: None,
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: Some("// Fetch user data from API".to_string()), // Fixed: use symbol.code_context
        content_type: None,
    };

    let context = CodeContext::from_symbol(&symbol);

    let embedding_text = engine.build_embedding_text(&symbol);

    // Should include all the important information
    assert!(embedding_text.contains("getUserData"));
    assert!(embedding_text.contains("function")); // SymbolKind::Function.to_string() returns "function" lowercase
    assert!(embedding_text.contains("function getUserData(): Promise<User>"));
    // Note: file_path is NOT included in embeddings (it's metadata, not semantic content)
    assert!(embedding_text.contains("Fetch user data from API")); // Now comes from symbol.code_context
}

#[cfg_attr(
    not(feature = "network_models"),
    ignore = "requires downloadable embedding model"
)]
#[tokio::test]
async fn test_build_embedding_text_includes_code_context() {
    // RED TEST: This will FAIL initially because build_embedding_text doesn't include code_context
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = temp_dir.path().to_path_buf();
    let db = create_test_db();

    let engine = EmbeddingEngine::new("bge-small", cache_dir, db)
        .await
        .unwrap();

    // Create a symbol WITH code_context populated (this is what extractors do)
    let code_context_lines = vec![
        "  // Validate user permissions",
        "  if (!hasPermission(user)) {",
        "    throw new Error('Unauthorized');",
        "  }",
        "  return await db.users.findById(userId);",
    ]
    .join("\n");

    let symbol = Symbol {
        id: "test".to_string(),
        name: "getUserData".to_string(),
        kind: SymbolKind::Function,
        language: "typescript".to_string(),
        file_path: "/src/services/user.ts".to_string(),
        start_line: 10,
        start_column: 0,
        end_line: 15,
        end_column: 1,
        start_byte: 200,
        end_byte: 350,
        signature: Some("function getUserData(userId: string): Promise<User>".to_string()),
        doc_comment: Some("/// Fetches user data from database".to_string()),
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: Some(code_context_lines.clone()), // ← This is populated by extractors!
        content_type: None,
    };

    let context = CodeContext::from_symbol(&symbol);

    let embedding_text = engine.build_embedding_text(&symbol);

    // Should include all the important information
    assert!(
        embedding_text.contains("getUserData"),
        "Embedding text should contain function name"
    );
    assert!(
        embedding_text.contains("function"),
        "Embedding text should contain symbol kind"
    );
    assert!(
        embedding_text.contains("getUserData(userId: string)"),
        "Embedding text should contain signature"
    );
    assert!(
        embedding_text.contains("Fetches user data from database"),
        "Embedding text should contain doc comment"
    );

    // THE KEY ASSERTION: code_context should be included for richer semantic understanding
    assert!(
        embedding_text.contains("hasPermission"),
        "Embedding text should contain code_context for semantic search - found actual code usage patterns"
    );
    assert!(
        embedding_text.contains("Unauthorized"),
        "Embedding text should include error messages from code_context"
    );
    assert!(
        embedding_text.contains("db.users.findById"),
        "Embedding text should include database calls from code_context"
    );
}

/// Test that embed_symbols_batch respects batch size limits
/// This is CRITICAL to prevent OOM errors when indexing files with 40k+ symbols
#[cfg_attr(
    not(feature = "network_models"),
    ignore = "requires downloadable embedding model"
)]
#[tokio::test]
async fn test_embed_symbols_batch_respects_batch_size_limits() {
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = temp_dir.path().to_path_buf();
    let db = create_test_db();

    let mut engine = EmbeddingEngine::new("bge-small", cache_dir, db)
        .await
        .unwrap();

    // Get the optimal batch size for this system
    let batch_size = engine.calculate_optimal_batch_size();

    // Create MORE symbols than the batch size to test batching logic
    // Using 3x batch size to ensure we test multiple batches
    let num_symbols = batch_size * 3;

    let symbols: Vec<Symbol> = (0..num_symbols)
        .map(|i| Symbol {
            id: format!("symbol-{}", i),
            name: format!("function{}", i),
            kind: SymbolKind::Function,
            language: "typescript".to_string(),
            file_path: format!("/test/file{}.ts", i / 10),
            start_line: i as u32 * 10,
            start_column: 0,
            end_line: i as u32 * 10 + 5,
            end_column: 1,
            start_byte: i as u32 * 200,
            end_byte: i as u32 * 200 + 100,
            signature: Some(format!("function function{}()", i)),
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
            content_type: None,
        })
        .collect();

    // Call embed_symbols_batch with more symbols than batch size
    // This should NOT try to allocate 23GB+ of memory like the bug report
    let results = engine.embed_symbols_batch(&symbols).unwrap();

    // Verify all symbols were embedded
    assert_eq!(
        results.len(),
        num_symbols,
        "Should embed all {} symbols even when exceeding batch size of {}",
        num_symbols,
        batch_size
    );

    // Verify embeddings have correct dimensions
    for (id, embedding) in &results {
        assert_eq!(
            embedding.len(),
            384,
            "Embedding for {} should have 384 dimensions",
            id
        );
    }
}

// ============================================================================
// MEMORY EMBEDDING TESTS - Phase 2: Custom RAG Pipeline for .memories/
// ============================================================================

#[test]
fn test_memory_embedding_text_checkpoint() {
    // Create a memory symbol simulating the "description" key from a checkpoint file
    let symbol = Symbol {
        id: "test_mem_checkpoint".to_string(),
        name: "description".to_string(), // Memory files have "description" key
        kind: SymbolKind::Variable,
        language: "json".to_string(),
        file_path: ".memories/2025-11-10/020018_b4a2.json".to_string(),
        start_line: 5,
        start_column: 2,
        end_line: 5,
        end_column: 200,
        start_byte: 100,
        end_byte: 300,
        signature: None,
        doc_comment: None,
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        // Simulated code_context from JSON file (what tree-sitter extracts)
        code_context: Some(
            r#"      2:   "id": "milestone_69114732_999aff",
      3:   "timestamp": 1762740018,
      4:   "type": "checkpoint",
  ➤   5:   "description": "Fixed auth bug by adding mutex to prevent race condition",
      6:   "tags": [
      7:     "bug""#
                .to_string(),
        ),
        content_type: None,
    };

    let temp_dir = TempDir::new().unwrap();
    let db = create_test_db();

    // Note: This test doesn't need async/network - just testing text building logic
    let engine = std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            EmbeddingEngine::new("bge-small", temp_dir.path().to_path_buf(), db).await
        })
    })
    .join()
    .unwrap()
    .unwrap();

    let embedding_text = engine.build_embedding_text(&symbol);

    // Should produce focused embedding: "checkpoint: {description}"
    assert!(
        embedding_text.contains("checkpoint:"),
        "Should prefix with memory type. Got: '{}'",
        embedding_text
    );
    assert!(
        embedding_text.contains("Fixed auth bug"),
        "Should include description text. Got: '{}'",
        embedding_text
    );
    assert!(
        embedding_text.contains("race condition"),
        "Should include full description. Got: '{}'",
        embedding_text
    );

    // Should NOT contain JSON structure noise (metadata fields)
    assert!(
        !embedding_text.contains("timestamp"),
        "Should not include timestamp field"
    );
    // Note: tags ARE now included as searchable terms (not as JSON structure)
}

#[test]
fn test_memory_embedding_text_decision() {
    let symbol = Symbol {
        id: "test_mem_decision".to_string(),
        name: "description".to_string(),
        kind: SymbolKind::Variable,
        language: "json".to_string(),
        file_path: ".memories/2025-11-11/143022_abc123.json".to_string(),
        start_line: 5,
        start_column: 2,
        end_line: 5,
        end_column: 150,
        start_byte: 100,
        end_byte: 250,
        signature: None,
        doc_comment: None,
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: Some(
            r#"      2:   "id": "dec_1736423000_xyz789",
      3:   "timestamp": 1736423000,
      4:   "type": "decision",
  ➤   5:   "description": "Chose SQLite over PostgreSQL for zero-dependency deployment",
      6:   "alternatives": ["PostgreSQL", "MySQL"]"#
                .to_string(),
        ),
        content_type: None,
    };

    let temp_dir = TempDir::new().unwrap();
    let db = create_test_db();

    let engine = std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            EmbeddingEngine::new("bge-small", temp_dir.path().to_path_buf(), db).await
        })
    })
    .join()
    .unwrap()
    .unwrap();

    let embedding_text = engine.build_embedding_text(&symbol);

    // Should produce: "decision: Chose SQLite over PostgreSQL..."
    assert!(
        embedding_text.starts_with("decision:"),
        "Should prefix with 'decision:'. Got: '{}'",
        embedding_text
    );
    assert!(
        embedding_text.contains("SQLite"),
        "Should include decision content. Got: '{}'",
        embedding_text
    );
}

#[test]
fn test_memory_embedding_skips_non_description_symbols() {
    // Test that "id", "timestamp", "type", "tags" symbols get EMPTY embedding text
    let non_description_symbols = vec!["id", "timestamp", "type", "tags", "git"];

    let temp_dir = TempDir::new().unwrap();
    let db = create_test_db();

    let engine = std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            EmbeddingEngine::new("bge-small", temp_dir.path().to_path_buf(), db).await
        })
    })
    .join()
    .unwrap()
    .unwrap();

    for symbol_name in non_description_symbols {
        let symbol = Symbol {
            id: format!("test_mem_{}", symbol_name),
            name: symbol_name.to_string(),
            kind: SymbolKind::Variable,
            language: "json".to_string(),
            file_path: ".memories/2025-11-11/test.json".to_string(),
            start_line: 2,
            start_column: 2,
            end_line: 2,
            end_column: 50,
            start_byte: 50,
            end_byte: 100,
            signature: None,
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: Some(format!(r#"      2:   "{}": "some_value""#, symbol_name)),
            content_type: None,
        };

        let embedding_text = engine.build_embedding_text(&symbol);

        assert_eq!(
            embedding_text, "",
            "Symbol '{}' should produce empty embedding text (skipped)",
            symbol_name
        );
    }
}

#[test]
fn test_memory_embedding_excludes_mutable_plans() {
    // Phase 3 feature: .memories/plans/ should NOT use custom memory pipeline
    // They should use standard JSON embedding instead
    let symbol = Symbol {
        id: "test_plan".to_string(),
        name: "description".to_string(),
        kind: SymbolKind::Variable,
        language: "json".to_string(),
        file_path: ".memories/plans/plan_test.json".to_string(), // Plans are excluded!
        start_line: 5,
        start_column: 2,
        end_line: 5,
        end_column: 100,
        start_byte: 100,
        end_byte: 200,
        signature: None,
        doc_comment: None,
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: Some(r#"      5:   "description": "Test plan description""#.to_string()),
        content_type: None,
    };

    let temp_dir = TempDir::new().unwrap();
    let db = create_test_db();

    let engine = std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            EmbeddingEngine::new("bge-small", temp_dir.path().to_path_buf(), db).await
        })
    })
    .join()
    .unwrap()
    .unwrap();

    let embedding_text = engine.build_embedding_text(&symbol);

    // Plans should use standard JSON embedding (name + kind + signature + doc)
    // NOT the custom memory pipeline
    assert_eq!(
        embedding_text,
        "description variable", // Standard JSON symbol embedding
        "Plans should NOT use custom memory embedding pipeline"
    );
}

#[test]
fn test_memory_embedding_handles_missing_type_field() {
    // Graceful degradation: if "type" field missing, default to "checkpoint"
    let symbol = Symbol {
        id: "test_mem_no_type".to_string(),
        name: "description".to_string(),
        kind: SymbolKind::Variable,
        language: "json".to_string(),
        file_path: ".memories/2025-11-11/broken.json".to_string(),
        start_line: 3,
        start_column: 2,
        end_line: 3,
        end_column: 100,
        start_byte: 50,
        end_byte: 150,
        signature: None,
        doc_comment: None,
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        // Missing "type" field!
        code_context: Some(
            r#"      2:   "id": "test_123",
  ➤   3:   "description": "Some memory without type field",
      4:   "timestamp": 123456"#
                .to_string(),
        ),
        content_type: None,
    };

    let temp_dir = TempDir::new().unwrap();
    let db = create_test_db();

    let engine = std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            EmbeddingEngine::new("bge-small", temp_dir.path().to_path_buf(), db).await
        })
    })
    .join()
    .unwrap()
    .unwrap();

    let embedding_text = engine.build_embedding_text(&symbol);

    // Should default to "checkpoint" prefix
    assert!(
        embedding_text.starts_with("checkpoint:"),
        "Should default to 'checkpoint:' when type field missing. Got: '{}'",
        embedding_text
    );
    assert!(
        embedding_text.contains("Some memory without type field"),
        "Should still extract description. Got: '{}'",
        embedding_text
    );
}

#[test]
fn test_standard_code_symbols_unchanged() {
    // Verify that normal code symbols (non-.memories/ files) still use standard embedding
    let symbol = Symbol {
        id: "test_code".to_string(),
        name: "getUserData".to_string(),
        kind: SymbolKind::Function,
        language: "typescript".to_string(),
        file_path: "src/services/user.ts".to_string(), // Regular code file
        start_line: 10,
        start_column: 0,
        end_line: 15,
        end_column: 1,
        start_byte: 200,
        end_byte: 350,
        signature: Some("function getUserData(): Promise<User>".to_string()),
        doc_comment: Some("Fetches user data from API".to_string()),
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: Some("const user = await fetchUser();".to_string()),
        content_type: None,
    };

    let temp_dir = TempDir::new().unwrap();
    let db = create_test_db();

    let engine = std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            EmbeddingEngine::new("bge-small", temp_dir.path().to_path_buf(), db).await
        })
    })
    .join()
    .unwrap()
    .unwrap();

    let embedding_text = engine.build_embedding_text(&symbol);

    // Should use standard embedding: name + kind + signature + doc_comment
    assert!(
        embedding_text.contains("getUserData"),
        "Should include name"
    );
    assert!(embedding_text.contains("function"), "Should include kind");
    assert!(
        embedding_text.contains("Promise<User>"),
        "Should include signature"
    );
    assert!(
        embedding_text.contains("Fetches user data"),
        "Should include doc comment"
    );

    // Should NOT contain code_context (removed in Phase 1)
    assert!(
        !embedding_text.contains("fetchUser"),
        "Should NOT include code_context (Phase 1 optimization)"
    );
}

#[test]
fn test_memory_embedding_handles_escaped_quotes() {
    // CRITICAL: Test that escaped quotes in descriptions are handled correctly
    // This validates the serde_json streaming deserializer fix
    let symbol = Symbol {
        id: "test_mem_escaped".to_string(),
        name: "description".to_string(),
        kind: SymbolKind::Variable,
        language: "json".to_string(),
        file_path: ".memories/2025-11-11/escaped.json".to_string(),
        start_line: 5,
        start_column: 2,
        end_line: 5,
        end_column: 150,
        start_byte: 100,
        end_byte: 250,
        signature: None,
        doc_comment: None,
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        // Description with escaped quotes, backslashes, and unicode
        code_context: Some(
            r#"      2:   "id": "test_escaped_123",
      3:   "timestamp": 1736423000,
      4:   "type": "checkpoint",
  ➤   5:   "description": "Fixed \"auth\" bug in C:\\Users\\path with unicode \u0041",
      6:   "tags": ["bug"]"#
                .to_string(),
        ),
        content_type: None,
    };

    let temp_dir = TempDir::new().unwrap();
    let db = create_test_db();

    let engine = std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            EmbeddingEngine::new("bge-small", temp_dir.path().to_path_buf(), db).await
        })
    })
    .join()
    .unwrap()
    .unwrap();

    let embedding_text = engine.build_embedding_text(&symbol);

    // Should properly parse escaped quotes as actual quote characters
    assert!(
        embedding_text.contains("Fixed \"auth\" bug"),
        "Should handle escaped quotes. Got: '{}'",
        embedding_text
    );

    // Should handle escaped backslashes
    assert!(
        embedding_text.contains("C:\\Users\\path"),
        "Should handle escaped backslashes. Got: '{}'",
        embedding_text
    );

    // Should handle unicode escapes (serde_json decodes \u0041 to 'A')
    assert!(
        embedding_text.contains("unicode A") || embedding_text.contains("unicode \u{0041}"),
        "Should handle unicode escapes. Got: '{}'",
        embedding_text
    );

    // Should still have the type prefix
    assert!(
        embedding_text.starts_with("checkpoint:"),
        "Should still have type prefix. Got: '{}'",
        embedding_text
    );
}

// ============================================================================
// Enhanced Memory Embeddings Tests - Tags and File Terms (NEW)
// ============================================================================

#[test]
fn test_memory_embedding_includes_tags() {
    // CONTRACT: Memory embeddings should include tags for better searchability
    // Format: "{type}: {description} | tags: {tag1} {tag2} {tag3}"

    let symbol = Symbol {
        id: "test_mem_with_tags".to_string(),
        name: "description".to_string(),
        kind: SymbolKind::Variable,
        language: "json".to_string(),
        file_path: ".memories/2025-11-13/test.json".to_string(),
        start_line: 5,
        start_column: 2,
        end_line: 5,
        end_column: 200,
        start_byte: 100,
        end_byte: 300,
        signature: None,
        doc_comment: None,
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        // Full memory JSON with tags array
        code_context: Some(
            r#"      2:   "id": "checkpoint_abc123",
      3:   "timestamp": 1762971017,
      4:   "type": "checkpoint",
  ➤   5:   "description": "Added 100KB file size limit for symbol extraction",
      6:   "tags": [
      7:     "performance",
      8:     "file-size-limit",
      9:     "indexing"
     10:   ]"#
                .to_string(),
        ),
        content_type: None,
    };

    let temp_dir = TempDir::new().unwrap();
    let db = create_test_db();

    let engine = std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            EmbeddingEngine::new("bge-small", temp_dir.path().to_path_buf(), db).await
        })
    })
    .join()
    .unwrap()
    .unwrap();

    let embedding_text = engine.build_embedding_text(&symbol);

    // Should include description
    assert!(
        embedding_text.contains("Added 100KB file size limit"),
        "Should include description. Got: '{}'",
        embedding_text
    );

    // Should include tags section
    assert!(
        embedding_text.contains("tags:") || embedding_text.contains("| tags"),
        "Should have tags section. Got: '{}'",
        embedding_text
    );

    // Should include individual tag terms (searchable)
    assert!(
        embedding_text.contains("performance"),
        "Should include 'performance' tag. Got: '{}'",
        embedding_text
    );
    assert!(
        embedding_text.contains("file-size-limit"),
        "Should include 'file-size-limit' tag. Got: '{}'",
        embedding_text
    );
    assert!(
        embedding_text.contains("indexing"),
        "Should include 'indexing' tag. Got: '{}'",
        embedding_text
    );
}

#[test]
fn test_memory_embedding_includes_file_terms() {
    // CONTRACT: Memory embeddings should extract semantic terms from files_changed
    // Format: "{type}: {description} | tags: {tags} | files: {extracted_terms}"

    let symbol = Symbol {
        id: "test_mem_with_files".to_string(),
        name: "description".to_string(),
        kind: SymbolKind::Variable,
        language: "json".to_string(),
        file_path: ".memories/2025-11-13/test.json".to_string(),
        start_line: 5,
        start_column: 2,
        end_line: 5,
        end_column: 200,
        start_byte: 100,
        end_byte: 300,
        signature: None,
        doc_comment: None,
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        // Full memory JSON with git.files_changed array
        code_context: Some(
            r#"      2:   "id": "checkpoint_def456",
      3:   "type": "checkpoint",
      4:   "git": {
      5:     "files_changed": [
      6:       "src/embeddings/mod.rs",
      7:       "src/embeddings/ort_model.rs",
      8:       "src/tools/workspace/indexing/extractor.rs"
      9:     ]
     10:   },
  ➤  11:   "description": "Optimized embedding generation performance",
     12:   "tags": ["performance"]"#
                .to_string(),
        ),
        content_type: None,
    };

    let temp_dir = TempDir::new().unwrap();
    let db = create_test_db();

    let engine = std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            EmbeddingEngine::new("bge-small", temp_dir.path().to_path_buf(), db).await
        })
    })
    .join()
    .unwrap()
    .unwrap();

    let embedding_text = engine.build_embedding_text(&symbol);

    // Should include files section
    assert!(
        embedding_text.contains("files:") || embedding_text.contains("| files"),
        "Should have files section. Got: '{}'",
        embedding_text
    );

    // Should include extracted file terms (not full paths!)
    assert!(
        embedding_text.contains("embeddings"),
        "Should extract 'embeddings' term. Got: '{}'",
        embedding_text
    );
    assert!(
        embedding_text.contains("ort_model"),
        "Should extract 'ort_model' term. Got: '{}'",
        embedding_text
    );
    assert!(
        embedding_text.contains("workspace"),
        "Should extract 'workspace' term. Got: '{}'",
        embedding_text
    );
    assert!(
        embedding_text.contains("indexing"),
        "Should extract 'indexing' term. Got: '{}'",
        embedding_text
    );
    assert!(
        embedding_text.contains("extractor"),
        "Should extract 'extractor' term. Got: '{}'",
        embedding_text
    );

    // Should NOT include noise terms
    assert!(
        !embedding_text.contains("src/") && !embedding_text.contains("mod.rs"),
        "Should NOT include path noise like 'src/' or 'mod.rs'. Got: '{}'",
        embedding_text
    );
}

#[test]
fn test_memory_embedding_full_format_with_tags_and_files() {
    // CONTRACT: Complete format test with tags AND files
    // Expected: "{type}: {description} | tags: {tags} | files: {file_terms}"

    let symbol = Symbol {
        id: "test_mem_complete".to_string(),
        name: "description".to_string(),
        kind: SymbolKind::Variable,
        language: "json".to_string(),
        file_path: ".memories/2025-11-13/complete_test.json".to_string(),
        start_line: 5,
        start_column: 2,
        end_line: 5,
        end_column: 200,
        start_byte: 100,
        end_byte: 300,
        signature: None,
        doc_comment: None,
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: Some(
            r#"      2:   "id": "decision_789",
      3:   "type": "decision",
      4:   "git": {
      5:     "files_changed": [
      6:       "src/database/schema.rs",
      7:       "src/database/symbols/mod.rs"
      8:     ]
      9:   },
  ➤  10:   "description": "Chose SQLite FTS5 for search performance",
     11:   "tags": ["architecture", "database", "performance"]"#
                .to_string(),
        ),
        content_type: None,
    };

    let temp_dir = TempDir::new().unwrap();
    let db = create_test_db();

    let engine = std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            EmbeddingEngine::new("bge-small", temp_dir.path().to_path_buf(), db).await
        })
    })
    .join()
    .unwrap()
    .unwrap();

    let embedding_text = engine.build_embedding_text(&symbol);

    // Verify complete format structure
    assert!(
        embedding_text.starts_with("decision:"),
        "Should start with type prefix. Got: '{}'",
        embedding_text
    );

    assert!(
        embedding_text.contains("Chose SQLite FTS5"),
        "Should include description. Got: '{}'",
        embedding_text
    );

    assert!(
        embedding_text.contains("tags:"),
        "Should have tags section. Got: '{}'",
        embedding_text
    );

    assert!(
        embedding_text.contains("architecture")
            && embedding_text.contains("database")
            && embedding_text.contains("performance"),
        "Should include all tags. Got: '{}'",
        embedding_text
    );

    assert!(
        embedding_text.contains("files:"),
        "Should have files section. Got: '{}'",
        embedding_text
    );

    assert!(
        embedding_text.contains("schema") && embedding_text.contains("symbols"),
        "Should include extracted file terms. Got: '{}'",
        embedding_text
    );

    // Verify order: description comes before tags/files
    let desc_pos = embedding_text.find("SQLite").unwrap();
    let tags_pos = embedding_text.find("tags:").unwrap();
    assert!(
        desc_pos < tags_pos,
        "Description should come before tags. Got: '{}'",
        embedding_text
    );
}

#[test]
fn test_memory_embedding_handles_missing_tags() {
    // CONTRACT: Should gracefully handle memories without tags field

    let symbol = Symbol {
        id: "test_mem_no_tags".to_string(),
        name: "description".to_string(),
        kind: SymbolKind::Variable,
        language: "json".to_string(),
        file_path: ".memories/2025-11-13/no_tags.json".to_string(),
        start_line: 3,
        start_column: 2,
        end_line: 3,
        end_column: 100,
        start_byte: 50,
        end_byte: 150,
        signature: None,
        doc_comment: None,
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: Some(
            r#"      2:   "type": "checkpoint",
  ➤   3:   "description": "Quick fix for bug""#
                .to_string(),
        ),
        content_type: None,
    };

    let temp_dir = TempDir::new().unwrap();
    let db = create_test_db();

    let engine = std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            EmbeddingEngine::new("bge-small", temp_dir.path().to_path_buf(), db).await
        })
    })
    .join()
    .unwrap()
    .unwrap();

    let embedding_text = engine.build_embedding_text(&symbol);

    // Should still work without tags
    assert!(
        embedding_text.contains("Quick fix for bug"),
        "Should include description even without tags. Got: '{}'",
        embedding_text
    );

    // Should not have empty tags section
    assert!(
        !embedding_text.contains("tags:  |") && !embedding_text.contains("tags: |"),
        "Should not have empty tags section. Got: '{}'",
        embedding_text
    );
}

// ============================================================================
// Markdown Embeddings Tests - Skip Empty Headings (NEW)
// ============================================================================

#[test]
fn test_markdown_heading_with_content_is_embedded() {
    // CONTRACT: Markdown headings with doc_comment should be embedded normally
    let symbol = Symbol {
        id: "test_md_with_content".to_string(),
        name: "Quick Start".to_string(),
        kind: SymbolKind::Module,
        language: "markdown".to_string(),
        file_path: "docs/README.md".to_string(),
        start_line: 5,
        start_column: 0,
        end_line: 10,
        end_column: 0,
        start_byte: 100,
        end_byte: 500,
        signature: None,
        doc_comment: Some(
            "Follow these steps to get started with Julie. First, install the dependencies..."
                .to_string(),
        ),
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: None,
        content_type: Some("documentation".to_string()),
    };

    let temp_dir = TempDir::new().unwrap();
    let db = create_test_db();

    let engine = std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            EmbeddingEngine::new("bge-small", temp_dir.path().to_path_buf(), db).await
        })
    })
    .join()
    .unwrap()
    .unwrap();

    let embedding_text = engine.build_embedding_text(&symbol);

    // Should include heading name
    assert!(
        embedding_text.contains("Quick Start"),
        "Should include heading name. Got: '{}'",
        embedding_text
    );

    // Should include documentation content
    assert!(
        embedding_text.contains("Follow these steps"),
        "Should include doc content. Got: '{}'",
        embedding_text
    );

    // Should NOT be empty
    assert!(
        !embedding_text.is_empty(),
        "Should not be empty for markdown with content"
    );
}

#[test]
fn test_markdown_empty_heading_is_skipped() {
    // CONTRACT: Markdown headings WITHOUT doc_comment should be skipped (return empty string)
    // This is consistent with memory JSON optimization where we skip metadata symbols
    let symbol = Symbol {
        id: "test_md_empty".to_string(),
        name: "Core Search Tools".to_string(),
        kind: SymbolKind::Module,
        language: "markdown".to_string(),
        file_path: "docs/README.md".to_string(),
        start_line: 15,
        start_column: 0,
        end_line: 16,
        end_column: 0,
        start_byte: 600,
        end_byte: 650,
        signature: None,
        doc_comment: None, // No content under this heading
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: None,
        content_type: Some("documentation".to_string()),
    };

    let temp_dir = TempDir::new().unwrap();
    let db = create_test_db();

    let engine = std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            EmbeddingEngine::new("bge-small", temp_dir.path().to_path_buf(), db).await
        })
    })
    .join()
    .unwrap()
    .unwrap();

    let embedding_text = engine.build_embedding_text(&symbol);

    // Should return empty string to skip embedding
    assert!(
        embedding_text.is_empty(),
        "Should return empty string for markdown heading without content. Got: '{}'",
        embedding_text
    );
}

#[test]
fn test_markdown_empty_string_doc_comment_is_skipped() {
    // CONTRACT: Empty string doc_comment should also be skipped
    let symbol = Symbol {
        id: "test_md_empty_string".to_string(),
        name: "Implementation Details".to_string(),
        kind: SymbolKind::Module,
        language: "markdown".to_string(),
        file_path: "docs/ARCHITECTURE.md".to_string(),
        start_line: 20,
        start_column: 0,
        end_line: 21,
        end_column: 0,
        start_byte: 800,
        end_byte: 850,
        signature: None,
        doc_comment: Some("".to_string()), // Empty string
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: None,
        content_type: Some("documentation".to_string()),
    };

    let temp_dir = TempDir::new().unwrap();
    let db = create_test_db();

    let engine = std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            EmbeddingEngine::new("bge-small", temp_dir.path().to_path_buf(), db).await
        })
    })
    .join()
    .unwrap()
    .unwrap();

    let embedding_text = engine.build_embedding_text(&symbol);

    // Should return empty string for empty doc_comment
    assert!(
        embedding_text.is_empty(),
        "Should return empty string for markdown heading with empty doc_comment. Got: '{}'",
        embedding_text
    );
}

#[test]
fn test_non_markdown_symbols_unchanged_by_markdown_optimization() {
    // CONTRACT: Non-markdown symbols should not be affected by markdown optimization
    let symbol = Symbol {
        id: "test_rust_fn".to_string(),
        name: "process_data".to_string(),
        kind: SymbolKind::Function,
        language: "rust".to_string(),
        file_path: "src/processor.rs".to_string(),
        start_line: 42,
        start_column: 0,
        end_line: 50,
        end_column: 0,
        start_byte: 1200,
        end_byte: 1500,
        signature: Some("fn process_data(input: &str) -> Result<String>".to_string()),
        doc_comment: None, // No doc comment
        visibility: Some(Visibility::Public),
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: None,
        content_type: None,
    };

    let temp_dir = TempDir::new().unwrap();
    let db = create_test_db();

    let engine = std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            EmbeddingEngine::new("bge-small", temp_dir.path().to_path_buf(), db).await
        })
    })
    .join()
    .unwrap()
    .unwrap();

    let embedding_text = engine.build_embedding_text(&symbol);

    // Should still embed name, kind, signature for non-markdown
    assert!(
        embedding_text.contains("process_data"),
        "Should include function name. Got: '{}'",
        embedding_text
    );

    assert!(
        embedding_text.contains("function"),
        "Should include kind. Got: '{}'",
        embedding_text
    );

    assert!(
        embedding_text.contains("Result<String>"),
        "Should include signature. Got: '{}'",
        embedding_text
    );

    // Should NOT be empty (markdown optimization doesn't affect other languages)
    assert!(
        !embedding_text.is_empty(),
        "Non-markdown symbols should still be embedded normally"
    );
}

#[test]
fn test_loaded_index_uses_correct_dimensions() {
    // This test documents that LoadedHnswIndex should respect the dimensions
    // parameter instead of hard-coding 384
    //
    // Problem: search_similar() hard-codes 384 dimensions
    // Solution: Store dimensions in LoadedHnswIndex struct

    use crate::embeddings::loaded_index::LoadedHnswIndex;
    use hnsw_rs::prelude::*;

    // Create HNSW with NON-384 dimensions (e.g., 128 for a smaller model)
    let dimensions = 128;
    let max_nb_connection = 16;
    let nb_elem = 100; // Initial capacity
    let nb_layer = 16; // Number of layers
    let ef_construction = 200;
    let hnsw: Hnsw<f32, DistCosine> = Hnsw::new(
        max_nb_connection,
        nb_elem,
        nb_layer,
        ef_construction,
        DistCosine,
    );

    // Transmute to 'static (same as production code does)
    let hnsw_static: Hnsw<'static, f32, DistCosine> = unsafe { std::mem::transmute(hnsw) };

    // Create LoadedHnswIndex
    let id_mapping = vec!["test_id".to_string()];
    let loaded_index =
        LoadedHnswIndex::from_built_hnsw(hnsw_static, id_mapping, dimensions).unwrap();

    // Verify dimensions are stored and accessible
    assert_eq!(
        loaded_index.get_dimensions(),
        128,
        "LoadedHnswIndex should store and return correct dimensions"
    );

    println!("✅ LoadedHnswIndex stores dimensions correctly (not hard-coded)");
}

#[test]
fn test_get_type_for_symbol_database_helper() {
    // Test the database helper method for querying types
    use crate::database::*;
    use crate::extractors::base::TypeInfo;

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_types_helper.db");

    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Create a test symbol
    let symbol = Symbol {
        id: "test_func_123".to_string(),
        name: "testFunction".to_string(),
        kind: SymbolKind::Function,
        language: "typescript".to_string(),
        file_path: "test.ts".to_string(),
        start_line: 1,
        start_column: 0,
        end_line: 5,
        end_column: 0,
        start_byte: 0,
        end_byte: 100,
        signature: Some("function testFunction()".to_string()),
        doc_comment: None,
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: None,
        content_type: None,
    };

    // Store file info first (foreign key dependency)
    let file_info = FileInfo {
        path: "test.ts".to_string(),
        language: "typescript".to_string(),
        hash: "hash123".to_string(),
        size: 100,
        last_modified: 123456,
        last_indexed: 0,
        symbol_count: 1,
        content: None,
    };
    db.bulk_store_files(&[file_info]).unwrap();

    // Store symbol
    db.bulk_store_symbols(&[symbol.clone()], "test_workspace").unwrap();

    // Store type using bulk_store_types
    let type_info = TypeInfo {
        symbol_id: symbol.id.clone(),
        resolved_type: "string".to_string(),
        generic_params: None,
        constraints: None,
        is_inferred: false,
        language: "typescript".to_string(),
        metadata: None,
    };
    db.bulk_store_types(&[type_info], "test_workspace").unwrap();

    // Test: get_type_for_symbol should return the type
    let result = db.get_type_for_symbol(&symbol.id).unwrap();
    assert_eq!(result, Some("string".to_string()));

    // Test: non-existent symbol should return None
    let result2 = db.get_type_for_symbol("nonexistent").unwrap();
    assert_eq!(result2, None);

    println!("✅ get_type_for_symbol() works correctly");
}

#[cfg_attr(
    not(feature = "network_models"),
    ignore = "requires downloadable embedding model"
)]
#[tokio::test]
async fn test_build_embedding_text_includes_type_information() {
    // RED PHASE: This test WILL FAIL initially - that's the point!
    // We're testing that build_embedding_text() queries and includes type information from the types table

    let temp_dir = TempDir::new().unwrap();
    let cache_dir = temp_dir.path().to_path_buf();
    let db_path = temp_dir.path().join("test_types.db");

    // Create database and store a symbol with type information
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    let symbol = Symbol {
        id: "test_user_func".to_string(),
        name: "fetchUserProfile".to_string(),
        kind: SymbolKind::Function,
        language: "typescript".to_string(),
        file_path: "/src/services/user.ts".to_string(),
        start_line: 10,
        start_column: 0,
        end_line: 15,
        end_column: 1,
        start_byte: 200,
        end_byte: 350,
        signature: Some("function fetchUserProfile(id: string)".to_string()),
        doc_comment: Some("Fetches a user profile by ID".to_string()),
        visibility: Some(Visibility::Public),
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: None,
        content_type: None,
    };

    // Store the symbol in database
    {
        let symbols_slice = [symbol.clone()];
        db.store_symbols_transactional(&symbols_slice).unwrap();
    }

    // Store type information for this symbol
    db.conn.execute(
        "INSERT INTO types (symbol_id, resolved_type, is_inferred, language)
         VALUES (?1, ?2, ?3, ?4)",
        (
            &symbol.id,
            "Promise<UserProfile>",  // This is the return type we want in embeddings
            0,  // is_inferred = false (explicit type)
            "typescript",
        ),
    ).unwrap();

    // Create EmbeddingEngine with database containing type info
    let db_arc = Arc::new(Mutex::new(db));
    let engine = EmbeddingEngine::new("bge-small", cache_dir, db_arc)
        .await
        .unwrap();

    // Build embedding text
    let embedding_text = engine.build_embedding_text(&symbol);

    // ASSERTIONS: Verify type information is included
    println!("Embedding text: {}", embedding_text);

    // Should include the basic symbol info (existing behavior)
    assert!(embedding_text.contains("fetchUserProfile"), "Should include function name");
    assert!(embedding_text.contains("function"), "Should include symbol kind");
    assert!(embedding_text.contains("Fetches a user profile"), "Should include doc comment");

    // NEW BEHAVIOR: Should include type information from types table
    assert!(
        embedding_text.contains("Promise<UserProfile>"),
        "Should include resolved type from types table. Got: '{}'",
        embedding_text
    );
}
