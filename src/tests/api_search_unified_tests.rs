//! Tests for unified cross-content search API (content_type + hybrid).
//!
//! Tests `POST /api/search` with content_type="memory"|"all" and hybrid mode,
//! plus `POST /api/search/debug` hybrid_mode field. Verifies backward
//! compatibility, graceful degradation, and memory result shape.

use std::sync::Arc;
use std::time::Instant;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use tower::ServiceExt; // for `oneshot`

use crate::api;
use crate::daemon_indexer::IndexRequest;
use crate::daemon_state::{DaemonState, LoadedWorkspace, WorkspaceLoadStatus};
use crate::memory::index::MemoryIndex;
use crate::memory::Checkpoint;
use crate::registry::GlobalRegistry;
use crate::server::AppState;
use crate::workspace::JulieWorkspace;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

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

/// Create a workspace with both a code search index AND a memory index.
async fn setup_workspace_with_code_and_memories(
    state: &Arc<AppState>,
    temp_dir: &tempfile::TempDir,
) -> String {
    use crate::search::{LanguageConfigs, SearchIndex, SymbolDocument};
    use std::path::PathBuf;

    let ws_id = "unified-ws-001".to_string();
    let tantivy_dir = temp_dir.path().join("tantivy");
    std::fs::create_dir_all(&tantivy_dir).unwrap();

    let configs = LanguageConfigs::load_embedded();
    let search_index =
        SearchIndex::create_with_language_configs(&tantivy_dir, &configs).unwrap();

    let symbols = vec![
        SymbolDocument {
            id: "sym-1".to_string(),
            name: "SearchIndex".to_string(),
            signature: "pub struct SearchIndex".to_string(),
            doc_comment: "Tantivy-backed search index.".to_string(),
            file_path: "src/search/index.rs".to_string(),
            kind: "struct".to_string(),
            language: "rust".to_string(),
            start_line: 131,
            code_body: "pub struct SearchIndex { index: Index }".to_string(),
        },
        SymbolDocument {
            id: "sym-2".to_string(),
            name: "get_user".to_string(),
            signature: "pub async fn get_user(&self, id: u64) -> Option<&User>".to_string(),
            doc_comment: "Get a user by ID.".to_string(),
            file_path: "src/user.rs".to_string(),
            kind: "method".to_string(),
            language: "rust".to_string(),
            start_line: 42,
            code_body: "pub async fn get_user(&self, id: u64) -> Option<&User> { }"
                .to_string(),
        },
    ];

    for sym in &symbols {
        search_index.add_symbol(sym).unwrap();
    }
    search_index.commit().unwrap();

    // Create a memory index with test memories
    let memory_dir = temp_dir.path().join("indexes/memories/tantivy");
    std::fs::create_dir_all(&memory_dir).unwrap();
    let memory_index = MemoryIndex::create(&memory_dir).unwrap();

    let chk1 = Checkpoint {
        id: "chk-001".to_string(),
        timestamp: "2026-03-01T10:00:00Z".to_string(),
        description: "Decided to use Tantivy for search instead of FTS5".to_string(),
        checkpoint_type: None,
        context: None,
        decision: Some(
            "Replaced FTS5 with Tantivy for better search quality".to_string(),
        ),
        alternatives: None,
        impact: Some("high".to_string()),
        evidence: None,
        symbols: Some(vec![
            "SearchIndex".to_string(),
            "CodeTokenizer".to_string(),
        ]),
        next: None,
        confidence: None,
        unknowns: None,
        tags: Some(vec![
            "search".to_string(),
            "tantivy".to_string(),
            "architecture".to_string(),
        ]),
        git: None,
        summary: None,
        plan_id: None,
    };
    memory_index
        .add_checkpoint(&chk1, Some(".memories/2026-03-01/checkpoint_abc123.md"))
        .unwrap();

    let chk2 = Checkpoint {
        id: "chk-002".to_string(),
        timestamp: "2026-03-02T14:00:00Z".to_string(),
        description: "Added user authentication with JWT tokens".to_string(),
        checkpoint_type: None,
        context: None,
        decision: Some("Implement JWT-based authentication".to_string()),
        alternatives: None,
        impact: Some("medium".to_string()),
        evidence: None,
        symbols: Some(vec![
            "get_user".to_string(),
            "authenticate".to_string(),
        ]),
        next: None,
        confidence: None,
        unknowns: None,
        tags: Some(vec![
            "auth".to_string(),
            "jwt".to_string(),
            "security".to_string(),
        ]),
        git: None,
        summary: None,
        plan_id: None,
    };
    memory_index
        .add_checkpoint(&chk2, Some(".memories/2026-03-02/checkpoint_def456.md"))
        .unwrap();

    memory_index.commit().unwrap();

    let workspace = JulieWorkspace {
        root: PathBuf::from("/fake/project"),
        julie_dir: temp_dir.path().to_path_buf(),
        db: None,
        search_index: Some(Arc::new(std::sync::Mutex::new(search_index))),
        watcher: None,
        embedding_provider: None,
        embedding_runtime_status: None,
        config: Default::default(),
    };

    let loaded_ws = LoadedWorkspace {
        workspace,
        status: WorkspaceLoadStatus::Ready,
        path: PathBuf::from("/fake/project"),
    };

    let mut ds = state.daemon_state.write().await;
    ds.workspaces.insert(ws_id.clone(), loaded_ws);

    ws_id
}

/// Create a workspace with code index only (no memory index).
async fn setup_workspace_code_only(
    state: &Arc<AppState>,
    temp_dir: &tempfile::TempDir,
) -> String {
    use crate::search::{LanguageConfigs, SearchIndex, SymbolDocument};
    use std::path::PathBuf;

    let ws_id = "code-only-ws".to_string();
    let tantivy_dir = temp_dir.path().join("tantivy");
    std::fs::create_dir_all(&tantivy_dir).unwrap();

    let configs = LanguageConfigs::load_embedded();
    let search_index =
        SearchIndex::create_with_language_configs(&tantivy_dir, &configs).unwrap();

    search_index
        .add_symbol(&SymbolDocument {
            id: "sym-1".to_string(),
            name: "Widget".to_string(),
            signature: "pub struct Widget".to_string(),
            doc_comment: "A widget.".to_string(),
            file_path: "src/widget.rs".to_string(),
            kind: "struct".to_string(),
            language: "rust".to_string(),
            start_line: 1,
            code_body: "pub struct Widget {}".to_string(),
        })
        .unwrap();
    search_index.commit().unwrap();

    let workspace = JulieWorkspace {
        root: PathBuf::from("/fake/project"),
        julie_dir: temp_dir.path().to_path_buf(),
        db: None,
        search_index: Some(Arc::new(std::sync::Mutex::new(search_index))),
        watcher: None,
        embedding_provider: None,
        embedding_runtime_status: None,
        config: Default::default(),
    };

    let loaded_ws = LoadedWorkspace {
        workspace,
        status: WorkspaceLoadStatus::Ready,
        path: PathBuf::from("/fake/project"),
    };

    let mut ds = state.daemon_state.write().await;
    ds.workspaces.insert(ws_id.clone(), loaded_ws);

    ws_id
}

// ---------------------------------------------------------------------------
// Tests: content_type filtering
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_search_default_content_type_returns_code_only() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    setup_workspace_with_code_and_memories(&state, &temp_dir).await;

    let app = test_app(state);
    let body = serde_json::json!({
        "query": "SearchIndex",
        "limit": 10
    });

    let req = Request::builder()
        .method("POST")
        .uri("/api/search")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["search_target"], "definitions");
    assert!(json["count"].as_u64().unwrap() > 0);
    assert!(json["symbols"].is_array());
    assert!(json["memories"].is_null());
}

#[tokio::test]
async fn test_search_content_type_code_returns_code_only() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    setup_workspace_with_code_and_memories(&state, &temp_dir).await;

    let app = test_app(state);
    let body = serde_json::json!({
        "query": "SearchIndex",
        "content_type": "code",
        "limit": 10
    });

    let req = Request::builder()
        .method("POST")
        .uri("/api/search")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert!(json["count"].as_u64().unwrap() > 0);
    assert!(json["symbols"].is_array());
    assert!(json["memories"].is_null());
}

#[tokio::test]
async fn test_search_content_type_memory_returns_only_memories() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    setup_workspace_with_code_and_memories(&state, &temp_dir).await;

    let app = test_app(state);
    let body = serde_json::json!({
        "query": "tantivy search",
        "content_type": "memory",
        "limit": 10
    });

    let req = Request::builder()
        .method("POST")
        .uri("/api/search")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert!(json["count"].as_u64().unwrap() > 0);
    assert!(json["memories"].is_array());
    assert!(json["symbols"].is_null());

    let memories = json["memories"].as_array().unwrap();
    let first = &memories[0];
    assert_eq!(first["content_type"], "memory");
    assert!(first["id"].is_string());
    assert!(first["body"].is_string());
    assert!(first["score"].is_number());
}

#[tokio::test]
async fn test_search_content_type_all_returns_mixed_results() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    setup_workspace_with_code_and_memories(&state, &temp_dir).await;

    let app = test_app(state);
    let body = serde_json::json!({
        "query": "search",
        "content_type": "all",
        "limit": 20
    });

    let req = Request::builder()
        .method("POST")
        .uri("/api/search")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert!(json["count"].as_u64().unwrap() > 0);
    assert!(json["symbols"].is_array());
    assert!(json["memories"].is_array());

    if let Some(symbols) = json["symbols"].as_array() {
        if !symbols.is_empty() {
            assert_eq!(symbols[0]["content_type"], "code");
        }
    }
    if let Some(memories) = json["memories"].as_array() {
        if !memories.is_empty() {
            assert_eq!(memories[0]["content_type"], "memory");
        }
    }
}

// ---------------------------------------------------------------------------
// Tests: hybrid mode
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_search_hybrid_flag_present_in_request() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    setup_workspace_with_code_and_memories(&state, &temp_dir).await;

    let app = test_app(state);
    let body = serde_json::json!({
        "query": "search",
        "content_type": "all",
        "hybrid": true,
        "limit": 10
    });

    let req = Request::builder()
        .method("POST")
        .uri("/api/search")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_debug_search_includes_hybrid_mode() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    setup_workspace_with_code_and_memories(&state, &temp_dir).await;

    let app = test_app(state);
    let body = serde_json::json!({
        "query": "search",
        "content_type": "all",
        "hybrid": true,
        "limit": 10
    });

    let req = Request::builder()
        .method("POST")
        .uri("/api/search/debug")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert!(
        json["hybrid_mode"].is_boolean(),
        "Debug response should include hybrid_mode field, got: {:?}",
        json
    );
    assert_eq!(json["hybrid_mode"], true);
}

#[tokio::test]
async fn test_debug_search_hybrid_mode_false_by_default() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    setup_workspace_with_code_and_memories(&state, &temp_dir).await;

    let app = test_app(state);
    let body = serde_json::json!({
        "query": "SearchIndex",
        "limit": 10
    });

    let req = Request::builder()
        .method("POST")
        .uri("/api/search/debug")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["hybrid_mode"], false);
}

// ---------------------------------------------------------------------------
// Tests: graceful degradation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_search_memory_no_memory_index_graceful() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    setup_workspace_code_only(&state, &temp_dir).await;

    let app = test_app(state);
    let body = serde_json::json!({
        "query": "anything",
        "content_type": "memory",
        "limit": 10
    });

    let req = Request::builder()
        .method("POST")
        .uri("/api/search")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["count"], 0);
}
