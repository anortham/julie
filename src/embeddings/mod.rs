// Julie's Embeddings Module - The Semantic Bridge
//
// This module provides semantic search capabilities using FastEmbed for easy model integration.
// It enables cross-language understanding by generating meaning-based vector representations.

use crate::database::SymbolDatabase;
use crate::extractors::base::Symbol;
use anyhow::Result;
use fastembed::{EmbeddingModel, TextEmbedding, TextInitOptions};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

pub mod cross_language;
pub mod vector_store;

/// Context information for generating richer embeddings
#[derive(Debug, Clone)]
pub struct CodeContext {
    pub parent_symbol: Option<Box<Symbol>>,
    pub surrounding_code: Option<String>,
    pub file_context: Option<String>,
}

impl Default for CodeContext {
    fn default() -> Self {
        Self::new()
    }
}

impl CodeContext {
    pub fn new() -> Self {
        Self {
            parent_symbol: None,
            surrounding_code: None,
            file_context: None,
        }
    }

    pub fn from_symbol(symbol: &Symbol) -> Self {
        Self {
            parent_symbol: None,
            surrounding_code: None,
            file_context: Some(symbol.file_path.clone()),
        }
    }
}

/// The embedding engine that powers semantic code understanding
pub struct EmbeddingEngine {
    model: TextEmbedding,
    model_name: String,
    dimensions: usize,
    /// Required database connection for persistence (no in-memory fallback)
    db: Arc<Mutex<SymbolDatabase>>,
}

impl EmbeddingEngine {
    /// Create a new embedding engine with database persistence
    pub fn new(
        model_name: &str,
        cache_dir: PathBuf,
        db: Arc<Mutex<SymbolDatabase>>,
    ) -> Result<Self> {
        let (model, dimensions) = match model_name {
            "bge-small" => {
                let options =
                    TextInitOptions::new(EmbeddingModel::BGESmallENV15).with_cache_dir(cache_dir);
                (TextEmbedding::try_new(options)?, 384)
            }
            _ => {
                // Default to BGE Small for now
                let options =
                    TextInitOptions::new(EmbeddingModel::BGESmallENV15).with_cache_dir(cache_dir);
                (TextEmbedding::try_new(options)?, 384)
            }
        };

        tracing::info!(
            "🧠 EmbeddingEngine initialized with model {} (database-backed, no in-memory storage)",
            model_name
        );

        Ok(Self {
            model,
            model_name: model_name.to_string(),
            dimensions,
            db,
        })
    }

    /// Generate context-aware embedding for a symbol
    pub fn embed_symbol(&mut self, symbol: &Symbol, context: &CodeContext) -> Result<Vec<f32>> {
        let enriched_text = self.build_embedding_text(symbol, context);

        // Generate embedding
        let embeddings = self.model.embed(vec![enriched_text], None)?;
        Ok(embeddings.into_iter().next().unwrap())
    }

    /// Generate embedding for arbitrary text
    pub fn embed_text(&mut self, text: &str) -> Result<Vec<f32>> {
        let embeddings = self.model.embed(vec![text.to_string()], None)?;
        Ok(embeddings.into_iter().next().unwrap())
    }

    /// Get the dimensions of embeddings produced by this model
    pub fn dimensions(&self) -> usize {
        self.dimensions
    }

    /// Get the model name
    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    /// PERFORMANCE OPTIMIZATION: Generate embeddings for a batch of symbols using batched ML inference
    /// This dramatically reduces ML model overhead compared to individual embedding calls
    pub fn embed_symbols_batch(&mut self, symbols: &[Symbol]) -> Result<Vec<(String, Vec<f32>)>> {
        if symbols.is_empty() {
            return Ok(Vec::new());
        }

        // Collect all embedding texts and contexts in batches for efficient ML inference
        let mut batch_texts = Vec::new();
        let mut symbol_ids = Vec::new();

        for symbol in symbols {
            let context = CodeContext::from_symbol(symbol);
            let embedding_text = self.build_embedding_text(symbol, &context);
            batch_texts.push(embedding_text);
            symbol_ids.push(symbol.id.clone());
        }

        // Generate embeddings for all symbols in one batch call
        match self.model.embed(batch_texts, None) {
            Ok(batch_embeddings) => {
                // Map results back to (id, embedding) pairs
                let results = symbol_ids.into_iter().zip(batch_embeddings).collect();
                Ok(results)
            }
            Err(e) => {
                tracing::warn!(
                    "Batch embedding failed: {}, falling back to individual processing",
                    e
                );

                // Fallback to individual processing if batch fails
                let mut results = Vec::new();
                for symbol in symbols {
                    let context = CodeContext::from_symbol(symbol);
                    match self.embed_symbol(symbol, &context) {
                        Ok(embedding) => {
                            results.push((symbol.id.clone(), embedding));
                        }
                        Err(e) => {
                            // Log the error but continue with other symbols
                            tracing::warn!("Failed to embed symbol {}: {}", symbol.id, e);
                        }
                    }
                }
                Ok(results)
            }
        }
    }

    /// Update embeddings for all symbols in a file (database-only, no in-memory cache)
    pub async fn upsert_file_embeddings(
        &mut self,
        file_path: &str,
        symbols: &[Symbol],
    ) -> Result<()> {
        if symbols.is_empty() {
            return Ok(());
        }

        // PERFORMANCE OPTIMIZATION: Use batching for efficient ML inference
        let mut batch_texts = Vec::new();
        let mut symbol_contexts = Vec::new();

        for symbol in symbols {
            let context = CodeContext::from_symbol(symbol);
            let embedding_text = self.build_embedding_text(symbol, &context);
            batch_texts.push(embedding_text);
            symbol_contexts.push((symbol, context));
        }

        // Generate embeddings for all symbols in one batch call
        match self.model.embed(batch_texts, None) {
            Ok(batch_embeddings) => {
                let db_guard = self.db.lock().await;

                // Persist all embeddings directly to database
                for (embedding, (symbol, _context)) in
                    batch_embeddings.into_iter().zip(symbol_contexts.iter())
                {
                    let vector_id = &symbol.id;

                    // Store the vector data
                    if let Err(e) = db_guard.store_embedding_vector(
                        vector_id,
                        &embedding,
                        self.dimensions,
                        &self.model_name,
                    ) {
                        tracing::warn!("Failed to persist vector for {}: {}", symbol.id, e);
                    }

                    // Store the metadata linking symbol to vector
                    if let Err(e) = db_guard.store_embedding_metadata(
                        &symbol.id,
                        vector_id,
                        &self.model_name,
                        None, // embedding_hash not computed yet
                    ) {
                        tracing::warn!(
                            "Failed to persist embedding metadata for {}: {}",
                            symbol.id,
                            e
                        );
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to generate batch embeddings for file {}: {}. Falling back to individual processing.",
                    file_path,
                    e
                );

                // Fallback to individual processing if batch fails
                for symbol in symbols {
                    let context = CodeContext::from_symbol(symbol);
                    match self.embed_symbol(symbol, &context) {
                        Ok(embedding) => {
                            let db_guard = self.db.lock().await;
                            let vector_id = &symbol.id;

                            if let Err(e) = db_guard.store_embedding_vector(
                                vector_id,
                                &embedding,
                                self.dimensions,
                                &self.model_name,
                            ) {
                                tracing::warn!("Failed to persist vector for {}: {}", symbol.id, e);
                            }

                            if let Err(e) = db_guard.store_embedding_metadata(
                                &symbol.id,
                                vector_id,
                                &self.model_name,
                                None, // embedding_hash not computed yet
                            ) {
                                tracing::warn!(
                                    "Failed to persist embedding metadata for {}: {}",
                                    symbol.id,
                                    e
                                );
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Failed to embed symbol {} in {}: {}",
                                symbol.id,
                                file_path,
                                e
                            );
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Remove embeddings for symbols in a file (database-only)
    /// Note: Requires symbol IDs to be provided since we don't track file->symbol mapping in memory
    pub async fn remove_embeddings_for_symbols(&mut self, symbol_ids: &[String]) -> Result<()> {
        if symbol_ids.is_empty() {
            return Ok(());
        }

        let db_guard = self.db.lock().await;

        for symbol_id in symbol_ids {
            if let Err(e) = db_guard.delete_embeddings_for_symbol(symbol_id) {
                tracing::warn!("Failed to delete embeddings for {}: {}", symbol_id, e);
            }
        }

        tracing::debug!("Removed embeddings for {} symbols", symbol_ids.len());
        Ok(())
    }

    /// Retrieve an embedding vector from the database
    pub async fn get_embedding(&self, symbol_id: &str) -> Result<Option<Vec<f32>>> {
        let db_guard = self.db.lock().await;
        db_guard.get_embedding_for_symbol(symbol_id, &self.model_name)
    }

    pub fn build_embedding_text(&self, symbol: &Symbol, context: &CodeContext) -> String {
        // Combine multiple sources of information for richer embeddings
        let mut parts = vec![symbol.name.clone(), symbol.kind.to_string()];

        // Add signature if available
        if let Some(sig) = &symbol.signature {
            parts.push(sig.clone());
        }

        // Add parent context
        if let Some(parent) = &context.parent_symbol {
            parts.push(format!("in {}", parent.name));
        }

        // Type information would be included in signature if available
        // (removed type_info field since it doesn't exist in Symbol struct)

        // Add surrounding code context (first few lines)
        if let Some(surrounding) = &context.surrounding_code {
            parts.push(surrounding.clone());
        }

        // Add filename context (helps with architectural understanding)
        if let Some(filename) = std::path::Path::new(&symbol.file_path).file_name() {
            parts.push(filename.to_string_lossy().to_string());
        }

        parts.join(" ")
    }
}

/// Calculate cosine similarity between two embedding vectors
pub fn cosine_similarity(vec_a: &[f32], vec_b: &[f32]) -> f32 {
    if vec_a.len() != vec_b.len() {
        return 0.0;
    }

    let dot_product: f32 = vec_a.iter().zip(vec_b.iter()).map(|(a, b)| a * b).sum();
    let norm_a: f32 = vec_a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = vec_b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot_product / (norm_a * norm_b)
}

/// Similarity search result
#[derive(Debug, Clone)]
pub struct SimilarityResult {
    pub symbol_id: String,
    pub similarity_score: f32,
    pub embedding: Vec<f32>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::SymbolDatabase;
    use crate::extractors::base::*;
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::sync::Mutex;

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
}
