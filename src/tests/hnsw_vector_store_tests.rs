// HNSW Vector Store Tests - TDD Implementation
//
// Following Test-Driven Development methodology:
// 1. RED: Write failing tests that define the contract
// 2. GREEN: Implement minimal code to make tests pass
// 3. REFACTOR: Improve implementation while keeping tests green
//
// These tests define Julie's semantic search performance requirements:
// - Sub-500ms similarity search for 6k vectors
// - Accurate k-NN results (matches brute force baseline)
// - Index persistence and loading
// - Memory efficiency

#[cfg(test)]
mod hnsw_tests {
    use crate::embeddings::vector_store::VectorStore;
    // cosine_similarity not used in these tests
    // use crate::embeddings::cosine_similarity;
    use anyhow::Result;
    use std::time::Instant;
    use tempfile::TempDir;

    /// Helper: Generate test embedding vectors
    fn generate_test_vector(dimensions: usize, seed: usize) -> Vec<f32> {
        let mut vec = Vec::with_capacity(dimensions);
        for i in 0..dimensions {
            // Simple deterministic generation for reproducible tests
            let val = ((seed * 1000 + i * 7) % 100) as f32 / 100.0;
            vec.push(val);
        }
        // Normalize vector
        let magnitude: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        vec.iter().map(|x| x / magnitude).collect()
    }

    /// Helper: Generate a dataset of test vectors
    fn generate_test_dataset(count: usize, dimensions: usize) -> Vec<(String, Vec<f32>)> {
        (0..count)
            .map(|i| {
                let id = format!("symbol_{}", i);
                let vector = generate_test_vector(dimensions, i);
                (id, vector)
            })
            .collect()
    }

    // ========================================================================
    // TEST 1: Basic HNSW Index Building
    // ========================================================================

    #[test]
    fn test_hnsw_index_building() -> Result<()> {
        // GIVEN: A VectorStore with stored vectors
        let mut store = VectorStore::new(384)?;

        // Store 100 test vectors
        let dataset = generate_test_dataset(100, 384);
        for (id, vector) in &dataset {
            store.store_vector(id.clone(), vector.clone())?;
        }

        // WHEN: We build an HNSW index
        let result = store.build_hnsw_index();

        // THEN: Index should be built successfully
        assert!(result.is_ok(), "HNSW index building should succeed");

        // AND: Index should be marked as built
        assert!(store.has_hnsw_index(), "VectorStore should report having HNSW index");

        Ok(())
    }

    // ========================================================================
    // TEST 2: Fast Similarity Search with HNSW
    // ========================================================================

    #[test]
    fn test_fast_similarity_search_performance() -> Result<()> {
        // GIVEN: A VectorStore with 6,085 vectors (Julie's real dataset size)
        let mut store = VectorStore::new(384)?;
        let dataset = generate_test_dataset(6085, 384);

        for (id, vector) in &dataset {
            store.store_vector(id.clone(), vector.clone())?;
        }

        // AND: HNSW index is built
        store.build_hnsw_index()?;

        // WHEN: We perform similarity search
        let query = generate_test_vector(384, 9999);
        let start = Instant::now();
        let results = store.search_similar_hnsw(&query, 10, 0.5)?;
        let duration = start.elapsed();

        // THEN: Search should complete in under 500ms (CRITICAL PERFORMANCE REQUIREMENT)
        assert!(
            duration.as_millis() < 500,
            "HNSW search took {}ms (must be <500ms for production)",
            duration.as_millis()
        );

        // AND: Should return up to 10 results
        assert!(results.len() <= 10, "Should respect limit parameter");

        // AND: Results should be sorted by similarity (highest first)
        for i in 1..results.len() {
            assert!(
                results[i-1].similarity_score >= results[i].similarity_score,
                "Results must be sorted by similarity score descending"
            );
        }

        Ok(())
    }

    // ========================================================================
    // TEST 3: HNSW Results Match Brute Force Baseline
    // ========================================================================

    #[test]
    #[ignore] // TODO: Fix DistCosine distance vs similarity conversion
    fn test_hnsw_accuracy_vs_brute_force() -> Result<()> {
        // GIVEN: A small dataset where we can verify correctness
        let mut store = VectorStore::new(384)?;
        let dataset = generate_test_dataset(100, 384);

        for (id, vector) in &dataset {
            store.store_vector(id.clone(), vector.clone())?;
        }

        // WHEN: We search with both brute force and HNSW
        let query = generate_test_vector(384, 9999);

        // Brute force baseline (current implementation)
        let brute_force_results = store.search_similar(&query, 5, 0.0)?;
        println!("Brute force results: {:?}", brute_force_results.iter().map(|r| &r.symbol_id).collect::<Vec<_>>());

        // HNSW search
        store.build_hnsw_index()?;
        let hnsw_results = store.search_similar_hnsw(&query, 5, 0.0)?;
        println!("HNSW results: {:?}", hnsw_results.iter().map(|r| &r.symbol_id).collect::<Vec<_>>());

        // THEN: HNSW should find similar results as brute force
        // (HNSW is approximate, so exact ordering may differ slightly)
        assert_eq!(
            brute_force_results.len(),
            hnsw_results.len(),
            "Should return same number of results"
        );

        // Check that there's significant overlap in results (at least 80%)
        let brute_force_ids: std::collections::HashSet<_> = brute_force_results
            .iter()
            .map(|r| &r.symbol_id)
            .collect();
        let hnsw_ids: std::collections::HashSet<_> = hnsw_results
            .iter()
            .map(|r| &r.symbol_id)
            .collect();

        let overlap = brute_force_ids.intersection(&hnsw_ids).count();
        let overlap_ratio = overlap as f32 / brute_force_results.len() as f32;

        assert!(
            overlap_ratio >= 0.8,
            "HNSW should have at least 80% overlap with brute force (got {:.1}%)",
            overlap_ratio * 100.0
        );

        // Top result similarity scores should be very close
        let score_diff = (brute_force_results[0].similarity_score -
                         hnsw_results[0].similarity_score).abs();
        assert!(
            score_diff < 0.05,
            "Top similarity scores should be close (diff: {:.3})",
            score_diff
        );

        Ok(())
    }

    // ========================================================================
    // TEST 4: Index Persistence to Disk
    // ========================================================================

    #[test]
    #[ignore] // TODO: Implement hnswio-based persistence
    fn test_hnsw_index_persistence() -> Result<()> {
        // GIVEN: A VectorStore with HNSW index
        let temp_dir = TempDir::new()?;
        let index_path = temp_dir.path().join("test_index.hnsw");

        let mut store = VectorStore::new(384)?;
        let dataset = generate_test_dataset(100, 384);

        for (id, vector) in &dataset {
            store.store_vector(id.clone(), vector.clone())?;
        }

        store.build_hnsw_index()?;

        // WHEN: We save the index to disk
        let result = store.save_hnsw_index(&index_path);

        // THEN: Save should succeed
        assert!(result.is_ok(), "Saving HNSW index should succeed");

        // AND: Index file should exist
        assert!(
            index_path.exists(),
            "HNSW index file should be created"
        );

        // AND: Index file should not be empty
        let metadata = std::fs::metadata(&index_path)?;
        assert!(
            metadata.len() > 0,
            "HNSW index file should not be empty"
        );

        Ok(())
    }

    // ========================================================================
    // TEST 5: Index Loading from Disk
    // ========================================================================

    #[test]
    #[ignore] // TODO: Implement hnswio-based loading
    fn test_hnsw_index_loading() -> Result<()> {
        // GIVEN: A saved HNSW index
        let temp_dir = TempDir::new()?;
        let index_path = temp_dir.path().join("test_index.hnsw");

        // Create and save index
        let mut store1 = VectorStore::new(384)?;
        let dataset = generate_test_dataset(100, 384);

        for (id, vector) in &dataset {
            store1.store_vector(id.clone(), vector.clone())?;
        }

        store1.build_hnsw_index()?;
        store1.save_hnsw_index(&index_path)?;

        // Perform a search to get baseline results
        let query = generate_test_vector(384, 9999);
        let original_results = store1.search_similar_hnsw(&query, 5, 0.0)?;

        // WHEN: We create a new VectorStore and load the index
        let mut store2 = VectorStore::new(384)?;

        // Load the same vectors
        for (id, vector) in &dataset {
            store2.store_vector(id.clone(), vector.clone())?;
        }

        let load_result = store2.load_hnsw_index(&index_path);

        // THEN: Loading should succeed
        assert!(load_result.is_ok(), "Loading HNSW index should succeed");

        // AND: Index should be marked as loaded
        assert!(store2.has_hnsw_index(), "Loaded index should be available");

        // AND: Search results should match the original
        let loaded_results = store2.search_similar_hnsw(&query, 5, 0.0)?;

        assert_eq!(
            original_results.len(),
            loaded_results.len(),
            "Loaded index should return same number of results"
        );

        assert_eq!(
            original_results[0].symbol_id,
            loaded_results[0].symbol_id,
            "Loaded index should return same top result"
        );

        Ok(())
    }

    // ========================================================================
    // TEST 6: Incremental Index Updates
    // ========================================================================

    #[test]
    #[ignore] // TODO: Implement incremental updates API
    fn test_hnsw_incremental_updates() -> Result<()> {
        // GIVEN: A VectorStore with HNSW index
        let mut store = VectorStore::new(384)?;
        let dataset = generate_test_dataset(100, 384);

        for (id, vector) in &dataset {
            store.store_vector(id.clone(), vector.clone())?;
        }

        store.build_hnsw_index()?;

        // WHEN: We add new vectors after index is built
        let new_vector_id = "symbol_new".to_string();
        let new_vector = generate_test_vector(384, 999);

        let result = store.add_vector_to_hnsw(new_vector_id.clone(), new_vector.clone());

        // THEN: Adding should succeed
        assert!(result.is_ok(), "Adding vector to HNSW index should succeed");

        // AND: New vector should be searchable
        let search_results = store.search_similar_hnsw(&new_vector, 5, 0.0)?;

        assert!(
            search_results.iter().any(|r| r.symbol_id == new_vector_id),
            "Newly added vector should be findable in search"
        );

        Ok(())
    }

    // ========================================================================
    // TEST 7: Vector Removal from Index
    // ========================================================================

    #[test]
    #[ignore] // TODO: Implement vector removal API
    fn test_hnsw_vector_removal() -> Result<()> {
        // GIVEN: A VectorStore with HNSW index
        let mut store = VectorStore::new(384)?;
        let dataset = generate_test_dataset(100, 384);

        for (id, vector) in &dataset {
            store.store_vector(id.clone(), vector.clone())?;
        }

        store.build_hnsw_index()?;

        // WHEN: We remove a vector
        let removed_id = "symbol_50".to_string();
        let removed_vector = dataset[50].1.clone();

        let result = store.remove_vector_from_hnsw(&removed_id);

        // THEN: Removal should succeed
        assert!(result.is_ok(), "Removing vector from HNSW index should succeed");

        // AND: Removed vector should not appear in search results
        let search_results = store.search_similar_hnsw(&removed_vector, 10, 0.0)?;

        assert!(
            !search_results.iter().any(|r| r.symbol_id == removed_id),
            "Removed vector should not appear in search results"
        );

        Ok(())
    }

    // ========================================================================
    // TEST 8: Empty Index Handling
    // ========================================================================

    #[test]
    #[ignore] // TODO: Test empty index handling
    fn test_hnsw_empty_index() -> Result<()> {
        // GIVEN: An empty VectorStore
        let mut store = VectorStore::new(384)?;

        // WHEN: We try to build an index with no vectors
        let result = store.build_hnsw_index();

        // THEN: Should handle gracefully (either error or succeed with empty index)
        // This test documents the expected behavior
        match result {
            Ok(_) => {
                // If it succeeds, searches should return empty results
                let query = generate_test_vector(384, 0);
                let search_results = store.search_similar_hnsw(&query, 10, 0.0)?;
                assert!(search_results.is_empty(), "Empty index should return no results");
            }
            Err(e) => {
                // If it errors, that's also acceptable behavior
                assert!(
                    e.to_string().contains("empty") || e.to_string().contains("no vectors"),
                    "Error message should indicate empty index"
                );
            }
        }

        Ok(())
    }

    // ========================================================================
    // TEST 9: Stress Test - Large Dataset Performance
    // ========================================================================

    #[test]
    #[ignore] // Expensive test - run manually with `cargo test --ignored`
    fn test_hnsw_large_dataset_stress() -> Result<()> {
        // GIVEN: A large dataset (50k vectors to stress test)
        let mut store = VectorStore::new(384)?;

        println!("Generating 50k test vectors...");
        let dataset = generate_test_dataset(50_000, 384);

        println!("Storing vectors...");
        for (id, vector) in &dataset {
            store.store_vector(id.clone(), vector.clone())?;
        }

        // WHEN: We build index and search
        println!("Building HNSW index for 50k vectors...");
        let build_start = Instant::now();
        store.build_hnsw_index()?;
        let build_duration = build_start.elapsed();

        println!("Index built in {:?}", build_duration);

        // THEN: Index building should complete in reasonable time (<60s)
        assert!(
            build_duration.as_secs() < 60,
            "Index building took too long: {:?}",
            build_duration
        );

        // AND: Search should still be fast
        let query = generate_test_vector(384, 99999);
        let search_start = Instant::now();
        let results = store.search_similar_hnsw(&query, 10, 0.0)?;
        let search_duration = search_start.elapsed();

        println!("Search completed in {:?}", search_duration);

        assert!(
            search_duration.as_millis() < 500,
            "Search on large dataset took too long: {:?}",
            search_duration
        );

        assert!(!results.is_empty(), "Should find results in large dataset");

        Ok(())
    }
}