// Vector Store Module
//
// This module provides efficient storage and similarity search for embedding vectors
// using HNSW (Hierarchical Navigable Small World) algorithm for fast nearest neighbor search.

use super::SimilarityResult;
use anyhow::{anyhow, Result};
use hnsw_rs::prelude::*; // Includes Hnsw, DistCosine, and other distance metrics
                         // use hnsw_rs::hnswio::*;  // For HnswIo persistence (TODO: fix lifetime issues)
use std::collections::HashMap;
use std::path::Path;

const HNSW_MAX_LAYERS: usize = 16; // hnsw_rs NB_LAYER_MAX; required for dump persistence

use hnsw_rs::hnswio::{HnswIo, ReloadOptions};

/// High-performance vector store for embedding similarity search
pub struct VectorStore {
    dimensions: usize,
    vectors: HashMap<String, Vec<f32>>,
    /// HNSW index for fast approximate nearest neighbor search
    /// Stored alongside HnswIo to satisfy lifetime requirements for disk-loaded indexes
    hnsw_index: Option<Hnsw<'static, f32, DistCosine>>,
    /// HnswIo instance for loading from disk - kept alive to satisfy Hnsw lifetime when using mmap
    /// In non-mmap mode (default), this is None since data is copied into Hnsw
    _hnsw_io: Option<Box<HnswIo>>,
    /// Mapping from HNSW numeric IDs to symbol IDs
    /// Needed because HNSW uses usize indices but we use String symbol IDs
    id_mapping: Vec<String>,
}

impl VectorStore {
    /// Create a new vector store for embeddings of the given dimensions
    pub fn new(dimensions: usize) -> Result<Self> {
        Ok(Self {
            dimensions,
            vectors: HashMap::new(),
            hnsw_index: None,
            _hnsw_io: None,
            id_mapping: Vec::new(),
        })
    }

    /// Store a vector with associated symbol ID
    pub fn store_vector(&mut self, symbol_id: String, vector: Vec<f32>) -> Result<()> {
        if vector.len() != self.dimensions {
            return Err(anyhow::anyhow!(
                "Vector dimensions {} do not match expected {}",
                vector.len(),
                self.dimensions
            ));
        }

        self.vectors.insert(symbol_id, vector);
        Ok(())
    }

    /// Update an existing vector
    pub fn update_vector(&mut self, symbol_id: &str, vector: Vec<f32>) -> Result<()> {
        if vector.len() != self.dimensions {
            return Err(anyhow::anyhow!(
                "Vector dimensions {} do not match expected {}",
                vector.len(),
                self.dimensions
            ));
        }

        self.vectors.insert(symbol_id.to_string(), vector);
        Ok(())
    }

    /// Remove a vector
    pub fn remove_vector(&mut self, symbol_id: &str) -> Result<()> {
        self.vectors.remove(symbol_id);
        Ok(())
    }

    /// Get all vectors (for bulk operations like writing to SQLite)
    pub fn get_all_vectors(&self) -> HashMap<String, Vec<f32>> {
        self.vectors.clone()
    }

    /// Search for similar vectors using cosine similarity
    pub fn search_similar(
        &self,
        query_vector: &[f32],
        limit: usize,
        threshold: f32,
    ) -> Result<Vec<SimilarityResult>> {
        if query_vector.len() != self.dimensions {
            return Err(anyhow::anyhow!(
                "Query vector dimensions {} do not match expected {}",
                query_vector.len(),
                self.dimensions
            ));
        }

        let mut results = Vec::new();

        for (symbol_id, vector) in &self.vectors {
            let similarity = super::cosine_similarity(query_vector, vector);

            if similarity >= threshold {
                results.push(SimilarityResult {
                    symbol_id: symbol_id.clone(),
                    similarity_score: similarity,
                    embedding: vector.clone(),
                });
            }
        }

        // Sort by similarity score (highest first)
        results.sort_by(|a, b| b.similarity_score.partial_cmp(&a.similarity_score).unwrap());

        // Limit results
        results.truncate(limit);

        Ok(results)
    }

    /// Search using HNSW when available, otherwise fall back to brute-force search.
    /// Returns the similarity results and whether the HNSW index was used.
    pub fn search_with_fallback(
        &self,
        query_vector: &[f32],
        limit: usize,
        threshold: f32,
    ) -> Result<(Vec<SimilarityResult>, bool)> {
        if self.has_hnsw_index() {
            match self.search_similar_hnsw(query_vector, limit, threshold) {
                Ok(results) => return Ok((results, true)),
                Err(hnsw_err) => {
                    let fallback = self.search_similar(query_vector, limit, threshold).map_err(
                        |brute_err| {
                            anyhow!(
                                "HNSW search failed ({}), and brute-force fallback also failed ({})",
                                hnsw_err,
                                brute_err
                            )
                        },
                    )?;
                    return Ok((fallback, false));
                }
            }
        }

        Ok((self.search_similar(query_vector, limit, threshold)?, false))
    }

    /// Get the number of stored vectors
    pub fn len(&self) -> usize {
        self.vectors.len()
    }

    /// Check if the store is empty
    pub fn is_empty(&self) -> bool {
        self.vectors.is_empty()
    }

    /// Get the dimensions of vectors stored in this store
    #[allow(dead_code)]
    pub(crate) fn get_dimensions(&self) -> usize {
        self.dimensions
    }

    /// Get vector by symbol ID
    pub fn get_vector(&self, symbol_id: &str) -> Option<&Vec<f32>> {
        self.vectors.get(symbol_id)
    }

    // ========================================================================
    // HNSW Index Methods (TDD Implementation - Start with stubs that fail)
    // ========================================================================

    /// Build HNSW index from stored vectors
    pub fn build_hnsw_index(&mut self) -> Result<()> {
        if self.vectors.is_empty() {
            return Err(anyhow::anyhow!(
                "Cannot build HNSW index: no vectors stored"
            ));
        }

        // HNSW construction parameters (based on hnsw_rs best practices)
        let max_nb_connection = 32; // Typical: 16-64, good balance for code search
        let nb_elem = self.vectors.len();
        // hnsw_rs persistence requires using the full layer budget (NB_LAYER_MAX)
        let nb_layer = HNSW_MAX_LAYERS;
        let ef_construction = 400; // Higher = better quality, slower build (typical: 200-800)

        tracing::debug!(
            "Building HNSW index: {} vectors, {} layers, max_conn={}, ef_c={}",
            nb_elem,
            nb_layer,
            max_nb_connection,
            ef_construction
        );

        // Create HNSW index with cosine distance
        // Note: DistCosine expects pre-normalized vectors
        let mut hnsw = Hnsw::<'static, f32, DistCosine>::new(
            max_nb_connection,
            nb_elem,
            nb_layer,
            ef_construction,
            DistCosine {},
        );

        // Build ID mapping and prepare data for insertion
        // IMPORTANT: Sort by symbol ID for deterministic index building
        // HashMap iteration order is non-deterministic!
        self.id_mapping.clear();
        self.id_mapping.reserve(nb_elem);

        let mut sorted_vectors: Vec<_> = self.vectors.iter().collect();
        sorted_vectors.sort_by(|a, b| a.0.cmp(b.0)); // Sort by symbol ID

        let mut data_for_insertion = Vec::with_capacity(nb_elem);

        for (idx, (symbol_id, vector)) in sorted_vectors.iter().enumerate() {
            self.id_mapping.push((*symbol_id).clone());
            data_for_insertion.push((*vector, idx));
        }

        // Insert all vectors into the index (parallel for performance)
        hnsw.parallel_insert(&data_for_insertion);

        // Set to search mode (required before searching)
        hnsw.set_searching_mode(true);

        // Store the built index
        self.hnsw_index = Some(hnsw);

        tracing::info!(
            "✅ HNSW index built successfully: {} vectors indexed",
            nb_elem
        );
        Ok(())
    }

    /// Check if HNSW index is built
    pub fn has_hnsw_index(&self) -> bool {
        self.hnsw_index.is_some()
    }

    /// Search for similar vectors using HNSW index (fast approximate search)
    pub fn search_similar_hnsw(
        &self,
        query_vector: &[f32],
        limit: usize,
        threshold: f32,
    ) -> Result<Vec<SimilarityResult>> {
        if query_vector.len() != self.dimensions {
            return Err(anyhow::anyhow!(
                "Query vector dimensions {} do not match expected {}",
                query_vector.len(),
                self.dimensions
            ));
        }

        let hnsw = self.hnsw_index.as_ref().ok_or_else(|| {
            anyhow::anyhow!("HNSW index not built. Call build_hnsw_index() first")
        })?;

        // Perform k-NN search
        // ef_search controls search quality (higher = better but slower)
        let ef_search = (limit * 2).max(50); // Search wider than limit for better quality

        let neighbors = hnsw.search(query_vector, limit, ef_search);

        // Convert HNSW results to SimilarityResults
        let mut results = Vec::new();

        for neighbor in neighbors {
            let idx = neighbor.d_id;

            // Map HNSW ID back to symbol ID
            if idx >= self.id_mapping.len() {
                tracing::warn!("HNSW returned invalid ID: {}", idx);
                continue;
            }

            let symbol_id = &self.id_mapping[idx];

            // Get the actual vector for this symbol
            let vector = match self.vectors.get(symbol_id) {
                Some(v) => v,
                None => {
                    tracing::warn!("Symbol ID {} not found in vectors", symbol_id);
                    continue;
                }
            };

            // Calculate actual cosine similarity
            let similarity = super::cosine_similarity(query_vector, vector);

            // Apply threshold filter
            if similarity >= threshold {
                results.push(SimilarityResult {
                    symbol_id: symbol_id.clone(),
                    similarity_score: similarity,
                    embedding: vector.clone(),
                });
            }
        }

        // Results should already be sorted by HNSW, but re-sort to be sure
        results.sort_by(|a, b| b.similarity_score.partial_cmp(&a.similarity_score).unwrap());

        Ok(results)
    }

    /// Save HNSW index to disk using hnsw_rs file_dump
    /// Creates two files: {path}/hnsw_index.hnsw.graph and {path}/hnsw_index.hnsw.data
    pub fn save_hnsw_index(&mut self, path: &Path) -> Result<()> {
        let hnsw = self.hnsw_index.as_mut().ok_or_else(|| {
            anyhow::anyhow!("Cannot save: HNSW index not built. Call build_hnsw_index() first")
        })?;

        // Ensure the directory exists
        std::fs::create_dir_all(path)?;

        // Use "hnsw_index" as the base filename (creates hnsw_index.hnsw.graph + hnsw_index.hnsw.data)
        let filename = "hnsw_index";

        tracing::info!("💾 Saving HNSW index to {}", path.display());
        tracing::debug!(
            "Index has {} vectors, dimensions: {}",
            self.vectors.len(),
            self.dimensions
        );

        // CRITICAL: Disable search mode before dumping to allow write operations
        // The searching flag prevents internal write operations
        hnsw.set_searching_mode(false);
        tracing::debug!("Search mode disabled for dump");

        let dump_result = hnsw.file_dump(path, filename);

        tracing::debug!("file_dump returned: {:?}", dump_result);

        // Re-enable search mode after dumping
        hnsw.set_searching_mode(true);
        tracing::debug!("Search mode re-enabled");

        match dump_result {
            Ok(dumped_file) => {
                tracing::info!("✅ HNSW index saved successfully: {}", dumped_file);
                Ok(())
            }
            Err(e) => {
                tracing::error!("❌ HNSW dump failed with error: {:?}", e);
                Err(anyhow::anyhow!("Failed to save HNSW index: {}", e))
            }
        }
    }

    /// Load HNSW index from disk using hnsw_rs HnswIo
    /// Expects files: {path}/hnsw_index.hnsw.graph and {path}/hnsw_index.hnsw.data
    ///
    /// SAFETY: Uses unsafe transmute to extend lifetime to 'static. This is safe because:
    /// 1. We use ReloadOptions::default() which has datamap: false (no mmap)
    /// 2. With datamap: false, all vector data is copied into Hnsw during load
    /// 3. After load_hnsw returns, Hnsw owns all its data with no references to HnswIo
    /// 4. The lifetime constraint 'b: 'a in load_hnsw is overly conservative for non-mmap case
    pub fn load_hnsw_index(&mut self, path: &Path) -> Result<()> {
        let filename = "hnsw_index";
        let graph_file = path.join(format!("{}.hnsw.graph", filename));
        let data_file = path.join(format!("{}.hnsw.data", filename));

        // Check if persisted index files exist
        if !graph_file.exists() || !data_file.exists() {
            return Err(anyhow::anyhow!(
                "HNSW index files not found at {}. Expected {}.hnsw.graph and {}.hnsw.data",
                path.display(),
                filename,
                filename
            ));
        }

        tracing::info!("📂 Loading HNSW index from disk: {}", path.display());

        // Create HnswIo for loading (with default options = no mmap, data is copied)
        let mut hnsw_io = HnswIo::new(path, filename);
        let reload_options = ReloadOptions::default(); // datamap: false - copies data
        hnsw_io.set_options(reload_options);

        // Load the index - returns Hnsw<'a, ...> where 'a is tied to hnsw_io lifetime
        let loaded_hnsw: Hnsw<'_, f32, DistCosine> = hnsw_io
            .load_hnsw::<f32, DistCosine>()
            .map_err(|e| anyhow::anyhow!("Failed to load HNSW from disk: {}", e))?;

        // SAFETY: With datamap: false, all data is copied into Hnsw.
        // The lifetime 'a -> 'b constraint is overly conservative.
        // We can safely transmute to 'static because Hnsw owns its data.
        let static_hnsw: Hnsw<'static, f32, DistCosine> =
            unsafe { std::mem::transmute(loaded_hnsw) };

        // Load the ID mapping from vectors HashMap keys (sorted for determinism)
        self.id_mapping.clear();
        let mut sorted_ids: Vec<_> = self.vectors.keys().cloned().collect();
        sorted_ids.sort();
        self.id_mapping = sorted_ids;

        tracing::info!(
            "✅ HNSW index loaded from disk with {} vectors",
            self.vectors.len()
        );

        // Store the loaded index
        self.hnsw_index = Some(static_hnsw);

        // Note: hnsw_io is dropped here, but that's safe because data was copied

        Ok(())
    }

    /// Add a vector to existing HNSW index (incremental update)
    /// Note: Requires index rebuild or insert API - TO BE IMPLEMENTED
    pub fn add_vector_to_hnsw(&mut self, _symbol_id: String, _vector: Vec<f32>) -> Result<()> {
        Err(anyhow::anyhow!(
            "HNSW incremental addition not implemented - requires index rebuild"
        ))
    }

    /// Remove a vector from HNSW index
    /// Note: HNSW doesn't support deletion - requires index rebuild
    pub fn remove_vector_from_hnsw(&mut self, _symbol_id: &str) -> Result<()> {
        Err(anyhow::anyhow!(
            "HNSW vector removal not supported - HNSW is immutable after building"
        ))
    }

    /// Clear all in-memory data to release memory
    /// This is critical for avoiding memory leaks after HNSW index is persisted to disk
    ///
    /// Clears:
    /// - All vectors (can be 8GB+ for large codebases)
    /// - HNSW index (large graph structure)
    /// - ID mapping
    ///
    /// Call this after save_hnsw_index() to immediately release ~8GB of RAM
    pub fn clear(&mut self) {
        tracing::info!("🧹 Clearing VectorStore in-memory data to release memory...");

        let vectors_count = self.vectors.len();
        let vectors_bytes = vectors_count * self.dimensions * std::mem::size_of::<f32>();

        self.vectors.clear();
        self.vectors.shrink_to_fit(); // Release the HashMap's backing memory

        self.hnsw_index = None; // Drop the HNSW graph

        self.id_mapping.clear();
        self.id_mapping.shrink_to_fit();

        tracing::info!(
            "✅ Cleared {} vectors (~{:.2} MB) + HNSW index from memory",
            vectors_count,
            vectors_bytes as f64 / 1_048_576.0
        );
    }
}
