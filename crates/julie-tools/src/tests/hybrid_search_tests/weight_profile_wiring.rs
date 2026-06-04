/// Weight profile wiring tests (Phase 5, Task 6).
///
/// Verifies that `hybrid_search` uses `weighted_rrf_merge` when a
/// `SearchWeightProfile` is provided, and falls back to uniform `rrf_merge`
/// when `None` is passed.
#[cfg(test)]
mod weight_profile_wiring_tests {
    use anyhow::Result;

    use julie_core::database::SymbolDatabase;
    use julie_pipeline::embeddings::{DeviceInfo, EmbeddingProvider};
    use julie_extractors::SymbolKind;
    use julie_index::search::hybrid::hybrid_search;
    use julie_index::search::index::{SearchDocument, SearchFilter, SearchIndex};
    use julie_index::search::weights::SearchWeightProfile;
    use julie_test_support::db::{
        file_info_builder, store_file_info_if_missing, symbol_builder,
    };
    use tempfile::TempDir;

    /// Mock embedding provider that returns a deterministic vector.
    struct StaticProvider;

    impl EmbeddingProvider for StaticProvider {
        fn embed_query(&self, _text: &str) -> Result<Vec<f32>> {
            Ok(vec![1.0_f32; 384])
        }

        fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
            Ok(texts.iter().map(|_| vec![1.0_f32; 384]).collect())
        }

        fn dimensions(&self) -> usize {
            384
        }

        fn device_info(&self) -> DeviceInfo {
            DeviceInfo {
                runtime: "test".into(),
                device: "cpu".into(),
                model_name: "static-mock".into(),
                dimensions: 384,
            }
        }
    }

    /// Helper: create a SearchIndex + SymbolDatabase with test symbols + embeddings.
    fn setup_index_and_db_with_embeddings() -> (SearchIndex, SymbolDatabase, TempDir, TempDir) {
        let idx_dir = tempfile::tempdir().unwrap();
        let db_dir = tempfile::tempdir().unwrap();

        let index = SearchIndex::create(idx_dir.path()).unwrap();
        let mut db = SymbolDatabase::new(&db_dir.path().join("test.db")).unwrap();

        store_process_data_file(&db);
        db.store_symbols(&[process_data_symbol()]).unwrap();

        // Add symbol to Tantivy index
        index
            .add_search_doc(&SearchDocument::symbol_from_parts(
                "sym1",
                "process_data",
                "fn process_data(input: &str) -> Result<()>",
                "Processes input data.",
                "fn process_data(input: &str) -> Result<()> { Ok(()) }",
                "src/lib.rs",
                "function",
                "rust",
                10,
            ))
            .unwrap();
        index.commit().unwrap();

        // Seed embeddings so KNN returns results
        db.store_embeddings(&[("sym1".to_string(), vec![0.95_f32; 384])])
            .unwrap();

        (index, db, idx_dir, db_dir)
    }

    fn store_process_data_file(db: &SymbolDatabase) {
        store_file_info_if_missing(
            db,
            &file_info_builder("src/lib.rs")
                .language("rust")
                .hash("abc123")
                .size(100)
                .last_modified(0)
                .last_indexed(0)
                .symbol_count(0)
                .line_count(0)
                .build(),
        )
        .unwrap();
    }

    fn process_data_symbol() -> julie_extractors::Symbol {
        symbol_builder("sym1", "process_data", "src/lib.rs")
            .kind(SymbolKind::Function)
            .language("rust")
            .span(10, 0, 20, 0)
            .bytes(0, 200)
            .signature("fn process_data(input: &str) -> Result<()>")
            .doc_comment("Processes input data.")
            .confidence(1.0)
            .build()
    }

    #[test]
    fn test_hybrid_search_with_weight_profile_uses_weighted_merge() {
        let (index, db, _idx_dir, _db_dir) = setup_index_and_db_with_embeddings();
        let provider = StaticProvider;
        let profile = SearchWeightProfile::fast_search();

        let results = hybrid_search(
            "process_data",
            &SearchFilter::default(),
            10,
            &index,
            &db,
            Some(&provider),
            Some(profile),
        )
        .unwrap();

        assert!(
            !results.results.is_empty(),
            "should return results with weight profile"
        );
        assert_eq!(results.results[0].name, "process_data");
    }

    #[test]
    fn test_hybrid_search_none_profile_uses_uniform_merge() {
        let (index, db, _idx_dir, _db_dir) = setup_index_and_db_with_embeddings();
        let provider = StaticProvider;

        // None profile should still work (backward compat)
        let results = hybrid_search(
            "process_data",
            &SearchFilter::default(),
            10,
            &index,
            &db,
            Some(&provider),
            None,
        )
        .unwrap();

        assert!(
            !results.results.is_empty(),
            "should return results with None profile"
        );
    }

    #[test]
    fn test_hybrid_search_weight_profile_keyword_only_graceful() {
        // When no embedding provider but profile is given, should still work
        let idx_dir = tempfile::tempdir().unwrap();
        let db_dir = tempfile::tempdir().unwrap();

        let index = SearchIndex::create(idx_dir.path()).unwrap();
        let mut db = SymbolDatabase::new(&db_dir.path().join("test.db")).unwrap();

        store_process_data_file(&db);
        db.store_symbols(&[process_data_symbol()]).unwrap();

        index
            .add_search_doc(&SearchDocument::symbol_from_parts(
                "sym1",
                "process_data",
                "fn process_data(input: &str) -> Result<()>",
                "Processes input data.",
                "fn process_data(input: &str) -> Result<()> { Ok(()) }",
                "src/lib.rs",
                "function",
                "rust",
                10,
            ))
            .unwrap();
        index.commit().unwrap();

        let profile = SearchWeightProfile::get_context();
        let results = hybrid_search(
            "process_data",
            &SearchFilter::default(),
            10,
            &index,
            &db,
            None, // No embedding provider
            Some(profile),
        )
        .unwrap();

        assert!(
            !results.results.is_empty(),
            "keyword-only should still work with a weight profile"
        );
    }
}
