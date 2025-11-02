// Vector Store Module
//
// This module provides efficient storage and similarity search for embedding vectors
// using HNSW (Hierarchical Navigable Small World) algorithm for fast nearest neighbor search.

use super::SimilarityResult;
use crate::embeddings::LoadedHnswIndex;
use anyhow::Result;
use hnsw_rs::prelude::*; // Includes Hnsw, DistCosine, and other distance metrics
use std::collections::HashMap;
use std::path::Path;

const HNSW_MAX_LAYERS: usize = 16; // hnsw_rs NB_LAYER_MAX; required for dump persistence

/// HNSW Index Manager (SQLite is the single source of truth)
///
/// This is a pure HNSW index manager - it does NOT store vectors in memory.
/// All embedding vectors are stored in SQLite. This structure only manages:
/// - The HNSW graph structure for fast approximate nearest neighbor search
/// - ID mapping between HNSW numeric IDs and symbol_id strings
/// - Safe lifecycle management of loaded indexes via LoadedHnswIndex
///
/// During search, vectors are fetched from SQLite on-demand for re-ranking.
///
/// # Lifetime Safety
///
/// This implementation previously used an unsafe transmute to extend lifetimes.
/// Now we use LoadedHnswIndex, which safely encapsulates the HNSW-HnswIo relationship.
pub struct VectorStore {
    dimensions: usize,
    /// Loaded HNSW index with its IO wrapper (safe lifecycle management)
    /// LoadedHnswIndex keeps HnswIo alive alongside Hnsw to satisfy lifetime requirements
    loaded_index: Option<LoadedHnswIndex>,
}

impl VectorStore {
    /// Create a new HNSW index manager for embeddings of the given dimensions
    pub fn new(dimensions: usize) -> Result<Self> {
        Ok(Self {
            dimensions,
            loaded_index: None,
        })
    }

    /// Get the dimensions of vectors stored in this store
    #[allow(dead_code)]
    pub(crate) fn get_dimensions(&self) -> usize {
        self.dimensions
    }

    // ========================================================================
    // HNSW Index Methods (TDD Implementation - Start with stubs that fail)
    // ========================================================================

    /// Build HNSW index from provided embeddings (in-memory)
    ///
    /// This is used during initial indexing to build HNSW from embeddings loaded from SQLite.
    /// After building, the index can be saved to disk using save_hnsw_index().
    ///
    /// The HNSW is built in-memory using a direct builder pattern (not using HnswIo),
    /// so there's no lifetime issue - the built HNSW owns all its data.
    pub fn build_hnsw_index(&mut self, embeddings: &HashMap<String, Vec<f32>>) -> Result<()> {
        if embeddings.is_empty() {
            return Err(anyhow::anyhow!(
                "Cannot build HNSW index: no embeddings provided"
            ));
        }

        // HNSW construction parameters (based on hnsw_rs best practices)
        let max_nb_connection = 32; // Typical: 16-64, good balance for code search
        let nb_elem = embeddings.len();
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
        let mut id_mapping = Vec::with_capacity(nb_elem);

        let mut sorted_vectors: Vec<_> = embeddings.iter().collect();
        sorted_vectors.sort_by(|a, b| a.0.cmp(b.0)); // Sort by symbol ID

        let mut data_for_insertion = Vec::with_capacity(nb_elem);

        for (idx, (symbol_id, vector)) in sorted_vectors.iter().enumerate() {
            id_mapping.push((*symbol_id).clone());
            data_for_insertion.push((*vector, idx));
        }

        // Insert all vectors into the index (parallel for performance)
        hnsw.parallel_insert(&data_for_insertion);

        // Set to search mode (required before searching)
        hnsw.set_searching_mode(true);

        // Wrap the built HNSW in a temporary LoadedHnswIndex-like structure
        // Since we built it in-memory, we don't have HnswIo, so we create a minimal wrapper
        // Note: This is a temporary in-memory index used until it's saved to disk
        self.loaded_index = Some(LoadedHnswIndex::from_built_hnsw(hnsw, id_mapping)?);

        tracing::info!(
            "✅ HNSW index built successfully: {} vectors indexed",
            nb_elem
        );
        Ok(())
    }

    /// Check if HNSW index is built/loaded
    pub fn has_hnsw_index(&self) -> bool {
        self.loaded_index.is_some()
    }

    /// Search for similar vectors using HNSW index (fast approximate search)
    ///
    /// NEW ARCHITECTURE: Vectors are fetched from SQLite on-demand for re-ranking.
    /// HNSW provides fast approximate k-NN search, then we fetch actual vectors
    /// from the database to calculate exact cosine similarity for re-ranking.
    pub fn search_similar_hnsw(
        &self,
        db: &crate::database::SymbolDatabase,
        query_vector: &[f32],
        limit: usize,
        threshold: f32,
        model_name: &str,
    ) -> Result<Vec<SimilarityResult>> {
        if query_vector.len() != self.dimensions {
            return Err(anyhow::anyhow!(
                "Query vector dimensions {} do not match expected {}",
                query_vector.len(),
                self.dimensions
            ));
        }

        let index = self.loaded_index.as_ref().ok_or_else(|| {
            anyhow::anyhow!("HNSW index not loaded. Call load_hnsw_index() first")
        })?;

        // Delegate to LoadedHnswIndex for the actual search
        index.search_similar(db, query_vector, limit, threshold, model_name)
    }

    /// Save HNSW index to disk using hnsw_rs file_dump
    /// Creates three files:
    /// - {path}/hnsw_index.hnsw.graph
    /// - {path}/hnsw_index.hnsw.data
    /// - {path}/hnsw_index.id_mapping.json (symbol_id array)
    pub fn save_hnsw_index(&mut self, path: &Path) -> Result<()> {
        let index = self.loaded_index.as_mut().ok_or_else(|| {
            anyhow::anyhow!("Cannot save: HNSW index not loaded. Call load_hnsw_index() first")
        })?;

        // Ensure the directory exists
        std::fs::create_dir_all(path)?;

        // Use "hnsw_index" as the base filename (creates hnsw_index.hnsw.graph + hnsw_index.hnsw.data)
        let filename = "hnsw_index";

        tracing::info!("💾 Saving HNSW index to {}", path.display());
        tracing::debug!(
            "Index has {} vectors, dimensions: {}",
            index.len(),
            self.dimensions
        );

        let hnsw = index.hnsw_mut();

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

                // Save id_mapping alongside HNSW files
                let mapping_file = path.join(format!("{}.id_mapping.json", filename));
                let json = serde_json::to_string(index.id_mapping())?;
                std::fs::write(&mapping_file, json)?;
                tracing::debug!("💾 Saved id_mapping with {} entries", index.len());

                Ok(())
            }
            Err(e) => {
                tracing::error!("❌ HNSW dump failed with error: {:?}", e);
                Err(anyhow::anyhow!("Failed to save HNSW index: {}", e))
            }
        }
    }

    /// Load HNSW index from disk
    /// Expects files: {path}/hnsw_index.hnsw.graph, {path}/hnsw_index.hnsw.data,
    /// and {path}/hnsw_index.id_mapping.json
    ///
    /// Uses LoadedHnswIndex for safe lifetime management. The LoadedHnswIndex
    /// keeps HnswIo alive alongside the Hnsw to satisfy any lifetime requirements
    /// and safely encapsulates the unsafe transmute needed for disk-loaded indexes.
    pub fn load_hnsw_index(&mut self, path: &Path) -> Result<()> {
        let filename = "hnsw_index";
        let index = LoadedHnswIndex::load(path, filename)?;
        self.loaded_index = Some(index);
        Ok(())
    }

    /// Insert multiple vectors into existing HNSW index incrementally
    /// This is the primary method for real-time updates - maintains id_mapping and inserts vectors
    /// without rebuilding the entire index.
    pub fn insert_batch(&mut self, embeddings: &[(String, Vec<f32>)]) -> Result<()> {
        if embeddings.is_empty() {
            return Ok(());
        }

        let index = self.loaded_index.as_mut().ok_or_else(|| {
            anyhow::anyhow!("HNSW index not loaded. Call load_hnsw_index() first")
        })?;

        tracing::debug!(
            "Inserting {} new vectors into HNSW index incrementally",
            embeddings.len()
        );

        // Use insert_batch method on LoadedHnswIndex to avoid borrow checker issues
        index.insert_batch(&embeddings, self.dimensions)?;

        tracing::debug!(
            "✅ Successfully inserted {} vectors into HNSW index (total: {})",
            embeddings.len(),
            index.len()
        );

        Ok(())
    }

    /// Add a single vector to existing HNSW index (incremental update)
    /// For batch operations, use insert_batch() instead for better performance
    pub fn add_vector_to_hnsw(&mut self, symbol_id: String, vector: Vec<f32>) -> Result<()> {
        // Delegate to batch method
        self.insert_batch(&[(symbol_id, vector)])
    }

    /// Remove a vector from HNSW index
    /// Note: HNSW doesn't support deletion - requires index rebuild
    pub fn remove_vector_from_hnsw(&mut self, _symbol_id: &str) -> Result<()> {
        Err(anyhow::anyhow!(
            "HNSW vector removal not supported - HNSW is immutable after building"
        ))
    }

    /// Clear the HNSW index
    ///
    /// This is used when HNSW index rebuild fails or needs to be invalidated.
    /// After calling this, the HNSW index must be rebuilt from SQLite before searching.
    pub fn clear_hnsw_index(&mut self) {
        self.loaded_index = None;
        tracing::warn!("⚠️  HNSW index cleared. Must rebuild from SQLite before searching.");
    }

    /// Clear all in-memory index data to release memory
    ///
    /// Clears:
    /// - HNSW index (large graph structure)
    /// - ID mapping
    ///
    /// Note: Since VectorStore no longer stores embeddings in memory,
    /// this only clears the HNSW graph structure (~11MB for typical workspace).
    /// All embedding data lives in SQLite.
    pub fn clear(&mut self) {
        tracing::info!("🧹 Clearing VectorStore in-memory index data...");

        if let Some(index) = self.loaded_index.as_ref() {
            let mapping_count = index.len();
            tracing::info!(
                "✅ Cleared HNSW index ({} symbol mappings) from memory",
                mapping_count
            );
        }

        self.loaded_index = None; // Drop the loaded index
    }
}
