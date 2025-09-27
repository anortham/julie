// Vector Store Module
//
// This module provides efficient storage and similarity search for embedding vectors
// using HNSW (Hierarchical Navigable Small World) algorithm for fast nearest neighbor search.

use super::SimilarityResult;
use anyhow::Result;
use std::collections::HashMap;

/// High-performance vector store for embedding similarity search
pub struct VectorStore {
    dimensions: usize,
    vectors: HashMap<String, Vec<f32>>,
    // TODO: Add HNSW index for efficient similarity search
}

impl VectorStore {
    /// Create a new vector store for embeddings of the given dimensions
    pub fn new(dimensions: usize) -> Result<Self> {
        Ok(Self {
            dimensions,
            vectors: HashMap::new(),
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
