// Vector Store Module
//
// This module provides efficient storage and similarity search for embedding vectors
// using HNSW (Hierarchical Navigable Small World) algorithm for fast nearest neighbor search.

use super::SimilarityResult;
use anyhow::Result;
use hnsw_rs::prelude::*; // Includes Hnsw, DistCosine, and other distance metrics
                         // use hnsw_rs::hnswio::*;  // For HnswIo persistence (TODO: fix lifetime issues)
use std::collections::HashMap;
use std::path::Path;

const HNSW_MAX_LAYERS: usize = 16; // hnsw_rs NB_LAYER_MAX; required for dump persistence

use hnsw_rs::hnswio::{HnswIo, ReloadOptions};

/// HNSW Index Manager (SQLite is the single source of truth)
///
/// This is a pure HNSW index manager - it does NOT store vectors in memory.
/// All embedding vectors are stored in SQLite. This structure only manages:
/// - The HNSW graph structure for fast approximate nearest neighbor search
/// - ID mapping between HNSW numeric IDs and symbol_id strings
///
/// During search, vectors are fetched from SQLite on-demand for re-ranking.
pub struct VectorStore {
    dimensions: usize,
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
    /// Create a new HNSW index manager for embeddings of the given dimensions
    pub fn new(dimensions: usize) -> Result<Self> {
        Ok(Self {
            dimensions,
            hnsw_index: None,
            _hnsw_io: None,
            id_mapping: Vec::new(),
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

    /// Build HNSW index from provided embeddings (loaded from SQLite)
    ///
    /// This is the NEW architecture where SQLite is the single source of truth.
    /// No in-memory HashMap needed - embeddings are loaded from SQLite, HNSW is built,
    /// and then the embeddings can be discarded from memory.
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
        self.id_mapping.clear();
        self.id_mapping.reserve(nb_elem);

        let mut sorted_vectors: Vec<_> = embeddings.iter().collect();
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

            // 🔧 REFACTOR: Fetch vector from SQLite instead of HashMap
            // This is the key change - SQLite is now the single source of truth
            let vector = match db.get_embedding_for_symbol(symbol_id, model_name)? {
                Some(v) => v,
                None => {
                    tracing::warn!("Symbol ID {} not found in database", symbol_id);
                    continue;
                }
            };

            // Calculate actual cosine similarity
            let similarity = super::cosine_similarity(query_vector, &vector);

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
    /// Creates three files:
    /// - {path}/hnsw_index.hnsw.graph
    /// - {path}/hnsw_index.hnsw.data
    /// - {path}/hnsw_index.id_mapping.json (NEW: symbol_id array)
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
            self.id_mapping.len(),
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

                // Save id_mapping alongside HNSW files
                self.save_id_mapping(path)?;

                Ok(())
            }
            Err(e) => {
                tracing::error!("❌ HNSW dump failed with error: {:?}", e);
                Err(anyhow::anyhow!("Failed to save HNSW index: {}", e))
            }
        }
    }

    /// Save id_mapping to JSON file alongside HNSW index
    /// Creates {path}/hnsw_index.id_mapping.json
    fn save_id_mapping(&self, path: &Path) -> Result<()> {
        let mapping_file = path.join("hnsw_index.id_mapping.json");
        let json = serde_json::to_string(&self.id_mapping)?;
        std::fs::write(&mapping_file, json)?;
        tracing::debug!("💾 Saved id_mapping with {} entries", self.id_mapping.len());
        Ok(())
    }

    /// Load id_mapping from JSON file
    /// Expects {path}/hnsw_index.id_mapping.json
    fn load_id_mapping(&mut self, path: &Path) -> Result<()> {
        let mapping_file = path.join("hnsw_index.id_mapping.json");

        if !mapping_file.exists() {
            return Err(anyhow::anyhow!(
                "ID mapping file not found at {}",
                mapping_file.display()
            ));
        }

        let json = std::fs::read_to_string(&mapping_file)?;
        self.id_mapping = serde_json::from_str(&json)?;
        tracing::debug!(
            "📂 Loaded id_mapping with {} entries",
            self.id_mapping.len()
        );
        Ok(())
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

        // Load the ID mapping from persisted JSON file
        self.load_id_mapping(path)?;

        tracing::info!(
            "✅ HNSW index loaded from disk with {} symbol mappings",
            self.id_mapping.len()
        );

        // Store the loaded index
        self.hnsw_index = Some(static_hnsw);

        // Note: hnsw_io is dropped here, but that's safe because data was copied

        Ok(())
    }

    /// Insert multiple vectors into existing HNSW index incrementally
    /// This is the primary method for real-time updates - maintains id_mapping and inserts vectors
    /// without rebuilding the entire index.
    pub fn insert_batch(&mut self, embeddings: &[(String, Vec<f32>)]) -> Result<()> {
        if embeddings.is_empty() {
            return Ok(());
        }

        let hnsw = self.hnsw_index.as_mut().ok_or_else(|| {
            anyhow::anyhow!("HNSW index not built. Call build_hnsw_index() first")
        })?;

        tracing::debug!(
            "Inserting {} new vectors into HNSW index incrementally",
            embeddings.len()
        );

        for (symbol_id, vector) in embeddings {
            // Validate dimensions
            if vector.len() != self.dimensions {
                tracing::warn!(
                    "Skipping symbol {} - vector dimensions {} don't match expected {}",
                    symbol_id,
                    vector.len(),
                    self.dimensions
                );
                continue;
            }

            // Get next index and append to mapping
            let idx = self.id_mapping.len();
            self.id_mapping.push(symbol_id.clone());

            // Insert into HNSW with the new index
            // Note: HNSW insert() takes (&[f32], usize) format
            hnsw.insert((vector.as_slice(), idx));
        }

        tracing::debug!(
            "✅ Successfully inserted {} vectors into HNSW index (total: {})",
            embeddings.len(),
            self.id_mapping.len()
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
        self.hnsw_index = None;
        self.id_mapping.clear();
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

        let mapping_count = self.id_mapping.len();

        self.hnsw_index = None; // Drop the HNSW graph

        self.id_mapping.clear();
        self.id_mapping.shrink_to_fit();

        tracing::info!(
            "✅ Cleared HNSW index ({} symbol mappings) from memory",
            mapping_count
        );
    }
}
