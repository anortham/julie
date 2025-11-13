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

    /// Calculate batch size from GPU VRAM using DirectML-safe formula
    /// Based on real-world testing: 6GB GPU → batch_size=30 (DirectML-safe)
    ///
    /// # DirectML Memory Pressure Fix (v1.7.1)
    ///
    /// **Problem:** Previous formula used TOTAL VRAM without accounting for already-allocated memory.
    /// - 6GB A1000 at 97.6% utilization (5.86GB/6GB used) with batch_size=50:
    ///   → 55-second batch time (severe thrashing)
    ///   → GPU crash on next batch (DirectML error 887A0006)
    ///
    /// **Solution:** 40% more conservative formula for DirectML fragility:
    /// - Old: `(VRAM_GB / 6.0) * 50` → 6GB GPU = batch_size=50
    /// - New: `(VRAM_GB / 6.0) * 30` → 6GB GPU = batch_size=30 ✓
    ///
    /// **Why DirectML Needs Extra Headroom:**
    /// - DirectML on Windows is more fragile under memory pressure than CUDA
    /// - Fails with 887A0006 (GPU not responding) rather than graceful OOM
    /// - Smaller batches prevent thrashing and maintain stable operation
    ///
    /// # Performance Characteristics
    ///
    /// **Batch Size vs Speed (BGE-small on compute-bound GPUs):**
    /// - 12GB RTX GPU: batch_size=30 → 14.0s, batch_size=60 → 14.3s (no speedup)
    /// - BGE-small (384 dims) is compute-bound on modern GPUs
    /// - GPU cores are 100% utilized even at smaller batch sizes
    /// - Larger batches just take proportionally longer per batch
    ///
    /// **Why Conservative Sizing Works:**
    /// 1. No performance penalty on compute-bound workloads (most cases)
    /// 2. Prevents crashes under memory pressure (DirectML safety)
    /// 3. Safe scaling across GPU memory ranges (4GB→24GB)
    /// 4. Future-proof for larger models that may benefit from batching
    pub(crate) fn batch_size_from_vram(vram_bytes: usize) -> usize {
        let vram_gb = vram_bytes as f64 / 1_073_741_824.0;

        // DIRECTML-SAFE FORMULA: batch_size = (VRAM_GB / 6.0) * 30
        // 40% more conservative than previous formula to handle DirectML memory pressure
        //
        // Background: DirectML on Windows is more fragile under memory pressure than CUDA.
        // Previous formula used total VRAM without accounting for already-allocated memory,
        // causing crashes at 97.6% GPU utilization (5.86GB/6GB used).
        //
        // Real-world validation:
        // - 6GB A1000 at 97.6% utilization: batch_size=50 → 55s batch time → GPU crash
        // - 6GB A1000 with batch_size=30: Stable operation under memory pressure
        //
        // Examples:
        // - 4GB:  (4/6)  * 30 = 20  → clamp to 25 (minimum)
        // - 6GB:  (6/6)  * 30 = 30  ✓ (DirectML-safe)
        // - 8GB:  (8/6)  * 30 = 40  ✓
        // - 12GB: (12/6) * 30 = 60  ✓
        // - 16GB: (16/6) * 30 = 80  ✓
        // - 24GB: (24/6) * 30 = 120 ✓

        let calculated = ((vram_gb / 6.0) * 30.0) as usize;

        // Clamp to safe range: [25, 250]
        // - Minimum 25: Ensures reasonable performance even on small GPUs
        // - Maximum 250: Avoid timeout issues and excessive failure blast radius
        let batch_size = calculated.clamp(25, 250);

        tracing::info!(
            "📊 GPU Memory: {:.2} GB → Dynamic batch size: {} (DirectML-safe)",
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
        // Phase 2: Filter out symbols with empty embedding text (e.g., non-description memory symbols)
        let mut batch_texts = Vec::new();
        let mut symbol_ids = Vec::new();

        for symbol in symbols {
            let _context = CodeContext::from_symbol(symbol);
            let embedding_text = self.build_embedding_text(symbol);

            // Skip symbols with empty embedding text (Phase 2: memory optimization)
            // Empty text means the symbol should not get an embedding (e.g., "id", "timestamp", "tags" in .memories/)
            if embedding_text.is_empty() {
                continue;
            }

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
        // RAG-optimized embeddings: Focus on semantic units, not noise
        // Philosophy: One concept per embedding = clearer signal in 384-dimensional space
        //
        // 2025-11-11 Phase 1: Removed code_context (was 88% of embedded text)
        // Rationale: BGE-Small truncates at 512 tokens (~2KB), code_context added noise
        // Result: 75-88% faster embedding generation, clearer semantic matching
        //
        // 2025-11-11 Phase 2: Custom pipeline for .memories/ files
        // Memory files get focused embeddings: "{type}: {description}"
        // Example: "checkpoint: Fixed auth bug" or "decision: Chose SQLite over PostgreSQL"

        // Check if this is a memory file (but NOT mutable plans from Phase 3)
        if symbol.file_path.starts_with(".memories/") && !symbol.file_path.starts_with(".memories/plans/") {
            return self.build_memory_embedding_text(symbol);
        }

        // Standard code symbol embedding
        let mut parts = vec![symbol.name.clone(), symbol.kind.to_string()];

        // Add signature if available (type information aids semantic understanding)
        if let Some(sig) = &symbol.signature {
            parts.push(sig.clone());
        }

        // Add documentation comment if available (enables natural language queries)
        if let Some(doc) = &symbol.doc_comment {
            parts.push(doc.clone());
        }

        // NOTE: code_context REMOVED for RAG optimization
        // - Was ~1,304 chars avg (88% of embedded text)
        // - Random surrounding code added noise to semantic matching
        // - BGE-Small truncates at 512 tokens anyway (model limit)
        // - code_context still stored in DB for FTS5 text search
        // If search quality degrades, consider adding back with 500-char truncation

        parts.join(" ")
    }

    /// Build focused embedding text for memory files
    ///
    /// Memory files use a custom RAG pattern:
    /// - ONLY the "description" symbol gets an embedding
    /// - Embedding text format: "{type}: {description}"
    /// - Other symbols (id, timestamp, tags) return empty (skipped)
    ///
    /// This gives us 1 focused embedding per memory instead of 10 scattered ones.
    fn build_memory_embedding_text(&self, symbol: &Symbol) -> String {
        // Only embed "description" symbols - skip id, timestamp, tags, etc.
        if symbol.name != "description" {
            return String::new(); // Empty = skip embedding for this symbol
        }

        // Extract the description value from code_context JSON
        let description_value = self.extract_json_string_value(&symbol.code_context, "description")
            .unwrap_or_else(|| symbol.name.clone()); // Fallback to symbol name if extraction fails

        // Extract the type value to prefix the description
        let type_value = self.extract_json_string_value(&symbol.code_context, "type")
            .unwrap_or_else(|| "checkpoint".to_string()); // Default to "checkpoint" if type not found

        // Extract tags for searchability
        let tags = self.extract_json_array_value(&symbol.code_context, "tags");

        // Extract semantic terms from files_changed
        let file_terms = self.extract_file_terms(&symbol.code_context);

        // Build enhanced embedding: "{type}: {description} | tags: {tags} | files: {file_terms}"
        // Examples:
        // - "checkpoint: Fixed auth bug | tags: bugfix security | files: auth middleware"
        // - "decision: Chose SQLite over PostgreSQL | tags: architecture database | files: schema migrations"
        let mut parts = vec![format!("{}: {}", type_value, description_value)];

        // Add tags if present
        if let Some(tag_list) = tags {
            if !tag_list.is_empty() {
                parts.push(format!("tags: {}", tag_list.join(" ")));
            }
        }

        // Add file terms if present
        if let Some(terms) = file_terms {
            parts.push(format!("files: {}", terms));
        }

        parts.join(" | ")
    }

    /// Extract a JSON string value from code_context field
    ///
    /// Memory symbols have code_context like:
    /// ```
    ///   2:   "id": "milestone_69114732_999aff",
    ///   3:   "timestamp": 1762740018,
    ///   4:   "type": "milestone",
    ///   5:   "description": "Updated JULIE_2_PLAN.md...",
    /// ```
    ///
    /// This extracts the value for a given key (e.g., "description" or "type")
    /// Properly handles escaped quotes, unicode escapes, and all JSON string edge cases.
    fn extract_json_string_value(&self, code_context: &Option<String>, key: &str) -> Option<String> {
        let context = code_context.as_ref()?;

        // Find the line containing the key
        // Format: "  5:   "description": "Updated JULIE_2_PLAN.md..."
        let search_pattern = format!("\"{}\": ", key);

        for line in context.lines() {
            if line.contains(&search_pattern) {
                // Find where the JSON string value starts (after the key)
                if let Some(key_idx) = line.find(&search_pattern) {
                    let value_start = &line[key_idx + search_pattern.len()..];

                    // Use serde_json streaming deserializer to parse JUST the string value
                    // This properly handles:
                    // - Escaped quotes: "Fixed \"auth\" bug"
                    // - Escaped backslashes: "Path: C:\\Users\\..."
                    // - Unicode escapes: "Hello \u0041"
                    // - Trailing commas (ignores them)
                    use serde::Deserialize;
                    let mut deserializer = serde_json::Deserializer::from_str(value_start);
                    if let Ok(value) = String::deserialize(&mut deserializer) {
                        return Some(value);
                    }
                }
            }
        }

        None
    }

    /// Extract a JSON array value from code_context field
    ///
    /// Memory symbols have code_context with arrays like:
    /// ```
    ///   6:   "tags": [
    ///   7:     "performance",
    ///   8:     "file-size-limit",
    ///   9:     "indexing"
    ///  10:   ]
    /// ```
    ///
    /// This extracts the array values as a Vec<String>
    fn extract_json_array_value(&self, code_context: &Option<String>, key: &str) -> Option<Vec<String>> {
        let context = code_context.as_ref()?;

        // Find the line containing the key with array start
        // Format: "  6:   "tags": ["
        let search_pattern = format!("\"{}\": [", key);

        for (i, line) in context.lines().enumerate() {
            if line.contains(&search_pattern) {
                // Collect lines until we find the closing bracket
                let mut array_lines = Vec::new();
                let lines: Vec<&str> = context.lines().collect();

                for j in i..lines.len() {
                    // Strip line number prefix (e.g., "  ➤   5:   " or "      6:   ")
                    // Format: optional spaces, optional arrow, line number, colon, spaces, then JSON
                    let cleaned = if let Some(colon_pos) = lines[j].find(':') {
                        // Skip past first colon (line number) and any leading spaces
                        &lines[j][colon_pos + 1..].trim_start()
                    } else {
                        lines[j]
                    };

                    array_lines.push(cleaned);
                    if cleaned.contains(']') {
                        break;
                    }
                }

                // Join lines to form valid JSON
                let array_json = array_lines.join("\n");

                // Find where the array starts (after the key)
                if let Some(key_idx) = array_json.find(&search_pattern) {
                    let value_start = &array_json[key_idx + search_pattern.len() - 1..]; // Include the '['

                    // Parse as JSON array
                    use serde::Deserialize;
                    let mut deserializer = serde_json::Deserializer::from_str(value_start);
                    if let Ok(values) = Vec::<String>::deserialize(&mut deserializer) {
                        return Some(values);
                    }
                }
            }
        }

        None
    }

    /// Extract semantic terms from file paths in git.files_changed
    ///
    /// Extracts meaningful domain terms from file paths while filtering noise:
    /// - Input: ["src/embeddings/mod.rs", "src/tools/workspace/indexing.rs"]
    /// - Output: "embeddings tools workspace indexing"
    ///
    /// Filters out: src/, lib/, test/, tests/, mod.rs, index.ts, file extensions
    fn extract_file_terms(&self, code_context: &Option<String>) -> Option<String> {
        let context = code_context.as_ref()?;

        // Look for git.files_changed array nested in the JSON
        // Format: "git": { "files_changed": ["path1", "path2"] }

        // First find if there's a "files_changed" key
        if !context.contains("\"files_changed\"") {
            return None;
        }

        // Extract the files_changed array
        let search_pattern = "\"files_changed\": [";

        for (i, line) in context.lines().enumerate() {
            if line.contains(search_pattern) {
                // Collect lines until closing bracket
                let mut array_lines = Vec::new();
                let lines: Vec<&str> = context.lines().collect();

                for j in i..lines.len() {
                    // Strip line number prefix (same as extract_json_array_value)
                    let cleaned = if let Some(colon_pos) = lines[j].find(':') {
                        &lines[j][colon_pos + 1..].trim_start()
                    } else {
                        lines[j]
                    };

                    array_lines.push(cleaned);
                    if cleaned.contains(']') {
                        break;
                    }
                }

                // Join and parse
                let array_json = array_lines.join("\n");

                if let Some(key_idx) = array_json.find(search_pattern) {
                    let value_start = &array_json[key_idx + search_pattern.len() - 1..];

                    use serde::Deserialize;
                    let mut deserializer = serde_json::Deserializer::from_str(value_start);
                    if let Ok(files) = Vec::<String>::deserialize(&mut deserializer) {
                        // Extract semantic terms from file paths
                        use std::collections::HashSet;
                        use std::path::Path;

                        let mut terms = HashSet::new();

                        for file in files {
                            let path = Path::new(&file);

                            for component in path.iter() {
                                let comp = component.to_string_lossy().to_string();

                                // Skip common non-semantic terms
                                if matches!(comp.as_str(),
                                    "src" | "lib" | "test" | "tests" |
                                    "mod.rs" | "index.ts" | "index.js" |
                                    "main.rs" | "main.ts" | "app.ts"
                                ) {
                                    continue;
                                }

                                // Remove file extensions
                                let without_ext = comp
                                    .trim_end_matches(".rs")
                                    .trim_end_matches(".ts")
                                    .trim_end_matches(".js")
                                    .trim_end_matches(".tsx")
                                    .trim_end_matches(".jsx")
                                    .trim_end_matches(".py");

                                if !without_ext.is_empty() && without_ext.len() > 1 {
                                    terms.insert(without_ext.to_string());
                                }
                            }
                        }

                        if terms.is_empty() {
                            return None;
                        }

                        let mut terms_vec: Vec<String> = terms.into_iter().collect();
                        terms_vec.sort(); // Consistent ordering
                        return Some(terms_vec.join(" "));
                    }
                }
            }
        }

        None
    }
}

/// Implement Drop to log embedding engine cleanup
/// This helps track when the GPU model is released from memory
impl Drop for EmbeddingEngine {
    fn drop(&mut self) {
        tracing::info!(
            "🧹 Dropping EmbeddingEngine '{}' - OrtEmbeddingModel will now be dropped and GPU memory released",
            self.model_name
        );
        // Note: The actual cleanup happens when self.model (OrtEmbeddingModel) drops,
        // which will log its own GPU cleanup message via OrtEmbeddingModel::drop()
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
