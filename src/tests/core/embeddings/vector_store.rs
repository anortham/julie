// Vector Store Inline Tests
//
// Tests extracted from src/embeddings/vector_store.rs
// These tests verify the VectorStore implementation including storage,
// retrieval, similarity search, and HNSW index operations.

#[cfg(test)]
mod tests {
    use crate::embeddings::vector_store::VectorStore;
    use tempfile::TempDir;

    #[test]
    fn test_vector_store_creation() {
        let store = VectorStore::new(384).unwrap();
        assert_eq!(store.get_dimensions(), 384);
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

    #[test]
    fn test_save_hnsw_index_persists_files() {
        let mut store = VectorStore::new(3).unwrap();

        store
            .store_vector("a".to_string(), vec![1.0, 0.0, 0.0])
            .unwrap();
        store
            .store_vector("b".to_string(), vec![0.0, 1.0, 0.0])
            .unwrap();
        store
            .store_vector("c".to_string(), vec![0.0, 0.0, 1.0])
            .unwrap();

        store.build_hnsw_index().unwrap();

        let temp_dir = TempDir::new().unwrap();

        let result = store.save_hnsw_index(temp_dir.path());

        assert!(
            result.is_ok(),
            "expected save_hnsw_index to succeed but it returned {:?}",
            result.err()
        );

        let graph_path = temp_dir.path().join("hnsw_index.hnsw.graph");
        let data_path = temp_dir.path().join("hnsw_index.hnsw.data");

        assert!(graph_path.exists(), "graph file missing");
        assert!(data_path.exists(), "data file missing");

        let graph_len = std::fs::metadata(&graph_path).unwrap().len();
        let data_len = std::fs::metadata(&data_path).unwrap().len();

        assert!(graph_len > 0, "graph file should contain data");
        assert!(data_len > 0, "data file should contain vectors");
    }
}
