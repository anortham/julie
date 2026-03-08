//! Tests for memory checkpoint embedding (format, store, hybrid search).

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use tempfile::TempDir;

    use crate::database::SymbolDatabase;
    use crate::embeddings::{DeviceInfo, EmbeddingProvider};
    use crate::memory::embedding::format_checkpoint_for_embedding;
    use crate::memory::{Checkpoint, MemorySearchResult};

    /// Deterministic mock provider: produces a fixed-length vector seeded from
    /// a hash of the input text so different texts get different vectors.
    struct HashProvider {
        dims: usize,
    }

    impl HashProvider {
        fn new(dims: usize) -> Self {
            Self { dims }
        }
    }

    impl EmbeddingProvider for HashProvider {
        fn embed_query(&self, text: &str) -> Result<Vec<f32>> {
            Ok(deterministic_vector(text, self.dims))
        }

        fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
            Ok(texts
                .iter()
                .map(|t| deterministic_vector(t, self.dims))
                .collect())
        }

        fn dimensions(&self) -> usize {
            self.dims
        }

        fn device_info(&self) -> DeviceInfo {
            DeviceInfo {
                runtime: "test".to_string(),
                device: "cpu".to_string(),
                model_name: "hash-mock".to_string(),
                dimensions: self.dims,
            }
        }
    }

    /// Produce a deterministic f32 vector from text. Simple hash-based spread.
    fn deterministic_vector(text: &str, dims: usize) -> Vec<f32> {
        let mut v = vec![0.0_f32; dims];
        let mut hash: u64 = 5381;
        for b in text.bytes() {
            hash = hash.wrapping_mul(33).wrapping_add(b as u64);
        }
        for i in 0..dims {
            let seed = hash.wrapping_add(i as u64).wrapping_mul(2654435761);
            v[i] = ((seed % 1000) as f32 / 1000.0) - 0.5;
        }
        v
    }

    fn make_test_checkpoint(id: &str, description: &str) -> Checkpoint {
        Checkpoint {
            id: id.to_string(),
            timestamp: "2026-03-08T10:00:00.000Z".to_string(),
            description: description.to_string(),
            checkpoint_type: None,
            context: None,
            decision: Some("Use RRF for merge".to_string()),
            alternatives: None,
            impact: Some("Improves search relevance".to_string()),
            evidence: None,
            symbols: Some(vec!["rrf_merge".to_string(), "hybrid_search".to_string()]),
            next: None,
            confidence: None,
            unknowns: None,
            tags: Some(vec!["search".to_string(), "phase5".to_string()]),
            git: None,
            summary: None,
            plan_id: None,
        }
    }

    fn create_test_db() -> (SymbolDatabase, TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).expect("create db");
        (db, dir)
    }

    // ── Format ─────────────────────────────────────────────────────────

    #[test]
    fn test_format_checkpoint_includes_key_fields() {
        let cp = make_test_checkpoint("cp-001", "Implemented weighted RRF merge");
        let text = format_checkpoint_for_embedding(&cp);

        assert!(text.contains("Implemented weighted RRF merge"), "should contain description");
        assert!(text.contains("search"), "should contain tags");
        assert!(text.contains("phase5"), "should contain tags");
        assert!(text.contains("Use RRF for merge"), "should contain decision");
        assert!(text.contains("rrf_merge"), "should contain symbols");
    }

    #[test]
    fn test_format_checkpoint_handles_empty_optional_fields() {
        let cp = Checkpoint {
            id: "cp-002".to_string(),
            timestamp: "2026-03-08T10:00:00.000Z".to_string(),
            description: "Simple checkpoint".to_string(),
            checkpoint_type: None,
            context: None,
            decision: None,
            alternatives: None,
            impact: None,
            evidence: None,
            symbols: None,
            next: None,
            confidence: None,
            unknowns: None,
            tags: None,
            git: None,
            summary: None,
            plan_id: None,
        };

        let text = format_checkpoint_for_embedding(&cp);
        assert!(text.contains("Simple checkpoint"));
        // Should not panic or produce garbage
        assert!(!text.is_empty());
    }

    // ── Embed + Store ──────────────────────────────────────────────────

    #[test]
    fn test_embed_checkpoint_stores_in_db() {
        let (mut db, _dir) = create_test_db();
        let provider = HashProvider::new(384);
        let cp = make_test_checkpoint("cp-001", "Implemented weighted RRF merge");

        crate::memory::embedding::embed_checkpoint(&cp, &mut db, &provider).unwrap();

        assert_eq!(db.memory_embedding_count().unwrap(), 1);
    }

    #[test]
    fn test_embed_checkpoints_batch() {
        let (mut db, _dir) = create_test_db();
        let provider = HashProvider::new(384);

        let checkpoints = vec![
            make_test_checkpoint("cp-001", "First checkpoint"),
            make_test_checkpoint("cp-002", "Second checkpoint"),
            make_test_checkpoint("cp-003", "Third checkpoint"),
        ];

        let count = crate::memory::embedding::embed_checkpoints_batch(
            &checkpoints,
            &mut db,
            &provider,
        )
        .unwrap();

        assert_eq!(count, 3);
        assert_eq!(db.memory_embedding_count().unwrap(), 3);
    }

    #[test]
    fn test_embed_checkpoint_replaces_existing() {
        let (mut db, _dir) = create_test_db();
        let provider = HashProvider::new(384);

        let cp = make_test_checkpoint("cp-001", "Original description");
        crate::memory::embedding::embed_checkpoint(&cp, &mut db, &provider).unwrap();
        assert_eq!(db.memory_embedding_count().unwrap(), 1);

        // Re-embed same ID with different description — should replace, not duplicate
        let cp2 = make_test_checkpoint("cp-001", "Updated description");
        crate::memory::embedding::embed_checkpoint(&cp2, &mut db, &provider).unwrap();
        assert_eq!(db.memory_embedding_count().unwrap(), 1);
    }

    // ── KNN Memory Search ──────────────────────────────────────────────

    #[test]
    fn test_knn_memory_search_finds_similar_checkpoints() {
        let (mut db, _dir) = create_test_db();
        let provider = HashProvider::new(384);

        // Embed several checkpoints with distinct topics
        let checkpoints = vec![
            make_test_checkpoint("cp-auth", "Implemented user authentication with JWT tokens"),
            make_test_checkpoint("cp-search", "Added hybrid search with RRF merge algorithm"),
            make_test_checkpoint("cp-deploy", "Configured CI/CD pipeline for deployment"),
        ];
        crate::memory::embedding::embed_checkpoints_batch(&checkpoints, &mut db, &provider)
            .unwrap();

        // Query for something related to search
        let query = "search merging algorithm";
        let query_vec = provider.embed_query(query).unwrap();
        let results = db.knn_memory_search(&query_vec, 3).unwrap();

        assert_eq!(results.len(), 3);
        // All 3 checkpoints should be returned (KNN always returns k results if available)
    }

    // ── Hybrid Memory Search ───────────────────────────────────────────

    #[test]
    fn test_hybrid_memory_search_combines_bm25_and_knn() {
        let tantivy_dir = tempfile::tempdir().unwrap();
        let (mut db, _db_dir) = create_test_db();
        let provider = HashProvider::new(384);

        // Create and index checkpoints in both Tantivy and sqlite-vec
        let index = crate::memory::index::MemoryIndex::create(tantivy_dir.path()).unwrap();

        let checkpoints = vec![
            make_test_checkpoint("cp-auth", "JWT authentication tokens and session management"),
            make_test_checkpoint("cp-search", "Hybrid search with weighted RRF merge"),
            make_test_checkpoint("cp-deploy", "Kubernetes deployment and CI/CD pipeline"),
        ];

        for cp in &checkpoints {
            index.add_checkpoint(cp, Some(&format!("2026-03-08/{}.md", cp.id))).unwrap();
        }
        index.commit().unwrap();

        crate::memory::embedding::embed_checkpoints_batch(&checkpoints, &mut db, &provider)
            .unwrap();

        // Hybrid search should return results from both BM25 and KNN
        let results = crate::memory::embedding::hybrid_memory_search(
            "search merge",
            &index,
            &db,
            Some(&provider as &dyn EmbeddingProvider),
            10,
        )
        .unwrap();

        assert!(!results.is_empty(), "should return results from hybrid search");
    }

    #[test]
    fn test_hybrid_memory_search_degrades_without_provider() {
        let tantivy_dir = tempfile::tempdir().unwrap();
        let (db, _db_dir) = create_test_db();

        let index = crate::memory::index::MemoryIndex::create(tantivy_dir.path()).unwrap();

        let cp = make_test_checkpoint("cp-search", "Hybrid search with weighted RRF merge");
        index.add_checkpoint(&cp, Some("2026-03-08/cp-search.md")).unwrap();
        index.commit().unwrap();

        // No provider — should still return BM25 results
        let results = crate::memory::embedding::hybrid_memory_search(
            "search merge",
            &index,
            &db,
            None,
            10,
        )
        .unwrap();

        assert!(!results.is_empty(), "should return BM25 results even without provider");
    }
}
