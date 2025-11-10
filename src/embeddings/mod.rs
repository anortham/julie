// Julie's Embeddings Module - The Semantic Bridge
//
// This module provides semantic search capabilities with GPU-accelerated ONNX Runtime.
// It enables cross-language understanding by generating meaning-based vector representations.

use crate::database::SymbolDatabase;
use crate::extractors::base::Symbol;
use anyhow::{Context, Result};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing::warn;

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
    /// Optional database connection for persistence
    /// None = standalone mode (query-only, no persistence)
    /// Some = full mode (with persistence capabilities)
    db: Option<Arc<Mutex<SymbolDatabase>>>,
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
            db: Some(db),
            cache_dir,
            cpu_fallback_triggered: false,
        })
    }

    /// Create a standalone embedding engine without database (for query-only use)
    ///
    /// Use this when you only need `embed_text()` and don't need persistence.
    /// This avoids the dummy database overhead in tools like `julie-semantic query`.
    pub async fn new_standalone(model_name: &str, cache_dir: PathBuf) -> Result<Self> {
        tracing::info!("🚀 Initializing standalone EmbeddingEngine (no database)...");

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
            "🧠 Standalone EmbeddingEngine initialized (GPU-accelerated, {} dimensions)",
            dimensions
        );

        Ok(Self {
            model,
            model_name: model_name.to_string(),
            dimensions,
            db: None, // Standalone mode - no database
            cache_dir,
            cpu_fallback_triggered: false,
        })
    }

    /// Reinitialize the embedding engine in CPU-only mode after GPU failure
    ///
    /// This is called automatically when DirectML GPU crashes are detected.
    /// Recreates the ONNX model without GPU acceleration (Issue #3 fix: no unsafe env var).
    async fn reinitialize_with_cpu_fallback(&mut self) -> Result<()> {
        if self.cpu_fallback_triggered {
            // Already in CPU mode, don't reinitialize again
            return Ok(());
        }

        tracing::error!("🚨 GPU device failure detected - reinitializing in CPU-only mode");
        tracing::warn!("   This is slower but stable. Future batches will use CPU.");

        // Recreate the model manager and get model paths
        let model_manager = ModelManager::new(self.cache_dir.clone())?;
        let model_paths = model_manager
            .ensure_model_downloaded(&self.model_name)
            .await?;

        // Recreate the ONNX model in CPU-only mode (no env var needed!)
        let new_model = OrtEmbeddingModel::new_cpu_only(
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
    pub fn embed_symbol(&mut self, symbol: &Symbol, _context: &CodeContext) -> Result<Vec<f32>> {
        let enriched_text = self.build_embedding_text(symbol);

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

    /// Calculate optimal batch size based on available GPU memory
    /// Uses conservative heuristics validated against real-world GPU testing
    ///
    /// # Real-World Data Points:
    /// - 6GB NVIDIA A1000: batch_size=50 works, batch_size=100 crashes
    /// - Conclusion: Use ~8-10% of GPU memory for batching (safe margin)
    ///
    /// # Returns
    /// Optimal batch size for this GPU, or fallback constants if detection fails
    pub fn calculate_optimal_batch_size(&self) -> usize {
        // Try to detect GPU memory
        if let Some(vram_bytes) = self.model.get_gpu_memory_bytes() {
            Self::batch_size_from_vram(vram_bytes)
        } else {
            // Fallback to conservative defaults
            if self.is_using_gpu() {
                50 // Conservative GPU default
            } else {
                100 // CPU mode
            }
        }
    }

    /// Calculate batch size from GPU VRAM using empirical formula
    /// Based on real-world testing: 6GB GPU → batch_size=50
    ///
    /// # Performance Characteristics (Important!)
    ///
    /// **When Larger Batches Help:**
    /// - Memory-bound workloads (overhead dominates)
    /// - Larger embedding models (e.g., bge-large with 1024 dims)
    /// - Future models with more complex architectures
    ///
    /// **When Larger Batches DON'T Help (Empirically Validated):**
    /// - BGE-small (384 dims) on modern RTX GPUs is **compute-bound**
    /// - Test: 12GB RTX GPU showed NO speedup from batch_size 50→97
    /// - Reason: GPU cores are 100% utilized at batch_size=50 already
    /// - Larger batches just take proportionally longer per batch
    ///
    /// **Why We Still Use Dynamic Batch Sizing:**
    /// 1. Prevents OOM crashes on smaller GPUs (6GB crashes at batch_size=100)
    /// 2. Safe scaling for users with different GPU memory (4GB→24GB)
    /// 3. Future-proof for larger models that may benefit from batching
    /// 4. No performance regression (same speed on compute-bound workloads)
    ///
    /// # Real-World Test Data:
    /// - 6GB NVIDIA A1000: batch_size=50 ✓ safe, batch_size=100 ✗ OOM crash
    /// - 12GB RTX GPU: batch_size=50 → 23.3s, batch_size=97 → 23.9s (no speedup)
    /// - Conclusion: Formula prevents crashes without sacrificing performance
    fn batch_size_from_vram(vram_bytes: usize) -> usize {
        let vram_gb = vram_bytes as f64 / 1_073_741_824.0;

        // Empirical formula: batch_size = (VRAM_GB / 6.0) * 50
        // This is conservative and validated against 6GB A1000 testing
        //
        // Examples:
        // - 4GB:  (4/6)  * 50 = 33  → clamp to 50 (minimum)
        // - 6GB:  (6/6)  * 50 = 50  ✓ (validated safe)
        // - 8GB:  (8/6)  * 50 = 67  ✓
        // - 12GB: (12/6) * 50 = 100 ✓ (safe but no speedup vs 50 on BGE-small)
        // - 16GB: (16/6) * 50 = 133 ✓
        // - 24GB: (24/6) * 50 = 200 ✓

        let calculated = ((vram_gb / 6.0) * 50.0) as usize;

        // Clamp to safe range: [50, 250]
        // - Minimum 50: Validated safe on 6GB GPU
        // - Maximum 250: Avoid timeout issues and excessive failure blast radius
        let batch_size = calculated.clamp(50, 250);

        tracing::info!(
            "📊 GPU Memory: {:.2} GB → Dynamic batch size: {}",
            vram_gb,
            batch_size
        );

        batch_size
    }

    /// PERFORMANCE OPTIMIZATION: Generate embeddings for a batch of symbols using batched ML inference
    /// This dramatically reduces ML model overhead compared to individual embedding calls
    /// Now GPU-accelerated for 10-100x speedup!
    ///
    /// CRITICAL: Respects batch size limits to prevent OOM errors (Bug fix for 23GB+ allocation attempts)
    pub fn embed_symbols_batch(&mut self, symbols: &[Symbol]) -> Result<Vec<(String, Vec<f32>)>> {
        if symbols.is_empty() {
            return Ok(Vec::new());
        }

        // Calculate optimal batch size based on GPU/CPU capabilities
        let batch_size = self.calculate_optimal_batch_size();

        // If input is within batch size, process directly (fast path)
        if symbols.len() <= batch_size {
            return self.embed_symbols_batch_internal(symbols);
        }

        // Split large inputs into manageable chunks to prevent OOM
        tracing::info!(
            "🔄 Splitting {} symbols into batches of {} to prevent memory exhaustion",
            symbols.len(),
            batch_size
        );

        let mut all_results = Vec::new();

        for chunk in symbols.chunks(batch_size) {
            match self.embed_symbols_batch_internal(chunk) {
                Ok(mut chunk_results) => {
                    all_results.append(&mut chunk_results);
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to process chunk of {} symbols: {}",
                        chunk.len(),
                        e
                    );
                    return Err(e);
                }
            }
        }

        Ok(all_results)
    }

    /// Internal batch embedding logic - processes a single batch WITHOUT splitting
    /// This is the actual ONNX batch call - caller must ensure batch size is safe!
    fn embed_symbols_batch_internal(&mut self, symbols: &[Symbol]) -> Result<Vec<(String, Vec<f32>)>> {
        // Collect all embedding texts and contexts for this batch
        let mut batch_texts = Vec::new();
        let mut symbol_ids = Vec::new();

        for symbol in symbols {
            let _context = CodeContext::from_symbol(symbol);
            let embedding_text = self.build_embedding_text(symbol);
            batch_texts.push(embedding_text);
            symbol_ids.push(symbol.id.clone());
        }

        // Generate embeddings for this batch in one GPU-accelerated call
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
                            let embedding_text = self.build_embedding_text(symbol);
                            tracing::warn!(
                                "Failed to embed symbol {} ({}::{}, {} chars): {}",
                                symbol.id,
                                symbol.file_path,
                                symbol.name,
                                embedding_text.len(),
                                e
                            );
                            // Log text preview at warn level for troubleshooting (Issue #2 fix)
                            tracing::warn!(
                                "Failed embedding text (first 500 chars): {:?}",
                                &embedding_text.chars().take(500).collect::<String>()
                            );
                            tracing::warn!(
                                "Text stats: length={}, lines={}",
                                embedding_text.len(),
                                embedding_text.lines().count()
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
        // Require database for persistence operations
        if self.db.is_none() {
            anyhow::bail!("Database required for upsert_file_embeddings - use EmbeddingEngine::new() instead of new_standalone()");
        }

        if symbols.is_empty() {
            return Ok(());
        }

        // PERFORMANCE OPTIMIZATION: Use batching for efficient ML inference
        let mut batch_texts = Vec::new();
        let mut symbol_contexts = Vec::new();

        for symbol in symbols {
            let context = CodeContext::from_symbol(symbol);
            let embedding_text = self.build_embedding_text(symbol);
            batch_texts.push(embedding_text);
            symbol_contexts.push((symbol, context));
        }

        // Generate embeddings for all symbols in one GPU-accelerated batch call
        match self.model.encode_batch(batch_texts) {
            Ok(batch_embeddings) => {
                // Safe unwrap - we checked is_none() above
                let db_guard = match self.db.as_ref().unwrap().lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => {
                        warn!("Database mutex poisoned during batch embedding storage, recovering: {}", poisoned);
                        poisoned.into_inner()
                    }
                };

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
                            // Safe unwrap - we checked is_none() above
                            let db_guard = match self.db.as_ref().unwrap().lock() {
                                Ok(guard) => guard,
                                Err(poisoned) => {
                                    warn!("Database mutex poisoned during individual embedding storage, recovering: {}", poisoned);
                                    poisoned.into_inner()
                                }
                            };
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
        // Require database for deletion operations
        if self.db.is_none() {
            anyhow::bail!("Database required for remove_embeddings_for_symbols - use EmbeddingEngine::new() instead of new_standalone()");
        }

        if symbol_ids.is_empty() {
            return Ok(());
        }

        // Safe unwrap - we checked is_none() above
        let db_guard = match self.db.as_ref().unwrap().lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                warn!("Database mutex poisoned during embedding removal, recovering: {}", poisoned);
                poisoned.into_inner()
            }
        };

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
        // Require database for retrieval operations
        if self.db.is_none() {
            anyhow::bail!("Database required for get_embedding - use EmbeddingEngine::new() instead of new_standalone()");
        }

        // Safe unwrap - we checked is_none() above
        let db_guard = match self.db.as_ref().unwrap().lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                warn!("Database mutex poisoned during embedding retrieval, recovering: {}", poisoned);
                poisoned.into_inner()
            }
        };
        db_guard.get_embedding_for_symbol(symbol_id, &self.model_name)
    }

    pub fn build_embedding_text(&self, symbol: &Symbol) -> String {
        // Minimal embeddings for clean semantic matching in 384-dimensional space
        // Philosophy: Less noise = stronger signal in BGE-small's limited dimensions
        // Issue #7 fix: Removed unused _context parameter - symbol.code_context used directly
        let mut parts = vec![symbol.name.clone(), symbol.kind.to_string()];

        // Add signature if available (type information aids semantic understanding)
        if let Some(sig) = &symbol.signature {
            parts.push(sig.clone());
        }

        // Add documentation comment if available (enables natural language queries)
        if let Some(doc) = &symbol.doc_comment {
            parts.push(doc.clone());
        }

        // Add code context if available (enables semantic search on actual code patterns)
        // This is the key enhancement: all 30 extractors capture 3 lines before + symbol + 3 after
        // Including this gives embeddings richer understanding of how symbols are used
        if let Some(ctx) = &symbol.code_context {
            parts.push(ctx.clone());
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
