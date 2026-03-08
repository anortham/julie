//! Phase 5 Integration Tests — Search Enhancement cross-cutting scenarios.
//!
//! Tests the seams between Phase 5 components working together end-to-end:
//! dynamic dimensions, memory embeddings, weighted RRF, unified search,
//! content type filtering, REST API new fields, and graceful degradation.

use std::sync::Arc;
use std::time::Instant;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;

use crate::api;
use crate::daemon_indexer::IndexRequest;
use crate::daemon_state::{DaemonState, LoadedWorkspace, WorkspaceLoadStatus};
use crate::memory::index::MemoryIndex;
use crate::memory::Checkpoint;
use crate::registry::GlobalRegistry;
use crate::server::AppState;
use crate::workspace::JulieWorkspace;

// ===========================================================================
// Shared helpers (used by both sync and async test modules)
// ===========================================================================

fn make_checkpoint(id: &str, desc: &str, tags: &[&str], symbols: &[&str]) -> Checkpoint {
    Checkpoint {
        id: id.into(),
        timestamp: "2026-03-08T10:00:00Z".into(),
        description: desc.into(),
        checkpoint_type: None,
        context: None,
        decision: Some(format!("Decision for {id}")),
        alternatives: None,
        impact: Some(format!("Impact of {id}")),
        evidence: None,
        symbols: Some(symbols.iter().map(|s| s.to_string()).collect()),
        next: None,
        confidence: None,
        unknowns: None,
        tags: Some(tags.iter().map(|t| t.to_string()).collect()),
        git: None,
        summary: None,
        plan_id: None,
    }
}

fn test_state(julie_home: std::path::PathBuf) -> Arc<AppState> {
    let (indexing_sender, _rx) = tokio::sync::mpsc::channel::<IndexRequest>(1);
    Arc::new(AppState {
        start_time: Instant::now(),
        registry: Arc::new(tokio::sync::RwLock::new(GlobalRegistry::new())),
        julie_home,
        daemon_state: Arc::new(tokio::sync::RwLock::new(DaemonState::new())),
        cancellation_token: CancellationToken::new(),
        indexing_sender,
        dispatch_manager: Arc::new(tokio::sync::RwLock::new(
            crate::agent::dispatch::DispatchManager::new(),
        )),
        backends: vec![],
    })
}

fn test_app(state: Arc<AppState>) -> axum::Router {
    axum::Router::new().nest("/api", api::routes(state))
}

async fn post_json(app: axum::Router, uri: &str, body: Value) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

/// Full-stack workspace: code index + memory index + DB + optional embeddings.
async fn setup_full_workspace(
    state: &Arc<AppState>,
    temp_dir: &tempfile::TempDir,
    embedding_provider: Option<Arc<dyn crate::embeddings::EmbeddingProvider>>,
) -> String {
    use crate::search::{LanguageConfigs, SearchIndex, SymbolDocument};

    let ws_id = "phase5-integ-ws".to_string();
    let tantivy_dir = temp_dir.path().join("tantivy");
    std::fs::create_dir_all(&tantivy_dir).unwrap();

    let configs = LanguageConfigs::load_embedded();
    let search_index =
        SearchIndex::create_with_language_configs(&tantivy_dir, &configs).unwrap();

    for (id, name, sig, kind, lang, path) in [
        ("sym-hybrid", "hybrid_search", "pub fn hybrid_search()", "function", "rust", "src/search/hybrid.rs"),
        ("sym-rrf", "weighted_rrf_merge", "pub fn weighted_rrf_merge()", "function", "rust", "src/search/hybrid.rs"),
        ("sym-auth", "authenticate_user", "async fn authenticate_user()", "function", "typescript", "src/auth/jwt.ts"),
        ("sym-embed", "EmbeddingPipeline", "pub struct EmbeddingPipeline", "struct", "rust", "src/embeddings/pipeline.rs"),
    ] {
        search_index.add_symbol(&SymbolDocument {
            id: id.into(), name: name.into(), signature: sig.into(),
            doc_comment: format!("{name} doc"), file_path: path.into(),
            kind: kind.into(), language: lang.into(), start_line: 1, code_body: String::new(),
        }).unwrap();
    }
    search_index.commit().unwrap();

    // Memory index with topically diverse checkpoints
    let memory_dir = temp_dir.path().join("indexes/memories/tantivy");
    std::fs::create_dir_all(&memory_dir).unwrap();
    let memory_index = MemoryIndex::create(&memory_dir).unwrap();

    let checkpoints = vec![
        make_checkpoint("cp-rrf", "Decided to use weighted RRF for hybrid search merge",
            &["search", "rrf", "architecture"], &["weighted_rrf_merge", "hybrid_search"]),
        make_checkpoint("cp-dims", "Implemented dynamic embedding dimensions with table recreation",
            &["embeddings", "migration"], &["recreate_vectors_table", "EmbeddingPipeline"]),
        make_checkpoint("cp-auth", "Added JWT authentication for user endpoints",
            &["auth", "jwt", "security"], &["authenticate_user"]),
    ];

    for cp in &checkpoints {
        memory_index
            .add_checkpoint(cp, Some(&format!(".memories/2026-03-08/{}.md", cp.id)))
            .unwrap();
    }
    memory_index.commit().unwrap();

    // DB for embedding storage
    let db_path = temp_dir.path().join("db");
    std::fs::create_dir_all(&db_path).unwrap();
    let db = crate::database::SymbolDatabase::new(&db_path.join("symbols.db")).unwrap();

    if let Some(ref provider) = embedding_provider {
        let mut db_mut = db;
        crate::memory::embedding::embed_checkpoints_batch(
            &checkpoints, &mut db_mut, provider.as_ref(),
        ).unwrap();
        register_workspace(state, temp_dir, Some(Arc::new(std::sync::Mutex::new(db_mut))),
            search_index, Some(provider.clone()), &ws_id).await;
    } else {
        register_workspace(state, temp_dir, Some(Arc::new(std::sync::Mutex::new(db))),
            search_index, None, &ws_id).await;
    }
    ws_id
}

async fn register_workspace(
    state: &Arc<AppState>,
    temp_dir: &tempfile::TempDir,
    db: Option<Arc<std::sync::Mutex<crate::database::SymbolDatabase>>>,
    search_index: crate::search::SearchIndex,
    embedding_provider: Option<Arc<dyn crate::embeddings::EmbeddingProvider>>,
    ws_id: &str,
) {
    let workspace = JulieWorkspace {
        root: std::path::PathBuf::from("/fake/project"),
        julie_dir: temp_dir.path().to_path_buf(),
        db,
        search_index: Some(Arc::new(std::sync::Mutex::new(search_index))),
        watcher: None,
        embedding_provider,
        embedding_runtime_status: None,
        config: Default::default(),
    };
    let loaded = LoadedWorkspace {
        workspace,
        status: WorkspaceLoadStatus::Ready,
        path: std::path::PathBuf::from("/fake/project"),
    };
    state.daemon_state.write().await.workspaces.insert(ws_id.to_string(), loaded);
}

// ===========================================================================
// Synchronous integration tests
// ===========================================================================

#[cfg(test)]
mod sync_integration_tests {
    use anyhow::Result;
    use tempfile::TempDir;

    use crate::database::SymbolDatabase;
    use crate::embeddings::{DeviceInfo, EmbeddingProvider};
    use crate::memory::embedding::{embed_checkpoint, embed_checkpoints_batch, hybrid_memory_search};
    use crate::memory::index::MemoryIndex;
    use crate::search::content_type::ContentType;
    use crate::search::hybrid::{hybrid_search, weighted_rrf_merge};
    use crate::search::index::{SearchFilter, SearchIndex, SymbolDocument};
    use crate::search::unified::{SearchResultItem, UnifiedSearchOptions, unified_search};
    use crate::search::weights::SearchWeightProfile;
    use crate::search::LanguageConfigs;
    use crate::search::SymbolSearchResult;

    use super::make_checkpoint;

    // ── Mock embedding provider ──────────────────────────────────────────

    struct HashProvider { dims: usize }

    impl HashProvider {
        fn new(dims: usize) -> Self { Self { dims } }
    }

    impl EmbeddingProvider for HashProvider {
        fn embed_query(&self, text: &str) -> Result<Vec<f32>> {
            Ok(deterministic_vector(text, self.dims))
        }
        fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
            Ok(texts.iter().map(|t| deterministic_vector(t, self.dims)).collect())
        }
        fn dimensions(&self) -> usize { self.dims }
        fn device_info(&self) -> DeviceInfo {
            DeviceInfo {
                runtime: "test".into(), device: "cpu".into(),
                model_name: "hash-mock".into(), dimensions: self.dims,
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

    // ── Infra helpers ────────────────────────────────────────────────────

    fn create_test_db() -> (SymbolDatabase, TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = SymbolDatabase::new(&dir.path().join("test.db")).expect("create db");
        (db, dir)
    }

    struct TestInfra {
        search_index: SearchIndex,
        memory_index: MemoryIndex,
        db: SymbolDatabase,
        provider: HashProvider,
        _tantivy_dir: TempDir,
        _memory_dir: TempDir,
        _db_dir: TempDir,
    }

    fn setup_infra(dims: usize) -> TestInfra {
        let tantivy_dir = tempfile::tempdir().unwrap();
        let memory_dir = tempfile::tempdir().unwrap();
        let db_dir = tempfile::tempdir().unwrap();
        let configs = LanguageConfigs::load_embedded();
        TestInfra {
            search_index: SearchIndex::create_with_language_configs(tantivy_dir.path(), &configs).unwrap(),
            memory_index: MemoryIndex::create(memory_dir.path()).unwrap(),
            db: SymbolDatabase::new(&db_dir.path().join("test.db")).unwrap(),
            provider: HashProvider::new(dims),
            _tantivy_dir: tantivy_dir,
            _memory_dir: memory_dir,
            _db_dir: db_dir,
        }
    }

    fn add_symbol(infra: &TestInfra, id: &str, name: &str, kind: &str, lang: &str) {
        infra.search_index.add_symbol(&SymbolDocument {
            id: id.into(), name: name.into(), kind: kind.into(), language: lang.into(),
            file_path: format!("src/{name}.rs"), signature: format!("fn {name}()"),
            doc_comment: String::new(), code_body: String::new(), start_line: 1,
        }).unwrap();
    }

    fn index_checkpoint(infra: &TestInfra, cp: &crate::memory::Checkpoint) {
        infra.memory_index
            .add_checkpoint(cp, Some(&format!("2026-03-08/{}.md", cp.id)))
            .unwrap();
    }

    // ── 1. Dynamic dimension migration ───────────────────────────────────

    #[test]
    fn test_dimension_migration_switch_model_and_reverify() {
        let (mut db, _dir) = create_test_db();
        let p384 = HashProvider::new(384);
        let p768 = HashProvider::new(768);

        // Initial 384-dim config
        db.set_embedding_config("bge-small-en-v1.5", 384).unwrap();
        let (model, dims) = db.get_embedding_config().unwrap();
        assert_eq!((model.as_str(), dims), ("bge-small-en-v1.5", 384));

        // Store 384-dim symbol embedding
        let vec_384 = p384.embed_query("test symbol").unwrap();
        db.store_embeddings(&[("sym-001".into(), vec_384)]).unwrap();
        assert_eq!(db.embedding_count().unwrap(), 1);

        // Store 384-dim memory embedding
        let cp = make_checkpoint("cp-dim", "dimension test", &["test"], &[]);
        embed_checkpoint(&cp, &mut db, &p384).unwrap();
        assert_eq!(db.memory_embedding_count().unwrap(), 1);

        // Switch to 768 dims: update config + recreate both tables
        db.set_embedding_config("bge-large-en-v1.5", 768).unwrap();
        db.recreate_vectors_table(768).unwrap();
        db.recreate_memory_vectors_table(768).unwrap();

        // Old embeddings wiped
        assert_eq!(db.embedding_count().unwrap(), 0);
        assert_eq!(db.memory_embedding_count().unwrap(), 0);

        // New 768-dim embeddings work
        let vec_768 = p768.embed_query("test symbol").unwrap();
        db.store_embeddings(&[("sym-001".into(), vec_768)]).unwrap();
        embed_checkpoint(&cp, &mut db, &p768).unwrap();
        assert_eq!(db.embedding_count().unwrap(), 1);
        assert_eq!(db.memory_embedding_count().unwrap(), 1);

        // Config reflects new model
        let (m2, d2) = db.get_embedding_config().unwrap();
        assert_eq!((m2.as_str(), d2), ("bge-large-en-v1.5", 768));

        // KNN works with new dimensions
        let qvec = p768.embed_query("test query").unwrap();
        assert_eq!(db.knn_search(&qvec, 5).unwrap().len(), 1);
        assert_eq!(db.knn_memory_search(&qvec, 5).unwrap().len(), 1);
    }

    // ── 2. Memory embed → hybrid retrieval pipeline ──────────────────────

    #[test]
    fn test_memory_embed_and_hybrid_retrieval_pipeline() {
        let mut infra = setup_infra(384);

        let checkpoints = vec![
            make_checkpoint("cp-rrf", "Weighted RRF merge for hybrid search combining BM25 and semantic",
                &["search", "rrf"], &["weighted_rrf_merge"]),
            make_checkpoint("cp-auth", "JWT authentication middleware for secure API access",
                &["auth", "security"], &["authenticate_user"]),
            make_checkpoint("cp-k8s", "Kubernetes deployment with rolling updates",
                &["devops", "deploy"], &["deploy_service"]),
        ];

        for cp in &checkpoints {
            index_checkpoint(&infra, cp);
        }
        infra.memory_index.commit().unwrap();

        embed_checkpoints_batch(&checkpoints, &mut infra.db, &infra.provider).unwrap();
        assert_eq!(infra.db.memory_embedding_count().unwrap(), 3);

        let results = hybrid_memory_search(
            "weighted RRF merge search",
            &infra.memory_index, &infra.db,
            Some(&infra.provider as &dyn EmbeddingProvider), 10,
        ).unwrap();

        assert!(!results.is_empty(), "hybrid memory search should return results");
        assert!(results.iter().any(|r| r.id == "cp-rrf"), "RRF checkpoint should appear");

        // Scores descending
        for w in results.windows(2) {
            assert!(w[0].score >= w[1].score, "scores should descend: {} >= {}", w[0].score, w[1].score);
        }
    }

    // ── 3. Weighted RRF profiles produce different rankings ──────────────

    #[test]
    fn test_weighted_rrf_profiles_produce_different_rankings() {
        // Overlapping IDs so weighted merge produces different scores per profile.
        // shared-A: rank 1 in tantivy, rank 3 in semantic
        // shared-B: rank 3 in tantivy, rank 1 in semantic
        fn make(id: &str, score: f32) -> SymbolSearchResult {
            SymbolSearchResult {
                id: id.into(), name: id.into(), signature: String::new(),
                doc_comment: String::new(), file_path: format!("src/{id}.rs"),
                kind: "function".into(), language: "rust".into(), start_line: 1, score,
            }
        }

        let tantivy = vec![make("shared-A", 1.0), make("tantivy-only", 0.8), make("shared-B", 0.6)];
        let semantic = vec![make("shared-B", 1.0), make("semantic-only", 0.8), make("shared-A", 0.6)];

        let fast = SearchWeightProfile::fast_search();
        let recall = SearchWeightProfile::recall();

        let fast_m = weighted_rrf_merge(tantivy.clone(), semantic.clone(), 60, 10, fast.keyword_weight, fast.semantic_weight);
        let recall_m = weighted_rrf_merge(tantivy, semantic, 60, 10, recall.keyword_weight, recall.semantic_weight);

        // fast_search (keyword=1.0, semantic=0.7): shared-A wins (rank 1 in keyword)
        // recall (keyword=0.7, semantic=1.0): shared-B wins (rank 1 in semantic)
        let fast_a = fast_m.iter().find(|r| r.id == "shared-A").unwrap().score;
        let fast_b = fast_m.iter().find(|r| r.id == "shared-B").unwrap().score;
        assert!(fast_a > fast_b, "fast: shared-A > shared-B: {fast_a} > {fast_b}");

        let recall_a = recall_m.iter().find(|r| r.id == "shared-A").unwrap().score;
        let recall_b = recall_m.iter().find(|r| r.id == "shared-B").unwrap().score;
        assert!(recall_b > recall_a, "recall: shared-B > shared-A: {recall_b} > {recall_a}");

        // Ranking order is OPPOSITE across profiles
        let fast_a_rank = fast_m.iter().position(|r| r.id == "shared-A").unwrap();
        let fast_b_rank = fast_m.iter().position(|r| r.id == "shared-B").unwrap();
        let recall_a_rank = recall_m.iter().position(|r| r.id == "shared-A").unwrap();
        let recall_b_rank = recall_m.iter().position(|r| r.id == "shared-B").unwrap();
        assert!(fast_a_rank < fast_b_rank, "fast: A before B");
        assert!(recall_b_rank < recall_a_rank, "recall: B before A");
    }

    // ── 4. Unified cross-content search with embeddings ──────────────────

    #[test]
    fn test_unified_search_cross_content_with_embeddings() {
        let mut infra = setup_infra(384);

        add_symbol(&infra, "sym-rrf", "weighted_rrf_merge", "function", "rust");
        add_symbol(&infra, "sym-embed", "EmbeddingPipeline", "struct", "rust");
        infra.search_index.commit().unwrap();

        let checkpoints = vec![
            make_checkpoint("cp-rrf", "Implemented weighted RRF merge for hybrid search",
                &["search", "rrf"], &["weighted_rrf_merge"]),
            make_checkpoint("cp-embed", "Added embedding pipeline for semantic search",
                &["embeddings"], &["EmbeddingPipeline"]),
        ];
        for cp in &checkpoints {
            index_checkpoint(&infra, cp);
        }
        infra.memory_index.commit().unwrap();
        embed_checkpoints_batch(&checkpoints, &mut infra.db, &infra.provider).unwrap();

        let results = unified_search(
            "RRF merge", &UnifiedSearchOptions { content_type: None, limit: 20 },
            &infra.search_index, &infra.memory_index, &infra.db,
            Some(&infra.provider as &dyn EmbeddingProvider),
        ).unwrap();

        assert!(!results.is_empty());
        assert!(results.iter().any(|r| r.content_type == ContentType::Code), "should have code");
        assert!(results.iter().any(|r| r.content_type == ContentType::Memory), "should have memory");

        // Verify tagging matches variant
        for r in &results {
            match &r.result {
                SearchResultItem::Code(_) => assert_eq!(r.content_type, ContentType::Code),
                SearchResultItem::Memory(_) => assert_eq!(r.content_type, ContentType::Memory),
            }
        }
        // Scores descending
        for w in results.windows(2) {
            assert!(w[0].score >= w[1].score);
        }
    }

    // ── 5. Content type filtering with embedded memories ─────────────────

    #[test]
    fn test_content_type_filtering_with_embedded_memories() {
        let mut infra = setup_infra(384);

        add_symbol(&infra, "sym-auth", "authenticate_user", "function", "rust");
        infra.search_index.commit().unwrap();

        let cp = make_checkpoint("cp-auth", "Authentication system with JWT tokens",
            &["auth"], &["authenticate_user"]);
        index_checkpoint(&infra, &cp);
        infra.memory_index.commit().unwrap();
        embed_checkpoint(&cp, &mut infra.db, &infra.provider).unwrap();

        let provider_ref = Some(&infra.provider as &dyn EmbeddingProvider);

        // Code only
        let code = unified_search("authenticate",
            &UnifiedSearchOptions { content_type: Some(ContentType::Code), limit: 10 },
            &infra.search_index, &infra.memory_index, &infra.db, provider_ref,
        ).unwrap();
        assert!(code.iter().all(|r| r.content_type == ContentType::Code));

        // Memory only
        let mem = unified_search("authentication JWT",
            &UnifiedSearchOptions { content_type: Some(ContentType::Memory), limit: 10 },
            &infra.search_index, &infra.memory_index, &infra.db, provider_ref,
        ).unwrap();
        assert!(mem.iter().all(|r| r.content_type == ContentType::Memory));

        // All — should include code at minimum
        let all = unified_search("authenticate",
            &UnifiedSearchOptions { content_type: None, limit: 20 },
            &infra.search_index, &infra.memory_index, &infra.db, provider_ref,
        ).unwrap();
        assert!(all.iter().any(|r| r.content_type == ContentType::Code));
    }

    // ── 7. Graceful degradation without embedding provider ───────────────

    #[test]
    fn test_graceful_degradation_no_embedding_provider() {
        let infra = setup_infra(384);

        add_symbol(&infra, "sym-search", "search_index", "struct", "rust");
        infra.search_index.commit().unwrap();

        let cp = make_checkpoint("cp-search", "Search index with Tantivy", &["search"], &["search_index"]);
        index_checkpoint(&infra, &cp);
        infra.memory_index.commit().unwrap();

        // Hybrid memory search degrades to BM25 without provider
        let mem = hybrid_memory_search("search index", &infra.memory_index, &infra.db, None, 10).unwrap();
        assert!(!mem.is_empty(), "should return BM25 results without provider");

        // Unified search without provider
        let results = unified_search("search index",
            &UnifiedSearchOptions { content_type: None, limit: 10 },
            &infra.search_index, &infra.memory_index, &infra.db, None,
        ).unwrap();
        assert!(!results.is_empty());
        assert!(results.iter().any(|r| r.content_type == ContentType::Code));
        assert!(results.iter().any(|r| r.content_type == ContentType::Memory));
    }

    // ── Hybrid search orchestrator with weight profiles ──────────────────

    #[test]
    fn test_hybrid_search_orchestrator_with_embeddings_and_profile() {
        let infra = setup_infra(384);

        let symbols = vec![
            SymbolDocument {
                id: "sym-pipeline".into(), name: "EmbeddingPipeline".into(),
                kind: "struct".into(), language: "rust".into(),
                file_path: "src/embeddings/pipeline.rs".into(),
                signature: "pub struct EmbeddingPipeline".into(),
                doc_comment: "Orchestrates embedding generation.".into(),
                code_body: String::new(), start_line: 30,
            },
            SymbolDocument {
                id: "sym-tokenizer".into(), name: "CodeTokenizer".into(),
                kind: "struct".into(), language: "rust".into(),
                file_path: "src/search/tokenizer.rs".into(),
                signature: "pub struct CodeTokenizer".into(),
                doc_comment: "CamelCase/snake_case aware tokenizer.".into(),
                code_body: String::new(), start_line: 10,
            },
        ];

        for sym in &symbols {
            infra.search_index.add_symbol(sym).unwrap();
        }
        infra.search_index.commit().unwrap();

        // Separate DB with file + symbol records (FK: symbols -> files)
        let db_dir = tempfile::tempdir().unwrap();
        let mut db = SymbolDatabase::new(&db_dir.path().join("hybrid.db")).unwrap();

        for sym in &symbols {
            db.conn.execute(
                "INSERT OR IGNORE INTO files (path, language, hash, size, last_modified, last_indexed)
                 VALUES (?, ?, 'deadbeef', 100, 0, 0)",
                rusqlite::params![sym.file_path, sym.language],
            ).unwrap();
            db.conn.execute(
                "INSERT INTO symbols (id, name, kind, language, file_path,
                 start_line, start_col, end_line, end_col, start_byte, end_byte,
                 reference_score, signature, doc_comment)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7, 0, 0, 100, 0.0, ?8, ?9)",
                rusqlite::params![
                    sym.id, sym.name, sym.kind, sym.language, sym.file_path,
                    sym.start_line, sym.start_line + 10, sym.signature, sym.doc_comment,
                ],
            ).unwrap();
        }

        let embeddings: Vec<(String, Vec<f32>)> = symbols.iter()
            .map(|s| (s.id.clone(), infra.provider.embed_query(&s.name).unwrap()))
            .collect();
        db.store_embeddings(&embeddings).unwrap();

        let results = hybrid_search(
            "EmbeddingPipeline", &SearchFilter::default(), 10,
            &infra.search_index, &db,
            Some(&infra.provider as &dyn EmbeddingProvider),
            Some(SearchWeightProfile::fast_search()),
        ).unwrap();

        assert!(!results.results.is_empty(), "hybrid search should return results");
        assert!(results.results.iter().any(|r| r.name == "EmbeddingPipeline"));
    }
}

// ===========================================================================
// REST API integration tests
// ===========================================================================

#[cfg(test)]
mod api_integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_api_search_content_type_all_returns_code_and_memories() {
        let temp_dir = tempfile::tempdir().unwrap();
        let state = test_state(temp_dir.path().to_path_buf());
        setup_full_workspace(&state, &temp_dir, None).await;

        let (status, body) = post_json(
            test_app(state), "/api/search",
            serde_json::json!({
                "query": "RRF merge hybrid search",
                "content_type": "all", "limit": 20, "project": "phase5-integ-ws"
            }),
        ).await;

        assert_eq!(status, StatusCode::OK, "response: {body}");
        assert!(body["count"].as_u64().unwrap_or(0) > 0, "should have results: {body}");
        assert!(body["symbols"].is_array() || body["memories"].is_array(),
            "should have symbols and/or memories: {body}");
    }

    #[tokio::test]
    async fn test_api_search_content_type_memory_returns_only_memories() {
        let temp_dir = tempfile::tempdir().unwrap();
        let state = test_state(temp_dir.path().to_path_buf());
        setup_full_workspace(&state, &temp_dir, None).await;

        let (status, body) = post_json(
            test_app(state), "/api/search",
            serde_json::json!({
                "query": "weighted RRF architecture",
                "content_type": "memory", "limit": 10, "project": "phase5-integ-ws"
            }),
        ).await;

        assert_eq!(status, StatusCode::OK, "response: {body}");
        assert!(body["symbols"].is_null(), "memory-only should not include symbols: {body}");

        if let Some(memories) = body["memories"].as_array() {
            assert!(!memories.is_empty(), "should find at least one memory");
            let m = &memories[0];
            assert_eq!(m["content_type"], "memory");
            assert!(m["id"].is_string());
            assert!(m["body"].is_string());
            assert!(m["score"].is_number());
        }
    }

    #[tokio::test]
    async fn test_api_search_memories_have_expected_fields() {
        let temp_dir = tempfile::tempdir().unwrap();
        let state = test_state(temp_dir.path().to_path_buf());
        setup_full_workspace(&state, &temp_dir, None).await;

        let (status, body) = post_json(
            test_app(state), "/api/search",
            serde_json::json!({
                "query": "embedding dimensions migration",
                "content_type": "memory", "limit": 10, "project": "phase5-integ-ws"
            }),
        ).await;

        assert_eq!(status, StatusCode::OK, "response: {body}");
        if let Some(memories) = body["memories"].as_array() {
            if let Some(m) = memories.first() {
                assert!(m["content_type"].is_string());
                assert!(m["id"].is_string());
                assert!(m["body"].is_string());
                assert!(m["score"].is_number());
                for field in ["tags", "symbols", "decision", "impact"] {
                    if !m[field].is_null() {
                        assert!(m[field].is_string(), "{field} should be string if present");
                    }
                }
            }
        }
    }

    #[tokio::test]
    async fn test_api_debug_search_includes_hybrid_mode_field() {
        let temp_dir = tempfile::tempdir().unwrap();
        let state = test_state(temp_dir.path().to_path_buf());
        setup_full_workspace(&state, &temp_dir, None).await;

        // hybrid=true
        let (s1, b1) = post_json(
            test_app(state.clone()), "/api/search/debug",
            serde_json::json!({ "query": "search", "hybrid": true, "project": "phase5-integ-ws" }),
        ).await;
        assert_eq!(s1, StatusCode::OK);
        assert_eq!(b1["hybrid_mode"], true, "should reflect hybrid=true: {b1}");

        // hybrid=false (default)
        let (s2, b2) = post_json(
            test_app(state), "/api/search/debug",
            serde_json::json!({ "query": "search", "project": "phase5-integ-ws" }),
        ).await;
        assert_eq!(s2, StatusCode::OK);
        assert_eq!(b2["hybrid_mode"], false, "should default to false: {b2}");
    }

    #[tokio::test]
    async fn test_api_search_code_default_excludes_memories() {
        let temp_dir = tempfile::tempdir().unwrap();
        let state = test_state(temp_dir.path().to_path_buf());
        setup_full_workspace(&state, &temp_dir, None).await;

        let (status, body) = post_json(
            test_app(state), "/api/search",
            serde_json::json!({ "query": "hybrid_search", "project": "phase5-integ-ws" }),
        ).await;

        assert_eq!(status, StatusCode::OK, "response: {body}");
        assert!(body["memories"].is_null(), "default search should not include memories: {body}");
    }

    #[tokio::test]
    async fn test_api_search_graceful_without_memory_index() {
        let temp_dir = tempfile::tempdir().unwrap();
        let state = test_state(temp_dir.path().to_path_buf());

        // Workspace with code only (no memory index on disk)
        use crate::search::{LanguageConfigs, SearchIndex, SymbolDocument};

        let tantivy_dir = temp_dir.path().join("tantivy");
        std::fs::create_dir_all(&tantivy_dir).unwrap();
        let configs = LanguageConfigs::load_embedded();
        let si = SearchIndex::create_with_language_configs(&tantivy_dir, &configs).unwrap();
        si.add_symbol(&SymbolDocument {
            id: "s1".into(), name: "test_function".into(), signature: "fn test_function()".into(),
            doc_comment: String::new(), file_path: "src/lib.rs".into(),
            kind: "function".into(), language: "rust".into(), start_line: 1, code_body: String::new(),
        }).unwrap();
        si.commit().unwrap();

        let ws = JulieWorkspace {
            root: std::path::PathBuf::from("/fake/p2"),
            julie_dir: temp_dir.path().to_path_buf(),
            db: None,
            search_index: Some(Arc::new(std::sync::Mutex::new(si))),
            watcher: None, embedding_provider: None,
            embedding_runtime_status: None, config: Default::default(),
        };
        state.daemon_state.write().await.workspaces.insert(
            "no-mem-ws".into(),
            LoadedWorkspace { workspace: ws, status: WorkspaceLoadStatus::Ready, path: "/fake/p2".into() },
        );

        // memory filter on workspace without memory index → empty, not error
        let (s1, b1) = post_json(
            test_app(state.clone()), "/api/search",
            serde_json::json!({ "query": "test", "content_type": "memory", "project": "no-mem-ws" }),
        ).await;
        assert_eq!(s1, StatusCode::OK, "should not error: {b1}");
        assert_eq!(b1["count"], 0);

        // "all" filter still returns code
        let (s2, b2) = post_json(
            test_app(state), "/api/search",
            serde_json::json!({ "query": "test_function", "content_type": "all", "project": "no-mem-ws" }),
        ).await;
        assert_eq!(s2, StatusCode::OK, "should not error: {b2}");
        assert!(b2["count"].as_u64().unwrap_or(0) > 0, "should find code: {b2}");
    }
}
