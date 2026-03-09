//! Phase 4 Integration Tests — End-to-end REST API verification.
//!
//! These tests exercise the full HTTP stack with realistic state:
//! - Real Tantivy search indexes with indexed symbols and file content
//! - Real checkpoint/plan files written to disk
//! - Real DispatchManager with actual dispatches
//! - Full axum Router built from `api::routes`
//!
//! Unlike the per-endpoint test files (which use minimal mocks), these tests
//! combine multiple subsystems to verify realistic E2E flows.

use std::sync::Arc;
use std::time::Instant;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use tower::ServiceExt; // for `oneshot`

use crate::agent::backend::BackendInfo;
use crate::agent::dispatch::DispatchManager;
use crate::api;
use crate::daemon_indexer::IndexRequest;
use crate::daemon_state::{DaemonState, LoadedWorkspace, WorkspaceLoadStatus};
use crate::memory::storage::format_checkpoint;
use crate::memory::{Checkpoint, CheckpointType};
use crate::registry::GlobalRegistry;
use crate::search::{FileDocument, LanguageConfigs, SearchIndex, SymbolDocument};
use crate::server::AppState;
use crate::workspace::JulieWorkspace;

// ===========================================================================
// Shared helpers
// ===========================================================================

/// Build a fully-populated AppState suitable for integration testing.
///
/// Includes backends and a DispatchManager so dashboard/agent tests work.
fn integration_state(julie_home: std::path::PathBuf) -> Arc<AppState> {
    let (indexing_sender, _rx) = tokio::sync::mpsc::channel::<IndexRequest>(1);
    let backends = vec![
        BackendInfo {
            name: "claude".to_string(),
            available: true,
            version: Some("1.0.0".to_string()),
        },
        BackendInfo {
            name: "test-backend".to_string(),
            available: false,
            version: None,
        },
    ];
    let registry = Arc::new(tokio::sync::RwLock::new(GlobalRegistry::new()));
    let cancellation_token = CancellationToken::new();
    Arc::new(AppState {
        start_time: Instant::now(),
        registry: registry.clone(),
        julie_home: julie_home.clone(),
        daemon_state: Arc::new(tokio::sync::RwLock::new(DaemonState::new(
            registry,
            julie_home,
            cancellation_token.clone(),
        ))),
        cancellation_token,
        indexing_sender,
        dispatch_manager: Arc::new(tokio::sync::RwLock::new(
            DispatchManager::with_backends(backends.clone()),
        )),
        backends,
    })
}

/// Build the test app with the `/api` nest.
fn test_app(state: Arc<AppState>) -> axum::Router {
    axum::Router::new().nest("/api", api::routes(state))
}

/// Register a workspace that has a real Tantivy search index with symbols
/// *and* file content. Returns the workspace ID.
async fn setup_search_workspace(
    state: &Arc<AppState>,
    temp_dir: &tempfile::TempDir,
) -> String {
    let ws_id = "integ-search-ws".to_string();
    let tantivy_dir = temp_dir.path().join("tantivy");
    std::fs::create_dir_all(&tantivy_dir).unwrap();

    let configs = LanguageConfigs::load_embedded();
    let search_index =
        SearchIndex::create_with_language_configs(&tantivy_dir, &configs).unwrap();

    // Index symbols across multiple languages
    let symbols = vec![
        SymbolDocument {
            id: "s1".into(),
            name: "AuthMiddleware".into(),
            signature: "pub struct AuthMiddleware".into(),
            doc_comment: "HTTP authentication middleware.".into(),
            file_path: "src/middleware/auth.rs".into(),
            kind: "struct".into(),
            language: "rust".into(),
            start_line: 10,
            code_body: "pub struct AuthMiddleware { secret: String }".into(),
        },
        SymbolDocument {
            id: "s2".into(),
            name: "verify_token".into(),
            signature: "pub fn verify_token(&self, token: &str) -> Result<Claims>".into(),
            doc_comment: "Verify a JWT token and return claims.".into(),
            file_path: "src/middleware/auth.rs".into(),
            kind: "method".into(),
            language: "rust".into(),
            start_line: 25,
            code_body: "pub fn verify_token(&self, token: &str) -> Result<Claims> { ... }".into(),
        },
        SymbolDocument {
            id: "s3".into(),
            name: "UserRepository".into(),
            signature: "class UserRepository".into(),
            doc_comment: "Database access layer for user entities.".into(),
            file_path: "src/repositories/user_repo.ts".into(),
            kind: "class".into(),
            language: "typescript".into(),
            start_line: 5,
            code_body: "class UserRepository { constructor(private db: Database) {} }".into(),
        },
        SymbolDocument {
            id: "s4".into(),
            name: "find_by_email".into(),
            signature: "async findByEmail(email: string): Promise<User | null>".into(),
            doc_comment: "Look up a user by email address.".into(),
            file_path: "src/repositories/user_repo.ts".into(),
            kind: "method".into(),
            language: "typescript".into(),
            start_line: 18,
            code_body: "async findByEmail(email: string): Promise<User | null> { return this.db.query(...); }".into(),
        },
        SymbolDocument {
            id: "s5".into(),
            name: "hash_password".into(),
            signature: "def hash_password(plain: str) -> str".into(),
            doc_comment: "Hash a plaintext password using bcrypt.".into(),
            file_path: "utils/crypto.py".into(),
            kind: "function".into(),
            language: "python".into(),
            start_line: 1,
            code_body: "def hash_password(plain: str) -> str: return bcrypt.hashpw(plain)".into(),
        },
    ];

    for sym in &symbols {
        search_index.add_symbol(sym).unwrap();
    }

    // Also index file content for content-mode search
    let files = vec![
        FileDocument {
            file_path: "src/middleware/auth.rs".into(),
            language: "rust".into(),
            content: "use jsonwebtoken;\npub struct AuthMiddleware { secret: String }\nimpl AuthMiddleware {\n    pub fn verify_token(&self, token: &str) -> Result<Claims> { ... }\n}".into(),
        },
        FileDocument {
            file_path: "src/repositories/user_repo.ts".into(),
            language: "typescript".into(),
            content: "import { Database } from './db';\nclass UserRepository {\n    constructor(private db: Database) {}\n    async findByEmail(email: string): Promise<User | null> { ... }\n}".into(),
        },
    ];

    for file in &files {
        search_index.add_file_content(file).unwrap();
    }

    search_index.commit().unwrap();

    let workspace = JulieWorkspace {
        root: temp_dir.path().to_path_buf(),
        julie_dir: temp_dir.path().join(".julie"),
        db: None,
        search_index: Some(Arc::new(std::sync::Mutex::new(search_index))),
        watcher: None,
        embedding_provider: None,
        embedding_runtime_status: None,
        config: Default::default(),
    };

    let loaded = LoadedWorkspace {
        workspace,
        status: WorkspaceLoadStatus::Ready,
        path: temp_dir.path().to_path_buf(),
    };

    state.daemon_state.write().await.workspaces.insert(ws_id.clone(), loaded);
    ws_id
}

/// Register a workspace with no search index (for memory/plan tests).
async fn setup_memory_workspace(
    state: &Arc<AppState>,
    root: &std::path::Path,
) -> String {
    let ws_id = "integ-mem-ws".to_string();

    let workspace = JulieWorkspace {
        root: root.to_path_buf(),
        julie_dir: root.join(".julie"),
        db: None,
        search_index: None,
        watcher: None,
        embedding_provider: None,
        embedding_runtime_status: None,
        config: Default::default(),
    };

    let loaded = LoadedWorkspace {
        workspace,
        status: WorkspaceLoadStatus::Ready,
        path: root.to_path_buf(),
    };

    state.daemon_state.write().await.workspaces.insert(ws_id.clone(), loaded);
    ws_id
}

/// Write a checkpoint to the `.memories/<date>/` dir under root.
fn write_checkpoint(root: &std::path::Path, cp: &Checkpoint) {
    let date = &cp.timestamp[..10];
    let date_dir = root.join(".memories").join(date);
    std::fs::create_dir_all(&date_dir).unwrap();

    let hash4 = cp
        .id
        .strip_prefix("checkpoint_")
        .unwrap_or(&cp.id)
        .get(..4)
        .unwrap_or("0000");
    let time_part = &cp.timestamp[11..19];
    let hhmmss = time_part.replace(':', "");
    let filename = format!("{}_{}.md", hhmmss, hash4);

    let content = format_checkpoint(cp);
    std::fs::write(date_dir.join(&filename), content).unwrap();
}

/// Write a plan file into `.memories/plans/`.
fn write_plan(root: &std::path::Path, id: &str, title: &str, status: &str) {
    let plans_dir = root.join(".memories").join("plans");
    std::fs::create_dir_all(&plans_dir).unwrap();
    let content = format!(
        "---\nid: {id}\ntitle: {title}\nstatus: {status}\ncreated: \"2026-03-07T10:00:00.000Z\"\nupdated: \"2026-03-07T10:00:00.000Z\"\ntags:\n  - integration\n---\n\n{title} plan body.\n"
    );
    std::fs::write(plans_dir.join(format!("{id}.md")), content).unwrap();
}

/// Set the active plan pointer.
fn write_active_plan(root: &std::path::Path, plan_id: &str) {
    let memories_dir = root.join(".memories");
    std::fs::create_dir_all(&memories_dir).unwrap();
    std::fs::write(memories_dir.join(".active-plan"), plan_id).unwrap();
}

/// Make a test checkpoint.
fn make_checkpoint(id: &str, timestamp: &str, description: &str) -> Checkpoint {
    Checkpoint {
        id: id.to_string(),
        timestamp: timestamp.to_string(),
        description: description.to_string(),
        checkpoint_type: Some(CheckpointType::Checkpoint),
        context: None,
        decision: None,
        alternatives: None,
        impact: None,
        evidence: None,
        symbols: None,
        next: None,
        confidence: None,
        unknowns: None,
        tags: Some(vec!["integration".to_string()]),
        git: None,
        summary: Some("Integration test checkpoint".to_string()),
        plan_id: None,
    }
}

/// POST JSON and return (status, body).
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

    if bytes.is_empty() {
        return (status, Value::Null);
    }
    let json: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

/// GET and return (status, body).
async fn get_json(app: axum::Router, uri: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("GET")
        .uri(uri)
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();

    if bytes.is_empty() {
        return (status, Value::Null);
    }
    let json: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

// ===========================================================================
// Search API — definition search with real Tantivy index
// ===========================================================================

#[tokio::test]
async fn test_integration_search_definitions_with_real_index() {
    let tmp = tempfile::tempdir().unwrap();
    let state = integration_state(tmp.path().to_path_buf());
    setup_search_workspace(&state, &tmp).await;

    let app = test_app(state);
    let (status, json) = post_json(
        app,
        "/api/search",
        serde_json::json!({
            "query": "AuthMiddleware",
            "search_target": "definitions",
            "limit": 10
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["search_target"], "definitions");
    assert!(json["count"].as_u64().unwrap() > 0, "expected results for AuthMiddleware");

    let symbols = json["symbols"].as_array().unwrap();
    // The top result should be AuthMiddleware (exact name match)
    assert_eq!(symbols[0]["name"], "AuthMiddleware");
    assert_eq!(symbols[0]["kind"], "struct");
    assert_eq!(symbols[0]["language"], "rust");
    assert_eq!(symbols[0]["file_path"], "src/middleware/auth.rs");
    assert!(symbols[0]["score"].as_f64().unwrap() > 0.0);
}

#[tokio::test]
async fn test_integration_search_cross_language_definitions() {
    let tmp = tempfile::tempdir().unwrap();
    let state = integration_state(tmp.path().to_path_buf());
    setup_search_workspace(&state, &tmp).await;

    // Search for a term that appears across languages
    let app = test_app(state);
    let (status, json) = post_json(
        app,
        "/api/search",
        serde_json::json!({
            "query": "verify_token find_by_email hash_password",
            "search_target": "definitions",
            "limit": 10
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let symbols = json["symbols"].as_array().unwrap();
    // Should get results from multiple languages
    let languages: Vec<&str> = symbols
        .iter()
        .filter_map(|s| s["language"].as_str())
        .collect();

    // At least 2 different languages should appear in results
    let unique_langs: std::collections::HashSet<&str> = languages.into_iter().collect();
    assert!(
        unique_langs.len() >= 2,
        "Expected results from at least 2 languages, got: {:?}",
        unique_langs
    );
}

#[tokio::test]
async fn test_integration_search_content_with_real_index() {
    let tmp = tempfile::tempdir().unwrap();
    let state = integration_state(tmp.path().to_path_buf());
    setup_search_workspace(&state, &tmp).await;

    let app = test_app(state);
    let (status, json) = post_json(
        app,
        "/api/search",
        serde_json::json!({
            "query": "jsonwebtoken",
            "search_target": "content",
            "limit": 10
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["search_target"], "content");

    let content = json["content"].as_array().unwrap();
    assert!(!content.is_empty(), "expected content results for 'jsonwebtoken'");
    assert_eq!(content[0]["file_path"], "src/middleware/auth.rs");
    assert_eq!(content[0]["language"], "rust");
}

#[tokio::test]
async fn test_integration_search_with_language_filter() {
    let tmp = tempfile::tempdir().unwrap();
    let state = integration_state(tmp.path().to_path_buf());
    setup_search_workspace(&state, &tmp).await;

    // Search only TypeScript symbols
    let app = test_app(state);
    let (status, json) = post_json(
        app,
        "/api/search",
        serde_json::json!({
            "query": "Repository",
            "search_target": "definitions",
            "language": "typescript",
            "limit": 10
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let symbols = json["symbols"].as_array().unwrap();
    // Every result should be TypeScript
    for sym in symbols {
        assert_eq!(
            sym["language"], "typescript",
            "Language filter should restrict results to TypeScript"
        );
    }
}

// ===========================================================================
// Search Debug API — verify scoring breakdown
// ===========================================================================

#[tokio::test]
async fn test_integration_debug_search_scoring_breakdown() {
    let tmp = tempfile::tempdir().unwrap();
    let state = integration_state(tmp.path().to_path_buf());
    setup_search_workspace(&state, &tmp).await;

    let app = test_app(state);
    let (status, json) = post_json(
        app,
        "/api/search/debug",
        serde_json::json!({
            "query": "verify_token",
            "search_target": "definitions",
            "limit": 10
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["search_target"], "definitions");

    // Debug-specific fields
    assert!(json["query_tokens"].is_array());
    let tokens = json["query_tokens"].as_array().unwrap();
    assert!(!tokens.is_empty(), "query_tokens should contain split tokens");

    // Symbols debug wrapper
    let symbols = &json["symbols"];
    assert!(symbols["results"].is_array());
    assert!(symbols["total_candidates"].is_number());

    let results = symbols["results"].as_array().unwrap();
    assert!(!results.is_empty(), "debug search should return results");

    // Verify scoring fields on first result
    let first = &results[0];
    assert!(first["name"].is_string());
    assert!(first["bm25_score"].is_number());
    assert!(first["final_score"].is_number());
    assert!(first["centrality_score"].is_number());
    assert!(first["centrality_boost"].is_number());
    assert!(first["pattern_boost"].is_number());
    assert!(first["nl_path_boost"].is_number());
    assert!(first["field_matches"].is_array());
    assert!(first["boost_explanation"].is_string());

    // BM25 and final scores should be positive for a matching result
    assert!(first["bm25_score"].as_f64().unwrap() > 0.0);
    assert!(first["final_score"].as_f64().unwrap() > 0.0);
}

#[tokio::test]
async fn test_integration_debug_search_content_mode() {
    let tmp = tempfile::tempdir().unwrap();
    let state = integration_state(tmp.path().to_path_buf());
    setup_search_workspace(&state, &tmp).await;

    let app = test_app(state);
    let (status, json) = post_json(
        app,
        "/api/search/debug",
        serde_json::json!({
            "query": "Database",
            "search_target": "content",
            "limit": 10
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["search_target"], "content");
    assert!(json["query_tokens"].is_array());

    let content = &json["content"];
    assert!(content["results"].is_array());
    assert!(content["query_tokens"].is_array());
}

// ===========================================================================
// Memories API — round-trip with real checkpoints on disk
// ===========================================================================

#[tokio::test]
async fn test_integration_memories_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    let state = integration_state(tmp.path().to_path_buf());
    setup_memory_workspace(&state, tmp.path()).await;

    // Write multiple checkpoints
    let cp1 = make_checkpoint(
        "checkpoint_aaa11111",
        "2026-03-07T10:00:00.000Z",
        "Implemented auth middleware",
    );
    let cp2 = make_checkpoint(
        "checkpoint_bbb22222",
        "2026-03-07T11:30:00.000Z",
        "Added user repository",
    );
    let cp3 = make_checkpoint(
        "checkpoint_ccc33333",
        "2026-03-07T14:00:00.000Z",
        "Wired search endpoint",
    );
    write_checkpoint(tmp.path(), &cp1);
    write_checkpoint(tmp.path(), &cp2);
    write_checkpoint(tmp.path(), &cp3);

    // List all
    let app = test_app(state.clone());
    let (status, json) = get_json(app, "/api/memories").await;

    assert_eq!(status, StatusCode::OK);
    let checkpoints = json["checkpoints"].as_array().unwrap();
    assert_eq!(checkpoints.len(), 3, "should list all 3 checkpoints");

    // Verify they're sorted by timestamp (most recent first)
    let timestamps: Vec<&str> = checkpoints
        .iter()
        .filter_map(|c| c["timestamp"].as_str())
        .collect();
    assert!(
        timestamps.windows(2).all(|w| w[0] >= w[1]),
        "checkpoints should be sorted most-recent-first, got: {:?}",
        timestamps
    );

    // Fetch single by ID
    let app = test_app(state.clone());
    let (status, json) = get_json(app, "/api/memories/checkpoint_aaa11111").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["id"], "checkpoint_aaa11111");
    assert_eq!(json["description"], "Implemented auth middleware");

    // Fetch by prefix
    let app = test_app(state.clone());
    let (status, json) = get_json(app, "/api/memories/checkpoint_bbb").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["id"], "checkpoint_bbb22222");
}

#[tokio::test]
async fn test_integration_memories_with_limit() {
    let tmp = tempfile::tempdir().unwrap();
    let state = integration_state(tmp.path().to_path_buf());
    setup_memory_workspace(&state, tmp.path()).await;

    for i in 0..5 {
        let cp = make_checkpoint(
            &format!("checkpoint_{:04x}aaaa", i),
            &format!("2026-03-07T{:02}:00:00.000Z", 10 + i),
            &format!("Checkpoint number {}", i),
        );
        write_checkpoint(tmp.path(), &cp);
    }

    let app = test_app(state);
    let (status, json) = get_json(app, "/api/memories?limit=2").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["checkpoints"].as_array().unwrap().len(), 2);
}

// ===========================================================================
// Plans API — round-trip with real plan files on disk
// ===========================================================================

#[tokio::test]
async fn test_integration_plans_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    let state = integration_state(tmp.path().to_path_buf());
    setup_memory_workspace(&state, tmp.path()).await;

    // Write two plans, make one active
    write_plan(tmp.path(), "plan-alpha", "Alpha Feature", "active");
    write_plan(tmp.path(), "plan-beta", "Beta Feature", "completed");
    write_active_plan(tmp.path(), "plan-alpha");

    // List all plans (returns a bare array)
    let app = test_app(state.clone());
    let (status, json) = get_json(app, "/api/plans").await;

    assert_eq!(status, StatusCode::OK);
    assert!(json.is_array(), "plans endpoint should return a JSON array");
    let plans = json.as_array().unwrap();
    assert_eq!(plans.len(), 2, "should list both plans");

    let titles: Vec<&str> = plans.iter().filter_map(|p| p["title"].as_str()).collect();
    assert!(titles.contains(&"Alpha Feature"));
    assert!(titles.contains(&"Beta Feature"));

    // Get single plan by ID
    let app = test_app(state.clone());
    let (status, json) = get_json(app, "/api/plans/plan-alpha").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["id"], "plan-alpha");
    assert_eq!(json["title"], "Alpha Feature");
    assert_eq!(json["status"], "active");

    // Get active plan
    let app = test_app(state.clone());
    let (status, json) = get_json(app, "/api/plans/active").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["id"], "plan-alpha");
    assert_eq!(json["title"], "Alpha Feature");
}

#[tokio::test]
async fn test_integration_plans_filter_by_status() {
    let tmp = tempfile::tempdir().unwrap();
    let state = integration_state(tmp.path().to_path_buf());
    setup_memory_workspace(&state, tmp.path()).await;

    write_plan(tmp.path(), "plan-a", "Active Plan", "active");
    write_plan(tmp.path(), "plan-c", "Completed Plan", "completed");
    write_plan(tmp.path(), "plan-d", "Draft Plan", "draft");

    let app = test_app(state.clone());
    let (status, json) = get_json(app, "/api/plans?status=completed").await;

    assert_eq!(status, StatusCode::OK);
    assert!(json.is_array());
    let plans = json.as_array().unwrap();
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0]["title"], "Completed Plan");
}

#[tokio::test]
async fn test_integration_no_active_plan_returns_404() {
    let tmp = tempfile::tempdir().unwrap();
    let state = integration_state(tmp.path().to_path_buf());
    setup_memory_workspace(&state, tmp.path()).await;

    // No .active-plan file written
    let app = test_app(state);
    let (status, _json) = get_json(app, "/api/plans/active").await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ===========================================================================
// Dashboard Stats — aggregated response shape
// ===========================================================================

#[tokio::test]
async fn test_integration_dashboard_stats_with_populated_state() {
    let tmp = tempfile::tempdir().unwrap();
    let state = integration_state(tmp.path().to_path_buf());

    // Register a search workspace (Ready) + a memory workspace
    setup_search_workspace(&state, &tmp).await;
    // Write checkpoints and an active plan under the search workspace root
    let cp = make_checkpoint(
        "checkpoint_dash1111",
        "2026-03-07T15:00:00.000Z",
        "Dashboard test checkpoint",
    );
    write_checkpoint(tmp.path(), &cp);
    write_plan(tmp.path(), "dash-plan", "Dashboard Plan", "active");
    write_active_plan(tmp.path(), "dash-plan");

    // Add a dispatch so the agents section is populated
    {
        let mut dm = state.dispatch_manager.write().await;
        let id = dm.start_dispatch("Review auth code".into(), "integ-search-ws".into(), "claude".into());
        dm.complete_dispatch(&id);
    }

    let app = test_app(state);
    let (status, json) = get_json(app, "/api/dashboard/stats").await;

    assert_eq!(status, StatusCode::OK);

    // Top-level sections must all be present
    assert!(json["projects"].is_object(), "missing 'projects' section");
    assert!(json["memories"].is_object(), "missing 'memories' section");
    assert!(json["agents"].is_object(), "missing 'agents' section");
    assert!(json["backends"].is_array(), "missing 'backends' section");

    // Projects: at least 1 ready workspace
    assert!(json["projects"]["total"].as_u64().unwrap() >= 1);
    assert!(json["projects"]["ready"].as_u64().unwrap() >= 1);

    // Agents: 1 completed dispatch
    assert_eq!(json["agents"]["total_dispatches"], 1);

    // Backends: should reflect the 2 we configured
    let backends = json["backends"].as_array().unwrap();
    assert_eq!(backends.len(), 2);
    assert_eq!(backends[0]["name"], "claude");
}

#[tokio::test]
async fn test_integration_dashboard_stats_empty_state() {
    let tmp = tempfile::tempdir().unwrap();
    let state = integration_state(tmp.path().to_path_buf());

    let app = test_app(state);
    let (status, json) = get_json(app, "/api/dashboard/stats").await;

    assert_eq!(status, StatusCode::OK);

    // All counts should be zero
    assert_eq!(json["projects"]["total"], 0);
    assert_eq!(json["memories"]["total_checkpoints"], 0);
    assert_eq!(json["agents"]["total_dispatches"], 0);

    // Backends still present (from state)
    assert_eq!(json["backends"].as_array().unwrap().len(), 2);
}

// ===========================================================================
// Agent Backends — verify detection response shape
// ===========================================================================

#[tokio::test]
async fn test_integration_agent_backends_response_shape() {
    let tmp = tempfile::tempdir().unwrap();
    let state = integration_state(tmp.path().to_path_buf());

    let app = test_app(state);
    let (status, json) = get_json(app, "/api/agents/backends").await;

    assert_eq!(status, StatusCode::OK);
    assert!(json["backends"].is_array(), "response should have 'backends' array");
    let backends = json["backends"].as_array().unwrap();
    assert_eq!(backends.len(), 2);

    // Verify shape of each backend entry
    for backend in backends {
        assert!(backend["name"].is_string());
        assert!(backend["available"].is_boolean());
        // version may be string or null
        assert!(backend["version"].is_string() || backend["version"].is_null());
    }

    // Verify specific backends
    assert_eq!(backends[0]["name"], "claude");
    assert_eq!(backends[0]["available"], true);
    assert_eq!(backends[0]["version"], "1.0.0");
    assert_eq!(backends[1]["name"], "test-backend");
    assert_eq!(backends[1]["available"], false);
    assert!(backends[1]["version"].is_null());
}

// ===========================================================================
// DispatchManager — context assembly logic (no real agent binary)
// ===========================================================================

#[tokio::test]
async fn test_integration_dispatch_lifecycle_via_api() {
    let tmp = tempfile::tempdir().unwrap();
    let state = integration_state(tmp.path().to_path_buf());

    // Register a workspace so dispatch can reference it
    setup_memory_workspace(&state, tmp.path()).await;

    // Start a dispatch via the manager directly (the POST endpoint needs a
    // real backend; we test the lifecycle through the GET endpoints instead)
    let dispatch_id = {
        let mut dm = state.dispatch_manager.write().await;
        let id = dm.start_dispatch("Analyze auth flow".into(), "integ-mem-ws".into(), "claude".into());
        dm.append_output(&id, "Analyzing authentication...\n");
        dm.append_output(&id, "Found 3 auth-related symbols.\n");
        dm.complete_dispatch(&id);
        id
    };

    // GET /api/agents/history — should list the dispatch
    let app = test_app(state.clone());
    let (status, json) = get_json(app, "/api/agents/history").await;

    assert_eq!(status, StatusCode::OK);
    let dispatches = json["dispatches"].as_array().unwrap();
    assert_eq!(dispatches.len(), 1);
    assert_eq!(dispatches[0]["task"], "Analyze auth flow");
    assert_eq!(dispatches[0]["status"], "completed");

    // GET /api/agents/:id — full detail
    let app = test_app(state.clone());
    let uri = format!("/api/agents/{}", dispatch_id);
    let (status, json) = get_json(app, &uri).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["id"], dispatch_id);
    assert_eq!(json["task"], "Analyze auth flow");
    assert_eq!(json["project"], "integ-mem-ws");
    assert_eq!(json["status"], "completed");
    // Output should contain both appended chunks
    let output = json["output"].as_str().unwrap();
    assert!(output.contains("Analyzing authentication"));
    assert!(output.contains("Found 3 auth-related symbols"));
}

#[tokio::test]
async fn test_integration_dispatch_not_found() {
    let tmp = tempfile::tempdir().unwrap();
    let state = integration_state(tmp.path().to_path_buf());

    let app = test_app(state);
    let (status, _json) = get_json(app, "/api/agents/nonexistent-id").await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ===========================================================================
// Health endpoint — sanity check
// ===========================================================================

#[tokio::test]
async fn test_integration_health_endpoint() {
    let tmp = tempfile::tempdir().unwrap();
    let state = integration_state(tmp.path().to_path_buf());

    let app = test_app(state);
    let (status, json) = get_json(app, "/api/health").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["status"], "ok");
}
