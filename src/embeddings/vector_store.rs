// Vector Store Module
//
// This module provides efficient storage and similarity search for embedding vectors
// using HNSW (Hierarchical Navigable Small World) algorithm for fast nearest neighbor search.

use super::SimilarityResult;
use anyhow::Result;
use hnsw_rs::prelude::*;  // Includes Hnsw, DistCosine, and other distance metrics
use std::collections::HashMap;
use std::path::Path;

/// High-performance vector store for embedding similarity search
pub struct VectorStore {
    dimensions: usize,
    vectors: HashMap<String, Vec<f32>>,
    /// HNSW index for fast approximate nearest neighbor search
    /// Note: Using 'static lifetime since index owns its data
    hnsw_index: Option<Hnsw<'static, f32, DistCosine>>,
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

    /// Get the number of stored vectors
    pub fn len(&self) -> usize {
        self.vectors.len()
    }

    /// Check if the store is empty
    pub fn is_empty(&self) -> bool {
        self.vectors.is_empty()
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
            return Err(anyhow::anyhow!("Cannot build HNSW index: no vectors stored"));
        }

        // HNSW construction parameters (based on hnsw_rs best practices)
        let max_nb_connection = 32;  // Typical: 16-64, good balance for code search
        let nb_elem = self.vectors.len();
        let nb_layer = 16.min((nb_elem as f32).ln().trunc() as usize);
        let ef_construction = 400;  // Higher = better quality, slower build (typical: 200-800)

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
        sorted_vectors.sort_by(|a, b| a.0.cmp(b.0));  // Sort by symbol ID

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

        tracing::info!("✅ HNSW index built successfully: {} vectors indexed", nb_elem);
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
        let ef_search = (limit * 2).max(50);  // Search wider than limit for better quality

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

    /// Save HNSW index to disk
    /// Note: Requires bincode serialization - TO BE IMPLEMENTED
    pub fn save_hnsw_index(&self, _path: &Path) -> Result<()> {
        Err(anyhow::anyhow!("HNSW persistence not yet implemented - requires hnswio module integration"))
    }

    /// Load HNSW index from disk
    /// Note: Requires bincode deserialization - TO BE IMPLEMENTED
    pub fn load_hnsw_index(&mut self, _path: &Path) -> Result<()> {
        Err(anyhow::anyhow!("HNSW loading not yet implemented - requires hnswio module integration"))
    }

    /// Add a vector to existing HNSW index (incremental update)
    /// Note: Requires index rebuild or insert API - TO BE IMPLEMENTED
    pub fn add_vector_to_hnsw(&mut self, _symbol_id: String, _vector: Vec<f32>) -> Result<()> {
        Err(anyhow::anyhow!("HNSW incremental addition not implemented - requires index rebuild"))
    }

    /// Remove a vector from HNSW index
    /// Note: HNSW doesn't support deletion - requires index rebuild
    pub fn remove_vector_from_hnsw(&mut self, _symbol_id: &str) -> Result<()> {
        Err(anyhow::anyhow!("HNSW vector removal not supported - HNSW is immutable after building"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vector_store_creation() {
        let store = VectorStore::new(384).unwrap();
        assert_eq!(store.dimensions, 384);
        assert!(store.is_empty());
    }

    #[test]
    fn test_store_and_retrieve_vector() {
        let mut store = VectorStore::new(3).unwrap();
        let vector = vec![1.0, 0.0, 0.0];

        store
            .store_vector("test-symbol".to_string(), vector.clone())
            .unwrap();

        assert_eq!(store.len(), 1);
        assert_eq!(store.get_vector("test-symbol"), Some(&vector));
    }

    #[test]
    fn test_dimension_validation() {
        let mut store = VectorStore::new(3).unwrap();
        let wrong_vector = vec![1.0, 0.0]; // Wrong dimensions

        let result = store.store_vector("test".to_string(), wrong_vector);
        assert!(result.is_err());
    }

    #[test]
    fn test_similarity_search() {
        let mut store = VectorStore::new(3).unwrap();

        // Store some test vectors
        store
            .store_vector("similar1".to_string(), vec![1.0, 0.0, 0.0])
            .unwrap();
        store
            .store_vector("similar2".to_string(), vec![0.9, 0.1, 0.0])
            .unwrap();
        store
            .store_vector("different".to_string(), vec![0.0, 1.0, 0.0])
            .unwrap();

        // Search for similar vectors
        let query = vec![1.0, 0.0, 0.0];
        let results = store.search_similar(&query, 10, 0.5).unwrap();

        // Should find the similar vectors
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].symbol_id, "similar1");
        assert!(results[0].similarity_score > 0.9);
    }

    #[test]
    fn test_update_and_remove_vector() {
        let mut store = VectorStore::new(3).unwrap();

        store
            .store_vector("test".to_string(), vec![1.0, 0.0, 0.0])
            .unwrap();
        assert_eq!(store.len(), 1);

        // Update the vector
        store.update_vector("test", vec![0.0, 1.0, 0.0]).unwrap();
        assert_eq!(store.get_vector("test"), Some(&vec![0.0, 1.0, 0.0]));

        // Remove the vector
        store.remove_vector("test").unwrap();
        assert!(store.is_empty());
    }
}
