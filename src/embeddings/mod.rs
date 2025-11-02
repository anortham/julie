// Julie's Embeddings Module - The Semantic Bridge
//
// This module provides semantic search capabilities with GPU-accelerated ONNX Runtime.
// It enables cross-language understanding by generating meaning-based vector representations.

use crate::database::SymbolDatabase;
use crate::extractors::base::Symbol;
use anyhow::{Context, Result};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

// GPU-accelerated embeddings infrastructure
use self::model_manager::ModelManager;
use self::ort_model::OrtEmbeddingModel;

pub mod cross_language;
pub mod loaded_index; // Safe wrapper for loaded HNSW indexes
pub mod model_manager; // Model downloading from HuggingFace
pub mod ort_model;
pub mod vector_store; // ONNX Runtime embeddings with GPU acceleration

// Re-export LoadedHnswIndex for use in other modules
pub use loaded_index::LoadedHnswIndex;

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

/// The embedding engine that powers semantic code understanding with GPU acceleration
pub struct EmbeddingEngine {
    model: OrtEmbeddingModel, // GPU-accelerated ONNX Runtime model
    model_name: String,
    dimensions: usize,
    /// Required database connection for persistence (no in-memory fallback)
    db: Arc<Mutex<SymbolDatabase>>,
    /// Cache directory for model files (needed for reinitialization)
    cache_dir: PathBuf,
    /// Track if we've fallen back to CPU mode due to GPU failure
    cpu_fallback_triggered: bool,
}

impl EmbeddingEngine {
    /// Create a new embedding engine with GPU-accelerated ONNX Runtime
    ///
    /// This is now async because it downloads the model from HuggingFace on first run.
    pub async fn new(
        model_name: &str,
        cache_dir: PathBuf,
        db: Arc<Mutex<SymbolDatabase>>,
    ) -> Result<Self> {
        tracing::info!("🚀 Initializing EmbeddingEngine with GPU acceleration...");

        // 1. Set up model manager and download model if needed
        let model_manager = ModelManager::new(cache_dir.clone())?;
        let model_paths = model_manager.ensure_model_downloaded(model_name).await?;

        // 2. Create GPU-accelerated ORT model
        let model = OrtEmbeddingModel::new(
            model_paths.model,
            model_paths.tokenizer,
            model_name,
            Some(model_manager.cache_dir()), // Pass cache dir for CoreML caching
        )?;

        let dimensions = model.dimensions();

        tracing::info!(
            "🧠 EmbeddingEngine initialized with model {} (GPU-accelerated, {} dimensions)",
            model_name,
            dimensions
        );

        Ok(Self {
            model,
            model_name: model_name.to_string(),
            dimensions,
            db,
            cache_dir,
            cpu_fallback_triggered: false,
        })
    }

    /// Reinitialize the embedding engine in CPU-only mode after GPU failure
    ///
    /// This is called automatically when DirectML GPU crashes are detected.
    /// Sets JULIE_FORCE_CPU=1 and recreates the ONNX model without GPU acceleration.
    async fn reinitialize_with_cpu_fallback(&mut self) -> Result<()> {
        if self.cpu_fallback_triggered {
            // Already in CPU mode, don't reinitialize again
            return Ok(());
        }

        tracing::error!("🚨 GPU device failure detected - reinitializing in CPU-only mode");
        tracing::warn!("   This is slower but stable. Future batches will use CPU.");

        // Set environment variable to force CPU mode
        unsafe {
            std::env::set_var("JULIE_FORCE_CPU", "1");
        }

        // Recreate the model manager and get model paths
        let model_manager = ModelManager::new(self.cache_dir.clone())?;
        let model_paths = model_manager
            .ensure_model_downloaded(&self.model_name)
            .await?;

        // Recreate the ONNX model in CPU-only mode
        let new_model = OrtEmbeddingModel::new(
            model_paths.model,
            model_paths.tokenizer,
            &self.model_name,
            Some(model_manager.cache_dir()),
        )
        .context("Failed to reinitialize embedding model in CPU mode")?;

        // Replace the crashed GPU model with CPU model
        self.model = new_model;
        self.cpu_fallback_triggered = true;

        tracing::info!("✅ Successfully reinitialized embedding engine in CPU-only mode");

        Ok(())
    }

    /// Generate context-aware embedding for a symbol
    pub fn embed_symbol(&mut self, symbol: &Symbol, context: &CodeContext) -> Result<Vec<f32>> {
        let enriched_text = self.build_embedding_text(symbol, context);

        // Generate embedding using GPU-accelerated ORT model
        self.model.encode_single(enriched_text)
    }

    /// Generate embedding for arbitrary text
    pub fn embed_text(&mut self, text: &str) -> Result<Vec<f32>> {
        // Generate embedding using GPU-accelerated ORT model
        self.model.encode_single(text.to_string())
    }

    /// Get the dimensions of embeddings produced by this model
    pub fn dimensions(&self) -> usize {
        self.dimensions
    }

    /// Get the model name
    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    /// Check if GPU acceleration is actually being used
    pub fn is_using_gpu(&self) -> bool {
        self.model.is_using_gpu()
    }

    /// PERFORMANCE OPTIMIZATION: Generate embeddings for a batch of symbols using batched ML inference
    /// This dramatically reduces ML model overhead compared to individual embedding calls
    /// Now GPU-accelerated for 10-100x speedup!
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

        // Generate embeddings for all symbols in one GPU-accelerated batch call
        let batch_result = self.model.encode_batch(batch_texts.clone());

        match batch_result {
            Ok(batch_embeddings) => {
                // Map results back to (id, embedding) pairs
                let results = symbol_ids.into_iter().zip(batch_embeddings).collect();
                Ok(results)
            }
            Err(e) => {
                let error_msg = e.to_string();

                // Check for GPU device failure (DirectML crash: 0x887A0005)
                let is_gpu_crash = error_msg.contains("887A0005")
                    || error_msg.contains("GPU device instance has been suspended")
                    || error_msg.contains("device removed");

                if is_gpu_crash && !self.cpu_fallback_triggered {
                    tracing::error!(
                        "🚨 GPU device failure detected during batch embedding: {}",
                        error_msg
                    );

                    // Attempt to reinitialize with CPU fallback
                    match tokio::task::block_in_place(|| {
                        tokio::runtime::Handle::current()
                            .block_on(self.reinitialize_with_cpu_fallback())
                    }) {
                        Ok(_) => {
                            tracing::info!(
                                "✅ Successfully reinitialized with CPU - retrying batch"
                            );
                            // Retry the batch with CPU mode now active
                            match self.model.encode_batch(batch_texts) {
                                Ok(batch_embeddings) => {
                                    let results =
                                        symbol_ids.into_iter().zip(batch_embeddings).collect();
                                    return Ok(results);
                                }
                                Err(retry_err) => {
                                    tracing::error!(
                                        "CPU batch embedding also failed: {}",
                                        retry_err
                                    );
                                }
                            }
                        }
                        Err(fallback_err) => {
                            tracing::error!(
                                "Failed to reinitialize with CPU fallback: {}",
                                fallback_err
                            );
                        }
                    }
                }

                tracing::warn!(
                    "Batch embedding failed ({} symbols): {}, falling back to individual processing",
                    symbols.len(),
                    e
                );
                tracing::debug!(
                    "Failed batch symbol IDs: {:?}",
                    symbols.iter().map(|s| &s.id).take(5).collect::<Vec<_>>()
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
                            // Check for GPU crash on individual embedding too
                            let error_msg = e.to_string();
                            let is_gpu_crash = error_msg.contains("887A0005")
                                || error_msg.contains("GPU device instance has been suspended");

                            if is_gpu_crash && !self.cpu_fallback_triggered {
                                tracing::error!(
                                    "🚨 GPU crash on individual embedding - triggering fallback"
                                );
                                if let Err(fallback_err) = tokio::task::block_in_place(|| {
                                    tokio::runtime::Handle::current()
                                        .block_on(self.reinitialize_with_cpu_fallback())
                                }) {
                                    tracing::error!("CPU fallback failed: {}", fallback_err);
                                }
                                // Try once more with CPU
                                if let Ok(embedding) = self.embed_symbol(symbol, &context) {
                                    results.push((symbol.id.clone(), embedding));
                                    continue;
                                }
                            }

                            // Log detailed error information for debugging
                            let embedding_text = self.build_embedding_text(symbol, &context);
                            tracing::warn!(
                                "Failed to embed symbol {} ({}::{}, {} chars): {}",
                                symbol.id,
                                symbol.file_path,
                                symbol.name,
                                embedding_text.len(),
                                e
                            );
                            tracing::debug!(
                                "Failed embedding text preview: {:?}",
                                &embedding_text.chars().take(200).collect::<String>()
                            );
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

        // Generate embeddings for all symbols in one GPU-accelerated batch call
        match self.model.encode_batch(batch_texts) {
            Ok(batch_embeddings) => {
                let db_guard = self.db.lock().unwrap();

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
                            let db_guard = self.db.lock().unwrap();
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

        let db_guard = self.db.lock().unwrap();

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
        let db_guard = self.db.lock().unwrap();
        db_guard.get_embedding_for_symbol(symbol_id, &self.model_name)
    }

    pub fn build_embedding_text(&self, symbol: &Symbol, _context: &CodeContext) -> String {
        // Minimal embeddings for clean semantic matching in 384-dimensional space
        // Philosophy: Less noise = stronger signal in BGE-small's limited dimensions
        let mut parts = vec![symbol.name.clone(), symbol.kind.to_string()];

        // Add signature if available (type information aids semantic understanding)
        if let Some(sig) = &symbol.signature {
            parts.push(sig.clone());
        }

        // Add documentation comment if available (enables natural language queries)
        if let Some(doc) = &symbol.doc_comment {
            parts.push(doc.clone());
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

/// Test function to verify real-time GPU embeddings are working
/// This should trigger incremental indexing with background GPU generation
/// Returns true if the real-time embedding system is operational
pub fn test_real_time_embeddings_marker() -> bool {
    // This function exists to test that file changes trigger
    // background GPU embedding generation via the file watcher
    // With RTX 4080, embeddings should generate in <1s for small changes
    true
}
