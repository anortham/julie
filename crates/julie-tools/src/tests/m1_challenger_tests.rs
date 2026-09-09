//! Empirical challenger tests for Milestone M1 (Codex Findings 2 & 6):
//! - Reader isolation: untagged query vectors cannot query KNN in hybrid_search_with_embedding or find_similar_by_tagged_query.
//! - Cross-model mismatch: mismatched encoder keys or source revisions fail transactional generation validation.
//! - Cancellation bubbling: cancelled EmbeddingRequestBudget bubbles up error without falling back to lexical results.

#[cfg(test)]
mod m1_challenger_tests {
    use std::sync::Arc;
    use tempfile::TempDir;

    use anyhow::Result;
    use julie_core::database::{FileInfo, SymbolDatabase};
    use julie_core::embeddings_contract::{
        DeviceInfo, EmbeddingProvider, EmbeddingRequestBudget, EncoderIdentity, SemanticMode,
        TaggedQueryEmbedding,
    };
    use julie_extractors::SymbolKind;
    use julie_index::search::hybrid::{
        compute_tagged_query_embedding_for_hybrid, hybrid_search_with_embedding,
        hybrid_search_with_tagged_embedding,
    };
    use julie_index::search::index::{SearchDocument, SearchFilter, SearchIndex};
    use julie_index::search::similarity;
    use julie_test_support::FakeToolContext;
    use julie_test_support::db::symbol_builder;

    use crate::get_context::GetContextTool;
    use crate::navigation::FastRefsTool;
    use crate::navigation::resolution::WorkspaceTarget;
    use crate::search::FastSearchTool;

    struct MockTestProvider {
        model_name: String,
        dimensions: usize,
    }

    impl MockTestProvider {
        fn new(model_name: &str, dimensions: usize) -> Self {
            Self {
                model_name: model_name.to_string(),
                dimensions,
            }
        }
    }

    impl EmbeddingProvider for MockTestProvider {
        fn embed_query(&self, _text: &str, budget: &EmbeddingRequestBudget) -> Result<Vec<f32>> {
            budget.check_budget()?;
            Ok(vec![1.0_f32; self.dimensions])
        }
        fn embed_batch(
            &self,
            texts: &[String],
            budget: &EmbeddingRequestBudget,
        ) -> Result<Vec<Vec<f32>>> {
            budget.check_budget()?;
            Ok(texts
                .iter()
                .map(|_| vec![1.0_f32; self.dimensions])
                .collect())
        }
        fn encoder_identity(&self) -> Result<EncoderIdentity> {
            Ok(EncoderIdentity::mock(&self.model_name, self.dimensions))
        }
        fn dimensions(&self) -> usize {
            self.dimensions
        }
        fn device_info(&self) -> DeviceInfo {
            DeviceInfo {
                runtime: "mock".into(),
                device: "cpu".into(),
                model_name: self.model_name.clone(),
                dimensions: self.dimensions,
            }
        }
    }

    fn setup_test_db(tmp: &TempDir) -> SymbolDatabase {
        let db_path = tmp.path().join("test.db");
        let mut db = SymbolDatabase::new(&db_path).unwrap();

        db.store_file_info(&FileInfo {
            path: "src/lib.rs".to_string(),
            language: "rust".to_string(),
            hash: "hash_lib".to_string(),
            size: 500,
            last_modified: 1000,
            last_indexed: 0,
            symbol_count: 1,
            line_count: 20,
            content: None,
        })
        .unwrap();

        let sym = symbol_builder("sym_semantic_only", "semantic_only_sym", "src/lib.rs")
            .kind(SymbolKind::Function)
            .language("rust")
            .span(1, 0, 10, 0)
            .bytes(0, 100)
            .signature("fn semantic_only_sym()")
            .confidence(1.0)
            .build();
        db.store_symbols(&[sym]).unwrap();

        db
    }

    // ── Challenge 1: Reader Isolation & Untagged Vectors in hybrid_search ─────

    #[test]
    fn challenge_untagged_query_vectors_cannot_query_knn_in_hybrid_search() {
        let tmp = TempDir::new().unwrap();
        let idx_dir = TempDir::new().unwrap();
        let mut db = setup_test_db(&tmp);
        let index = SearchIndex::create(idx_dir.path()).unwrap();

        // Publish a ready generation for encoder "encoder-alpha" at revision 0
        db.publish_test_generation("encoder-alpha", 0, 384).unwrap();

        // Store high-similarity embedding for "sym_semantic_only"
        let emb = vec![1.0_f32; 384];
        db.store_embeddings(&[("sym_semantic_only".to_string(), emb.clone())])
            .unwrap();

        // Add a Tantivy document that does NOT match "semantic_only_sym"
        index
            .add_search_doc(&SearchDocument::symbol_from_parts(
                "sym_kw",
                "keyword_sym",
                "fn keyword_sym()",
                "Keyword doc.",
                "fn keyword_sym() {}",
                "src/lib.rs",
                "function",
                "rust",
                15,
            ))
            .unwrap();
        index.commit().unwrap();

        // 1. Call hybrid_search_with_embedding with raw query vector (untagged).
        // Since it is untagged, encoder_key is "", which does NOT match "encoder-alpha".
        // KNN query must be skipped.
        let results_untagged = hybrid_search_with_embedding(
            "unrelated_search_term",
            &SearchFilter::default(),
            10,
            &index,
            &db,
            Some(emb.clone()),
            None,
        )
        .unwrap();

        assert!(
            !results_untagged
                .results
                .iter()
                .any(|r| r.name == "semantic_only_sym"),
            "Untagged query embedding must NOT yield semantic KNN results"
        );

        // 2. In contrast, calling with properly tagged embedding bound to "encoder-alpha"
        // MUST successfully execute KNN and find "semantic_only_sym".
        let tagged_alpha = TaggedQueryEmbedding::new(emb, "encoder-alpha", 0);
        let results_tagged = hybrid_search_with_tagged_embedding(
            "unrelated_search_term",
            &SearchFilter::default(),
            10,
            &index,
            &db,
            Some(tagged_alpha),
            None,
            SemanticMode::Auto,
        )
        .unwrap();

        assert!(
            results_tagged
                .results
                .iter()
                .any(|r| r.name == "semantic_only_sym"),
            "Tagged query embedding matching ready generation must execute KNN and find symbol"
        );
    }

    // ── Challenge 2: Untagged & Cross-Model Vectors in find_similar_by_tagged_query ─

    #[test]
    fn challenge_untagged_and_cross_model_vectors_fail_in_similarity_reader() {
        let tmp = TempDir::new().unwrap();
        let mut db = setup_test_db(&tmp);
        db.publish_test_generation("model-alpha", 0, 384).unwrap();

        let emb = vec![1.0_f32; 384];
        db.store_embeddings(&[("sym_semantic_only".to_string(), emb.clone())])
            .unwrap();

        // 1. Untagged vector in Auto mode -> returns empty vec, zero KNN
        let untagged = TaggedQueryEmbedding::untagged(emb.clone());
        let auto_res =
            similarity::find_similar_by_tagged_query(&db, &untagged, 5, 0.0, SemanticMode::Auto)
                .unwrap();
        assert!(
            auto_res.is_empty(),
            "Untagged query in Auto mode must return empty"
        );

        // 2. Untagged vector in Required mode -> must bail with SEMANTICS_NOT_READY
        let req_err = similarity::find_similar_by_tagged_query(
            &db,
            &untagged,
            5,
            0.0,
            SemanticMode::Required,
        )
        .unwrap_err();
        assert!(
            req_err.to_string().contains("SEMANTICS_NOT_READY"),
            "Untagged query in Required mode must bail with SEMANTICS_NOT_READY: {req_err}"
        );

        // 3. Cross-model mismatched encoder key ("model-beta" vs "model-alpha")
        let cross_model = TaggedQueryEmbedding::new(emb.clone(), "model-beta", 0);
        let cross_auto =
            similarity::find_similar_by_tagged_query(&db, &cross_model, 5, 0.0, SemanticMode::Auto)
                .unwrap();
        assert!(
            cross_auto.is_empty(),
            "Cross-model query in Auto mode must return empty"
        );

        let cross_req_err = similarity::find_similar_by_tagged_query(
            &db,
            &cross_model,
            5,
            0.0,
            SemanticMode::Required,
        )
        .unwrap_err();
        assert!(
            cross_req_err.to_string().contains("SEMANTICS_NOT_READY")
                && cross_req_err.to_string().contains("model-beta"),
            "Cross-model query in Required mode must bail with SEMANTICS_NOT_READY for model-beta: {cross_req_err}"
        );

        // 4. Mismatched revision (rev 42 vs rev 0)
        let wrong_rev = TaggedQueryEmbedding::new(emb.clone(), "model-alpha", 42);
        let rev_req_err = similarity::find_similar_by_tagged_query(
            &db,
            &wrong_rev,
            5,
            0.0,
            SemanticMode::Required,
        )
        .unwrap_err();
        assert!(
            rev_req_err.to_string().contains("SEMANTICS_NOT_READY")
                && rev_req_err.to_string().contains("rev 42"),
            "Mismatched revision in Required mode must bail: {rev_req_err}"
        );

        // 5. Correctly tagged query succeeds and returns results
        let correct = TaggedQueryEmbedding::new(emb, "model-alpha", 0);
        let correct_res =
            similarity::find_similar_by_tagged_query(&db, &correct, 5, 0.0, SemanticMode::Required)
                .unwrap();
        assert_eq!(correct_res.len(), 1);
        assert_eq!(correct_res[0].symbol.name, "semantic_only_sym");
    }

    // ── Challenge 3: Cancellation in compute_tagged_query_embedding_for_hybrid ─

    #[test]
    fn challenge_cancellation_bubbles_up_in_compute_tagged_query_embedding() {
        let tmp = TempDir::new().unwrap();
        let mut db = setup_test_db(&tmp);
        let provider = MockTestProvider::new("mock-model", 384);
        let key = provider.encoder_identity().unwrap().storage_key().unwrap();
        db.publish_test_generation(&key, 0, 384).unwrap();

        let budget = EmbeddingRequestBudget::for_testing();
        budget.cancel(); // Trigger cancellation

        // Auto mode: must return Err (cancellation), NOT Ok(None) fallback
        let auto_res = compute_tagged_query_embedding_for_hybrid(
            "test query",
            Some(&provider),
            &budget,
            &db,
            SemanticMode::Auto,
        );
        assert!(
            auto_res.is_err(),
            "Cancelled budget in Auto mode must return Err, not fallback to Ok(None)"
        );
        let err_msg = auto_res.unwrap_err().to_string();
        assert!(
            err_msg.contains("embedding request cancelled"),
            "Expected cancellation error message, got: {err_msg}"
        );

        // Required mode: must also return Err (cancellation)
        let req_res = compute_tagged_query_embedding_for_hybrid(
            "test query",
            Some(&provider),
            &budget,
            &db,
            SemanticMode::Required,
        );
        assert!(req_res.is_err());
        assert!(
            req_res
                .unwrap_err()
                .to_string()
                .contains("embedding request cancelled")
        );
    }

    // ── Challenge 4: Cancellation in FastRefs bubbles up without fallback ───────

    #[tokio::test]
    async fn challenge_cancellation_bubbles_up_in_fast_refs() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let mut db = SymbolDatabase::new(&db_path).unwrap();

        let provider = Arc::new(MockTestProvider::new("test-mock", 384));
        let key = provider.encoder_identity().unwrap().storage_key().unwrap();
        let gen_id = db.begin_embedding_generation(&key, 0, 384).unwrap();
        db.publish_embedding_generation(gen_id, 0, 0, 0).unwrap();

        let context = FakeToolContext::new()
            .with_primary_db_path(db_path)
            .with_embedding_provider(provider);

        let budget = EmbeddingRequestBudget::for_testing();
        budget.cancel(); // Trigger cancellation

        let tool_auto = FastRefsTool {
            symbol: "nonexistent_symbol".to_string(),
            limit: 10,
            reference_kind: None,
            include_definition: false,
            workspace: Some("primary".to_string()),
            semantics: Some(SemanticMode::Auto),
        };

        // Call tool with cancelled budget: must return Err, NOT Ok with empty/fallback text
        let res_auto = tool_auto
            .call_tool_with_target_and_budget(
                &context,
                &WorkspaceTarget::Primary,
                Some(budget.clone()),
            )
            .await;

        assert!(
            res_auto.is_err(),
            "Cancelled budget in FastRefs must bubble up error, not return Ok fallback"
        );
        let err_str = res_auto.unwrap_err().to_string();
        assert!(
            err_str.contains("cancelled"),
            "Error must mention cancellation, got: {err_str}"
        );

        // Also test Required mode
        let tool_req = FastRefsTool {
            symbol: "nonexistent_symbol".to_string(),
            limit: 10,
            reference_kind: None,
            include_definition: false,
            workspace: Some("primary".to_string()),
            semantics: Some(SemanticMode::Required),
        };

        let res_req = tool_req
            .call_tool_with_target_and_budget(&context, &WorkspaceTarget::Primary, Some(budget))
            .await;

        assert!(res_req.is_err());
        assert!(res_req.unwrap_err().to_string().contains("cancelled"));
    }

    // ── Challenge 5: Cancellation in GetContext bubbles up without fallback ────

    #[tokio::test]
    async fn challenge_cancellation_bubbles_up_in_get_context() {
        let tmp = TempDir::new().unwrap();
        let idx_dir = TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let mut db = SymbolDatabase::new(&db_path).unwrap();
        let index = Arc::new(SearchIndex::create(idx_dir.path()).unwrap());

        let provider = Arc::new(MockTestProvider::new("test-mock", 384));
        let key = provider.encoder_identity().unwrap().storage_key().unwrap();
        let gen_id = db.begin_embedding_generation(&key, 0, 384).unwrap();
        db.publish_embedding_generation(gen_id, 0, 0, 0).unwrap();

        let context = FakeToolContext::new()
            .with_primary_db_path(db_path)
            .with_search_index(index)
            .with_embedding_provider(provider);

        let budget = EmbeddingRequestBudget::for_testing();
        budget.cancel(); // Trigger cancellation

        let tool_auto = GetContextTool {
            query: "process_data".to_string(),
            max_tokens: None,
            workspace: Some("primary".to_string()),
            language: None,
            file_pattern: None,
            format: None,
            edited_files: None,
            entry_symbols: None,
            stack_trace: None,
            failing_test: None,
            max_hops: None,
            prefer_tests: None,
            semantics: Some(SemanticMode::Auto),
        };

        let res_auto = tool_auto
            .call_tool_with_target_and_budget(
                &context,
                WorkspaceTarget::Primary,
                Some(budget.clone()),
            )
            .await;

        assert!(
            res_auto.is_err(),
            "Cancelled budget in GetContext must bubble up error, not return Ok fallback"
        );
        let err_str = res_auto.unwrap_err().to_string();
        assert!(
            err_str.contains("cancelled"),
            "Error must mention cancellation, got: {err_str}"
        );

        // Required mode
        let tool_req = GetContextTool {
            query: "process_data".to_string(),
            max_tokens: None,
            workspace: Some("primary".to_string()),
            language: None,
            file_pattern: None,
            format: None,
            edited_files: None,
            entry_symbols: None,
            stack_trace: None,
            failing_test: None,
            max_hops: None,
            prefer_tests: None,
            semantics: Some(SemanticMode::Required),
        };

        let res_req = tool_req
            .call_tool_with_target_and_budget(&context, WorkspaceTarget::Primary, Some(budget))
            .await;

        assert!(res_req.is_err());
        assert!(res_req.unwrap_err().to_string().contains("cancelled"));
    }

    // ── Challenge 6: Required mode in FastSearchTool fails closed on missing provider ────

    #[tokio::test]
    async fn challenge_search_execution_required_mode_fails_closed_when_provider_missing() {
        let tmp = TempDir::new().unwrap();
        let idx_dir = TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let _db = SymbolDatabase::new(&db_path).unwrap();
        let index = Arc::new(SearchIndex::create(idx_dir.path()).unwrap());

        // Context with no embedding provider
        let context = FakeToolContext::new()
            .with_workspace_id("primary")
            .with_primary_db_path(db_path)
            .with_search_index(index);

        let tool = FastSearchTool {
            query: "process_data".to_string(),
            language: None,
            file_pattern: None,
            limit: 6,
            context_lines: None,
            exclude_tests: None,
            backend: Some(crate::search::SearchBackend::Semantic),
            workspace: Some("primary".to_string()),
            return_format: "full".to_string(),
            semantics: Some(SemanticMode::Required),
        };

        let res = tool.call_tool(&context).await;
        assert!(
            res.is_err(),
            "Required mode MUST fail closed when provider is missing"
        );
        let err_str = res.unwrap_err().to_string();
        assert!(
            err_str.contains("SEMANTICS_NOT_READY"),
            "Error must contain SEMANTICS_NOT_READY, got: {err_str}"
        );
    }

    // ── Challenge 7: Tagged readers reject stale canonical revision ────

    #[test]
    fn challenge_tagged_readers_reject_stale_canonical_revision_in_required_mode() {
        let tmp = TempDir::new().unwrap();
        let idx_dir = TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let mut db = SymbolDatabase::new(&db_path).unwrap();
        let index = SearchIndex::create(idx_dir.path()).unwrap();

        let provider = MockTestProvider::new("test-mock", 384);
        let key = provider.encoder_identity().unwrap().storage_key().unwrap();
        // Ready generation at rev 0
        db.publish_test_generation(&key, 0, 384).unwrap();

        // Now canonical revision advances to 1
        db.conn
            .execute(
                "INSERT INTO canonical_revisions (revision, workspace_id, kind, created_at) VALUES (1, 'ws1', 'incremental', 0)",
                [],
            )
            .unwrap();

        // Query tagged with rev 0 (stale compared to canonical revision 1)
        let stale_tagged = TaggedQueryEmbedding {
            vector: vec![0.1_f32; 384],
            encoder_key: key.clone(),
            source_revision: 0,
        };

        // 1. similarity::find_similar_by_tagged_query in Required mode must fail closed
        let sim_res = similarity::find_similar_by_tagged_query(
            &db,
            &stale_tagged,
            5,
            0.0,
            SemanticMode::Required,
        );
        assert!(
            sim_res.is_err(),
            "Must reject stale revision in Required mode"
        );
        let err_str = sim_res.unwrap_err().to_string();
        assert!(
            err_str.contains("SEMANTICS_NOT_READY"),
            "Error must contain SEMANTICS_NOT_READY, got: {err_str}"
        );
        assert!(err_str.contains("differs from current canonical revision"));

        // 2. hybrid_search_with_tagged_embedding in Required mode must fail closed
        let hybrid_res = hybrid_search_with_tagged_embedding(
            "test query",
            &SearchFilter::default(),
            5,
            &index,
            &db,
            Some(stale_tagged),
            None,
            SemanticMode::Required,
        );
        let err = match hybrid_res {
            Err(e) => e,
            Ok(_) => panic!("Must reject stale revision in Required mode"),
        };
        let err_str = err.to_string();
        assert!(
            err_str.contains("SEMANTICS_NOT_READY"),
            "Error must contain SEMANTICS_NOT_READY, got: {err_str}"
        );
        assert!(err_str.contains("differs from current canonical revision"));
    }
}
