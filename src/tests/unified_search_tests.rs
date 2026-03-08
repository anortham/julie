//! Tests for unified cross-content search (code + memories).

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use tempfile::TempDir;

    use crate::database::SymbolDatabase;
    use crate::embeddings::{DeviceInfo, EmbeddingProvider};
    use crate::memory::index::MemoryIndex;
    use crate::memory::Checkpoint;
    use crate::search::content_type::ContentType;
    use crate::search::index::{SearchFilter, SearchIndex, SymbolDocument};
    use crate::search::unified::{SearchResultItem, UnifiedSearchOptions, unified_search};

    /// Deterministic mock provider (same as memory_embedding_tests).
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

    fn make_checkpoint(id: &str, description: &str) -> Checkpoint {
        Checkpoint {
            id: id.to_string(),
            timestamp: "2026-03-08T10:00:00.000Z".to_string(),
            description: description.to_string(),
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
            tags: Some(vec!["test".to_string()]),
            git: None,
            summary: None,
            plan_id: None,
        }
    }

    struct TestInfra {
        search_index: SearchIndex,
        memory_index: MemoryIndex,
        db: SymbolDatabase,
        _tantivy_dir: TempDir,
        _memory_dir: TempDir,
        _db_dir: TempDir,
    }

    fn setup_test_infra() -> TestInfra {
        let tantivy_dir = tempfile::tempdir().unwrap();
        let memory_dir = tempfile::tempdir().unwrap();
        let db_dir = tempfile::tempdir().unwrap();

        let search_index = SearchIndex::create(tantivy_dir.path()).unwrap();
        let memory_index = MemoryIndex::create(memory_dir.path()).unwrap();
        let db_path = db_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        TestInfra {
            search_index,
            memory_index,
            db,
            _tantivy_dir: tantivy_dir,
            _memory_dir: memory_dir,
            _db_dir: db_dir,
        }
    }

    fn add_code_symbol(infra: &TestInfra, name: &str, kind: &str, language: &str) {
        let doc = SymbolDocument {
            id: format!("sym-{name}"),
            name: name.to_string(),
            kind: kind.to_string(),
            language: language.to_string(),
            file_path: format!("src/{name}.rs"),
            signature: format!("fn {name}()"),
            doc_comment: String::new(),
            code_body: String::new(),
            start_line: 1,
        };
        infra.search_index.add_symbol(&doc).unwrap();
    }

    fn add_memory_checkpoint(infra: &TestInfra, id: &str, description: &str) {
        let cp = make_checkpoint(id, description);
        infra
            .memory_index
            .add_checkpoint(&cp, Some(&format!("2026-03-08/{id}.md")))
            .unwrap();
    }

    // ── Content Type Filtering ─────────────────────────────────────────

    #[test]
    fn test_unified_search_content_type_all_returns_both() {
        let infra = setup_test_infra();

        add_code_symbol(&infra, "hybrid_search", "function", "rust");
        infra.search_index.commit().unwrap();

        add_memory_checkpoint(&infra, "cp-search", "Implemented hybrid search with RRF");
        infra.memory_index.commit().unwrap();

        let opts = UnifiedSearchOptions {
            content_type: None, // None = all
            limit: 10,
        };

        let results = unified_search(
            "hybrid search",
            &opts,
            &infra.search_index,
            &infra.memory_index,
            &infra.db,
            None,
        )
        .unwrap();

        let has_code = results.iter().any(|r| r.content_type == ContentType::Code);
        let has_memory = results.iter().any(|r| r.content_type == ContentType::Memory);

        assert!(has_code, "should contain code results");
        assert!(has_memory, "should contain memory results");
    }

    #[test]
    fn test_unified_search_content_type_code_only() {
        let infra = setup_test_infra();

        add_code_symbol(&infra, "hybrid_search", "function", "rust");
        infra.search_index.commit().unwrap();

        add_memory_checkpoint(&infra, "cp-search", "Implemented hybrid search with RRF");
        infra.memory_index.commit().unwrap();

        let opts = UnifiedSearchOptions {
            content_type: Some(ContentType::Code),
            limit: 10,
        };

        let results = unified_search(
            "hybrid search",
            &opts,
            &infra.search_index,
            &infra.memory_index,
            &infra.db,
            None,
        )
        .unwrap();

        assert!(
            results.iter().all(|r| r.content_type == ContentType::Code),
            "should only contain code results"
        );
    }

    #[test]
    fn test_unified_search_content_type_memory_only() {
        let infra = setup_test_infra();

        add_code_symbol(&infra, "hybrid_search", "function", "rust");
        infra.search_index.commit().unwrap();

        add_memory_checkpoint(&infra, "cp-search", "Implemented hybrid search with RRF");
        infra.memory_index.commit().unwrap();

        let opts = UnifiedSearchOptions {
            content_type: Some(ContentType::Memory),
            limit: 10,
        };

        let results = unified_search(
            "hybrid search",
            &opts,
            &infra.search_index,
            &infra.memory_index,
            &infra.db,
            None,
        )
        .unwrap();

        assert!(
            results.iter().all(|r| r.content_type == ContentType::Memory),
            "should only contain memory results"
        );
    }

    // ── Result Tagging ─────────────────────────────────────────────────

    #[test]
    fn test_unified_results_are_tagged_correctly() {
        let infra = setup_test_infra();

        add_code_symbol(&infra, "rrf_merge", "function", "rust");
        infra.search_index.commit().unwrap();

        add_memory_checkpoint(&infra, "cp-rrf", "Added RRF merge for search ranking");
        infra.memory_index.commit().unwrap();

        let opts = UnifiedSearchOptions {
            content_type: None,
            limit: 10,
        };

        let results = unified_search(
            "rrf merge",
            &opts,
            &infra.search_index,
            &infra.memory_index,
            &infra.db,
            None,
        )
        .unwrap();

        for r in &results {
            match &r.result {
                SearchResultItem::Code(_) => {
                    assert_eq!(r.content_type, ContentType::Code);
                }
                SearchResultItem::Memory(_) => {
                    assert_eq!(r.content_type, ContentType::Memory);
                }
            }
        }
    }

    // ── Score Ordering ─────────────────────────────────────────────────

    #[test]
    fn test_unified_results_sorted_by_score_descending() {
        let infra = setup_test_infra();

        // Add multiple items to get a meaningful ranking
        add_code_symbol(&infra, "search_index", "struct", "rust");
        add_code_symbol(&infra, "search_filter", "struct", "rust");
        infra.search_index.commit().unwrap();

        add_memory_checkpoint(&infra, "cp-1", "Search index implementation notes");
        add_memory_checkpoint(&infra, "cp-2", "Search filter pipeline refactoring");
        infra.memory_index.commit().unwrap();

        let opts = UnifiedSearchOptions {
            content_type: None,
            limit: 10,
        };

        let results = unified_search(
            "search",
            &opts,
            &infra.search_index,
            &infra.memory_index,
            &infra.db,
            None,
        )
        .unwrap();

        // Scores should be monotonically decreasing
        for window in results.windows(2) {
            assert!(
                window[0].score >= window[1].score,
                "results should be sorted by score descending: {} >= {}",
                window[0].score,
                window[1].score
            );
        }
    }

    // ── Empty Results ──────────────────────────────────────────────────

    #[test]
    fn test_unified_search_no_results() {
        let infra = setup_test_infra();

        let opts = UnifiedSearchOptions {
            content_type: None,
            limit: 10,
        };

        let results = unified_search(
            "nonexistent_query_xyz",
            &opts,
            &infra.search_index,
            &infra.memory_index,
            &infra.db,
            None,
        )
        .unwrap();

        assert!(results.is_empty());
    }
}
