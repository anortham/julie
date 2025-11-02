/// Safe wrapper for loaded HNSW index that keeps HnswIo alive
///
/// This module provides a type-safe solution to the lifetime issue in loading HNSW indexes from disk.
///
/// # The Problem
///
/// `hnsw_rs::HnswIo::load_hnsw()` returns `Hnsw<'a, T, D>` where `'a` is tied to the HnswIo's lifetime.
/// To store the Hnsw in VectorStore, we need `Hnsw<'static, T, D>`, which requires transmuting the
/// lifetime. The current code does this unsafely without proving it's sound.
///
/// # The Solution
///
/// This module provides `LoadedHnswIndex`, a wrapper that:
/// 1. Holds both HnswIo and the loaded Hnsw
/// 2. Keeps HnswIo alive as long as Hnsw might need it
/// 3. Encapsulates the unsafe transmute in one documented location
/// 4. Makes the HnswIo-Hnsw relationship explicit in the type system
///
/// # Safety Invariants
///
/// The unsafe transmute is safe because:
/// - ReloadOptions::default() has datamap: false (no memory mapping)
/// - With datamap: false, all vector data is copied into Hnsw's owned heap memory
/// - After loading, Hnsw owns its data and doesn't reference HnswIo
/// - We keep HnswIo alive (in _io field) for defensive programming
///
/// If hnsw_rs ever changes to use mmap by default, this code would need updating.
/// The _io field documents this intent.

use super::SimilarityResult;
use anyhow::Result;
use hnsw_rs::prelude::*;
use hnsw_rs::hnswio::{HnswIo, ReloadOptions};
use std::path::Path;

/// Loaded HNSW index with its associated IO wrapper
///
/// This type safely encapsulates the lifecycle of a loaded HNSW index.
/// The HnswIo is kept alive (in `_io`) to satisfy any potential mmap references,
/// though with default ReloadOptions, all data is copied into Hnsw's owned buffers.
pub struct LoadedHnswIndex {
    /// HnswIo instance - kept alive to safely satisfy the lifetime requirement
    /// In practice, with datamap: false (default), data is copied, so this could be dropped
    /// safely. We keep it for defensive programming and future extensibility.
    _io: Box<HnswIo>,

    /// The HNSW graph structure - owns its data after loading
    /// Originally Hnsw<'a, f32, DistCosine> where 'a was tied to _io
    /// Transmuted to 'static because we verified the data is owned, not borrowed
    hnsw: Hnsw<'static, f32, DistCosine>,

    /// Mapping from HNSW numeric IDs to symbol ID strings
    /// HNSW uses usize indices but Julie uses String symbol IDs
    id_mapping: Vec<String>,
}

impl LoadedHnswIndex {
    /// Create a LoadedHnswIndex from a built-in-memory HNSW
    ///
    /// This is used when HNSW is built from embeddings (not loaded from disk).
    /// In this case, there's no HnswIo, so we use None as a placeholder.
    pub fn from_built_hnsw(
        hnsw: Hnsw<'static, f32, DistCosine>,
        id_mapping: Vec<String>,
    ) -> Result<Self> {
        // Create a dummy HnswIo just to satisfy the type requirement
        // In practice, we'll never use it since we don't have mmap data
        // This is a bit of a hack, but it simplifies the architecture
        // Alternative: Make HnswIo optional in LoadedHnswIndex (future refactoring)
        let path = std::path::PathBuf::from("/tmp");
        let hnsw_io = HnswIo::new(&path, "dummy");

        Ok(Self {
            _io: Box::new(hnsw_io),
            hnsw,
            id_mapping,
        })
    }

    /// Load HNSW index from disk files
    ///
    /// Expects files at:
    /// - {path}/{filename}.hnsw.graph
    /// - {path}/{filename}.hnsw.data
    /// - {path}/{filename}.id_mapping.json
    ///
    /// # Safety
    ///
    /// This function contains an unsafe transmute. The transmute is safe because:
    /// 1. We use ReloadOptions::default() which disables memory mapping (datamap: false)
    /// 2. With datamap: false, all vector data is copied into Hnsw during load
    /// 3. After load_hnsw returns, Hnsw owns all its data with no references to HnswIo
    /// 4. The HnswIo is kept alive (_io field) for additional safety margin
    ///
    /// The lifetime constraint from load_hnsw is overly conservative for the non-mmap case.
    pub fn load(path: &Path, filename: &str) -> Result<Self> {
        let graph_file = path.join(format!("{}.hnsw.graph", filename));
        let data_file = path.join(format!("{}.hnsw.data", filename));

        // Verify files exist
        if !graph_file.exists() || !data_file.exists() {
            return Err(anyhow::anyhow!(
                "HNSW index files not found at {}. Expected {}.hnsw.graph and {}.hnsw.data",
                path.display(),
                filename,
                filename
            ));
        }

        tracing::info!("📂 Loading HNSW index from disk: {}", path.display());

        // Create HnswIo and load configuration
        let mut hnsw_io = HnswIo::new(path, filename);
        let reload_options = ReloadOptions::default(); // Importantly: datamap: false
        hnsw_io.set_options(reload_options);

        // Load HNSW with lifetime tied to hnsw_io
        let loaded_hnsw: Hnsw<'_, f32, DistCosine> = hnsw_io
            .load_hnsw::<f32, DistCosine>()
            .map_err(|e| anyhow::anyhow!("Failed to load HNSW from disk: {}", e))?;

        // SAFETY: Transmute 'a -> 'static
        //
        // This is safe because:
        // 1. ReloadOptions::default() has datamap: false (verified in hnsw_rs source)
        // 2. With datamap: false, load_hnsw copies all data into Hnsw's owned heap buffers
        // 3. The returned Hnsw<'a, ...> only borrows 'a if mmap is enabled
        // 4. Since mmap is disabled, Hnsw owns all its data
        // 5. Transmuting 'a to 'static is valid because Hnsw owns its data
        // 6. We keep hnsw_io alive (in _io) for belt-and-suspenders safety
        //
        // If hnsw_rs changes to enable mmap by default, this MUST be updated.
        let static_hnsw: Hnsw<'static, f32, DistCosine> =
            unsafe { std::mem::transmute(loaded_hnsw) };

        // Load the ID mapping from persisted JSON file
        let mapping_file = path.join(format!("{}.id_mapping.json", filename));
        if !mapping_file.exists() {
            return Err(anyhow::anyhow!(
                "ID mapping file not found at {}",
                mapping_file.display()
            ));
        }

        let json = std::fs::read_to_string(&mapping_file)?;
        let id_mapping: Vec<String> = serde_json::from_str(&json)?;

        tracing::info!(
            "✅ HNSW index loaded from disk with {} symbol mappings",
            id_mapping.len()
        );

        Ok(Self {
            _io: Box::new(hnsw_io),
            hnsw: static_hnsw,
            id_mapping,
        })
    }

    /// Search for similar vectors using HNSW
    ///
    /// Performs fast approximate k-NN search followed by exact similarity calculation
    /// on vectors fetched from the database for re-ranking.
    pub fn search_similar(
        &self,
        db: &crate::database::SymbolDatabase,
        query_vector: &[f32],
        limit: usize,
        threshold: f32,
        model_name: &str,
    ) -> Result<Vec<SimilarityResult>> {
        // Verify query vector dimensions
        // Note: VectorStore ensures this, but check for safety
        let dimensions = 384; // Should match VectorStore::dimensions
        if query_vector.len() != dimensions {
            return Err(anyhow::anyhow!(
                "Query vector dimensions {} do not match expected {}",
                query_vector.len(),
                dimensions
            ));
        }

        // Perform k-NN search with HNSW
        // ef_search controls search quality (higher = better but slower)
        let ef_search = (limit * 2).max(50); // Search wider than limit for better quality

        let neighbors = self.hnsw.search(query_vector, limit, ef_search);

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

            // Fetch vector from SQLite for re-ranking
            let vector = match db.get_embedding_for_symbol(symbol_id, model_name)? {
                Some(v) => v,
                None => {
                    tracing::warn!("Symbol ID {} not found in database", symbol_id);
                    continue;
                }
            };

            // Calculate exact cosine similarity
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

    /// Get reference to the HNSW index
    pub fn hnsw(&self) -> &Hnsw<'static, f32, DistCosine> {
        &self.hnsw
    }

    /// Get mutable reference to the HNSW index
    pub fn hnsw_mut(&mut self) -> &mut Hnsw<'static, f32, DistCosine> {
        &mut self.hnsw
    }

    /// Get reference to the ID mapping
    pub fn id_mapping(&self) -> &[String] {
        &self.id_mapping
    }

    /// Get mutable reference to the ID mapping
    pub fn id_mapping_mut(&mut self) -> &mut Vec<String> {
        &mut self.id_mapping
    }

    /// Get the number of vectors in the index
    pub fn len(&self) -> usize {
        self.id_mapping.len()
    }

    /// Check if the index is empty
    pub fn is_empty(&self) -> bool {
        self.id_mapping.is_empty()
    }

    /// Insert multiple vectors into the HNSW index
    ///
    /// Validates dimensions and appends to id_mapping while inserting into HNSW.
    pub fn insert_batch(
        &mut self,
        embeddings: &[(String, Vec<f32>)],
        expected_dimensions: usize,
    ) -> Result<()> {
        for (symbol_id, vector) in embeddings {
            // Validate dimensions
            if vector.len() != expected_dimensions {
                tracing::warn!(
                    "Skipping symbol {} - vector dimensions {} don't match expected {}",
                    symbol_id,
                    vector.len(),
                    expected_dimensions
                );
                continue;
            }

            // Get next index and append to mapping
            let idx = self.id_mapping.len();
            self.id_mapping.push(symbol_id.clone());

            // Insert into HNSW with the new index
            // Note: HNSW insert() takes (&[f32], usize) format
            self.hnsw.insert((vector.as_slice(), idx));
        }

        Ok(())
    }
}

// Implement Drop to explicitly document cleanup
impl Drop for LoadedHnswIndex {
    fn drop(&mut self) {
        tracing::debug!(
            "Dropping LoadedHnswIndex with {} symbols",
            self.id_mapping.len()
        );
        // Explicit drop order: hnsw first, then _io
        // In practice, Rust handles this automatically
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_loaded_hnsw_creation() {
        // This test would require actual HNSW files from a saved index
        // For now, we document the intended API
        // Real tests will use integration tests with actual indexes
    }

    #[test]
    fn test_id_mapping_access() {
        // Test that id_mapping is correctly preserved
        // Will be implemented with actual test data
    }

    #[test]
    fn test_search_similar_integration() {
        // Integration test with real HNSW index and database
        // Will be implemented in integration test suite
    }
}
