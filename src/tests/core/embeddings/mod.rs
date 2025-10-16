// Tests extracted from src/embeddings/mod.rs
// These were previously inline tests that have been moved to follow project standards

use crate::database::SymbolDatabase;
use crate::embeddings::{cosine_similarity, CodeContext, EmbeddingEngine};
use crate::extractors::base::{Symbol, SymbolKind};
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
    let engine = EmbeddingEngine::new("bge-small", cache_dir, db).unwrap();
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

    let mut engine = EmbeddingEngine::new("bge-small", cache_dir, db).unwrap();

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

    let mut engine = EmbeddingEngine::new("bge-small", cache_dir, db).unwrap();

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

    let mut engine = EmbeddingEngine::new("bge-small", cache_dir, db).unwrap();

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
    };

    let context = CodeContext::from_symbol(&symbol);
    assert_eq!(context.file_context, Some("/test.rs".to_string()));
}

#[cfg_attr(
    not(feature = "network_models"),
    ignore = "requires downloadable embedding model"
)]
#[test]
fn test_build_embedding_text() {
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = temp_dir.path().to_path_buf();
    let db = create_test_db();

    let engine = EmbeddingEngine::new("bge-small", cache_dir, db).unwrap();

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
        code_context: None,
    };

    let mut context = CodeContext::from_symbol(&symbol);
    context.surrounding_code = Some("// Fetch user data from API".to_string());

    let embedding_text = engine.build_embedding_text(&symbol, &context);

    // Should include all the important information
    assert!(embedding_text.contains("getUserData"));
    assert!(embedding_text.contains("function")); // SymbolKind::Function.to_string() returns "function" lowercase
    assert!(embedding_text.contains("function getUserData(): Promise<User>"));
    assert!(embedding_text.contains("user.ts"));
    assert!(embedding_text.contains("Fetch user data from API"));
}
