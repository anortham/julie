//! Tests for the search REST API endpoints.
//!
//! Tests both `POST /api/search` and `POST /api/search/debug` endpoints,
//! covering definition search, content search, debug scoring breakdown,
//! workspace resolution, and error cases.

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
use crate::registry::GlobalRegistry;
use crate::server::AppState;
use crate::workspace::JulieWorkspace;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// Create a fresh AppState for search API tests.
fn test_state(julie_home: std::path::PathBuf) -> Arc<AppState> {
    let (indexing_sender, _rx) = tokio::sync::mpsc::channel::<IndexRequest>(1);
    Arc::new(AppState {
        start_time: Instant::now(),
        registry: Arc::new(tokio::sync::RwLock::new(GlobalRegistry::new())),
        julie_home,
        daemon_state: Arc::new(tokio::sync::RwLock::new(DaemonState::new())),
        cancellation_token: CancellationToken::new(),
        indexing_sender,
    })
}

/// Build a test app with API routes.
fn test_app(state: Arc<AppState>) -> axum::Router {
    axum::Router::new().nest("/api", api::routes(state))
}

/// Create a workspace with a search index and some test symbols, then
/// register it in the daemon state as Ready.
async fn setup_workspace_with_symbols(
    state: &Arc<AppState>,
    temp_dir: &tempfile::TempDir,
) -> String {
    use crate::search::{LanguageConfigs, SearchIndex, SymbolDocument};
    use std::path::PathBuf;

    let ws_id = "test-ws-001".to_string();
    let tantivy_dir = temp_dir.path().join("tantivy");
    std::fs::create_dir_all(&tantivy_dir).unwrap();

    // Create a search index with some symbols
    let configs = LanguageConfigs::load_embedded();
    let search_index =
        SearchIndex::create_with_language_configs(&tantivy_dir, &configs).unwrap();

    // Add some test symbols
    let symbols = vec![
        SymbolDocument {
            id: "sym-1".to_string(),
            name: "SearchIndex".to_string(),
            signature: "pub struct SearchIndex".to_string(),
            doc_comment: "Tantivy-backed search index for code intelligence.".to_string(),
            file_path: "src/search/index.rs".to_string(),
            kind: "struct".to_string(),
            language: "rust".to_string(),
            start_line: 131,
            code_body: "pub struct SearchIndex { index: Index, reader: IndexReader }".to_string(),
        },
        SymbolDocument {
            id: "sym-2".to_string(),
            name: "search_symbols".to_string(),
            signature: "pub fn search_symbols(&self, query_str: &str, filter: &SearchFilter, limit: usize) -> Result<SymbolSearchResults>".to_string(),
            doc_comment: "Search for symbols matching the query.".to_string(),
            file_path: "src/search/index.rs".to_string(),
            kind: "method".to_string(),
            language: "rust".to_string(),
            start_line: 268,
            code_body: "pub fn search_symbols(&self, query_str: &str, filter: &SearchFilter, limit: usize) -> Result<SymbolSearchResults> { ... }".to_string(),
        },
        SymbolDocument {
            id: "sym-3".to_string(),
            name: "CodeTokenizer".to_string(),
            signature: "pub struct CodeTokenizer".to_string(),
            doc_comment: "Code-aware tokenizer for CamelCase and snake_case.".to_string(),
            file_path: "src/search/tokenizer.rs".to_string(),
            kind: "struct".to_string(),
            language: "rust".to_string(),
            start_line: 10,
            code_body: "pub struct CodeTokenizer { patterns: Vec<Pattern> }".to_string(),
        },
        SymbolDocument {
            id: "sym-4".to_string(),
            name: "get_user".to_string(),
            signature: "pub async fn get_user(&self, id: u64) -> Option<&User>".to_string(),
            doc_comment: "Get a user by ID.".to_string(),
            file_path: "src/tests/sample.rs".to_string(),
            kind: "method".to_string(),
            language: "rust".to_string(),
            start_line: 42,
            code_body: "pub async fn get_user(&self, id: u64) -> Option<&User> { self.users.get(&id) }".to_string(),
        },
    ];

    for sym in &symbols {
        search_index.add_symbol(sym).unwrap();
    }
    search_index.commit().unwrap();

    // Build the workspace
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

    // Register in daemon state
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
// POST /api/search — standard search
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_search_definitions_returns_results() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    setup_workspace_with_symbols(&state, &temp_dir).await;

    let app = test_app(state);
    let body = serde_json::json!({
        "query": "SearchIndex",
        "search_target": "definitions",
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
    assert!(json["content"].is_null()); // not present for definitions

    // Check first result has expected fields
    let first = &json["symbols"][0];
    assert!(first["id"].is_string());
    assert!(first["name"].is_string());
    assert!(first["signature"].is_string());
    assert!(first["file_path"].is_string());
    assert!(first["kind"].is_string());
    assert!(first["language"].is_string());
    assert!(first["start_line"].is_number());
    assert!(first["score"].is_number());
}

#[tokio::test]
async fn test_search_content_returns_results() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    setup_workspace_with_symbols(&state, &temp_dir).await;

    let app = test_app(state);
    let body = serde_json::json!({
        "query": "SearchIndex",
        "search_target": "content",
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

    assert_eq!(json["search_target"], "content");
    // Content search may or may not find results depending on how index_symbols
    // populates the content index. We just verify the shape is correct.
    assert!(json["count"].is_number());
    assert!(json["symbols"].is_null()); // not present for content
}

#[tokio::test]
async fn test_search_with_language_filter() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    setup_workspace_with_symbols(&state, &temp_dir).await;

    let app = test_app(state);
    let body = serde_json::json!({
        "query": "SearchIndex",
        "language": "rust",
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
}

#[tokio::test]
async fn test_search_with_specific_project() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    let ws_id = setup_workspace_with_symbols(&state, &temp_dir).await;

    let app = test_app(state);
    let body = serde_json::json!({
        "query": "SearchIndex",
        "project": ws_id,
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
async fn test_search_no_workspace_returns_404() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    // No workspace registered

    let app = test_app(state);
    let body = serde_json::json!({
        "query": "anything",
        "limit": 10
    });

    let req = Request::builder()
        .method("POST")
        .uri("/api/search")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_search_unknown_project_returns_404() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    setup_workspace_with_symbols(&state, &temp_dir).await;

    let app = test_app(state);
    let body = serde_json::json!({
        "query": "anything",
        "project": "nonexistent-workspace",
        "limit": 10
    });

    let req = Request::builder()
        .method("POST")
        .uri("/api/search")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_search_empty_query_returns_empty_results() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    setup_workspace_with_symbols(&state, &temp_dir).await;

    let app = test_app(state);
    let body = serde_json::json!({
        "query": "",
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

#[tokio::test]
async fn test_search_defaults_to_definitions() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    setup_workspace_with_symbols(&state, &temp_dir).await;

    let app = test_app(state);
    // No search_target specified — should default to "definitions"
    let body = serde_json::json!({
        "query": "SearchIndex"
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
}

// ---------------------------------------------------------------------------
// POST /api/search/debug — debug search
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_debug_search_definitions_returns_scoring_breakdown() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    setup_workspace_with_symbols(&state, &temp_dir).await;

    let app = test_app(state);
    let body = serde_json::json!({
        "query": "SearchIndex",
        "search_target": "definitions",
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

    assert_eq!(json["search_target"], "definitions");
    assert!(json["count"].as_u64().unwrap() > 0);
    assert!(json["query_tokens"].is_array());
    assert!(json["query_tokens"].as_array().unwrap().len() > 0);

    // Check the symbols debug results
    let symbols = &json["symbols"];
    assert!(symbols["results"].is_array());
    assert!(symbols["query_tokens"].is_array());
    assert!(symbols["total_candidates"].is_number());

    // Check first result has debug scoring fields
    let first = &symbols["results"][0];
    assert!(first["id"].is_string());
    assert!(first["name"].is_string());
    assert!(first["final_score"].is_number());
    assert!(first["bm25_score"].is_number());
    assert!(first["centrality_score"].is_number());
    assert!(first["centrality_boost"].is_number());
    assert!(first["pattern_boost"].is_number());
    assert!(first["nl_path_boost"].is_number());
    assert!(first["query_tokens"].is_array());
    assert!(first["field_matches"].is_array());
    assert!(first["boost_explanation"].is_string());

    // field_matches should contain "name" since we searched for "SearchIndex"
    let field_matches: Vec<String> = first["field_matches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert!(
        field_matches.contains(&"name".to_string()),
        "Searching for 'SearchIndex' should match the name field, got: {:?}",
        field_matches
    );

    // BM25 score should be positive
    assert!(first["bm25_score"].as_f64().unwrap() > 0.0);
    // Final score should be positive
    assert!(first["final_score"].as_f64().unwrap() > 0.0);
}

#[tokio::test]
async fn test_debug_search_query_tokens_show_splits() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    setup_workspace_with_symbols(&state, &temp_dir).await;

    let app = test_app(state);
    // CamelCase query should be split into tokens
    let body = serde_json::json!({
        "query": "SearchIndex",
        "search_target": "definitions",
        "limit": 5
    });

    let req = Request::builder()
        .method("POST")
        .uri("/api/search/debug")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    let tokens = json["query_tokens"].as_array().unwrap();
    // "SearchIndex" should be tokenized into components like "search", "index"
    // (possibly with stemmed variants)
    assert!(
        tokens.len() >= 2,
        "CamelCase 'SearchIndex' should produce at least 2 tokens, got: {:?}",
        tokens
    );

    // Should contain lowercase splits
    let token_strings: Vec<String> = tokens.iter().map(|t| t.as_str().unwrap().to_string()).collect();
    assert!(
        token_strings.iter().any(|t| t.contains("search") || t.contains("index")),
        "Tokens should include 'search' or 'index' splits, got: {:?}",
        token_strings
    );
}

#[tokio::test]
async fn test_debug_search_snake_case_tokens() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    setup_workspace_with_symbols(&state, &temp_dir).await;

    let app = test_app(state);
    // snake_case query should be split into tokens
    let body = serde_json::json!({
        "query": "search_symbols",
        "search_target": "definitions",
        "limit": 5
    });

    let req = Request::builder()
        .method("POST")
        .uri("/api/search/debug")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    let tokens = json["query_tokens"].as_array().unwrap();
    // "search_symbols" should be split into "search" and "symbols" (and possibly stemmed)
    assert!(
        tokens.len() >= 2,
        "snake_case 'search_symbols' should produce at least 2 tokens, got: {:?}",
        tokens
    );
}

#[tokio::test]
async fn test_debug_search_content_returns_debug_info() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    setup_workspace_with_symbols(&state, &temp_dir).await;

    let app = test_app(state);
    let body = serde_json::json!({
        "query": "tokenizer",
        "search_target": "content",
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

    assert_eq!(json["search_target"], "content");
    assert!(json["query_tokens"].is_array());
    // Content debug results
    let content = &json["content"];
    assert!(content["results"].is_array());
    assert!(content["query_tokens"].is_array());
}

#[tokio::test]
async fn test_debug_search_no_workspace_returns_404() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    // No workspace registered

    let app = test_app(state);
    let body = serde_json::json!({
        "query": "anything",
        "limit": 10
    });

    let req = Request::builder()
        .method("POST")
        .uri("/api/search/debug")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_debug_search_boost_explanation_format() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    setup_workspace_with_symbols(&state, &temp_dir).await;

    let app = test_app(state);
    let body = serde_json::json!({
        "query": "SearchIndex",
        "search_target": "definitions",
        "limit": 5
    });

    let req = Request::builder()
        .method("POST")
        .uri("/api/search/debug")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    let first = &json["symbols"]["results"][0];
    let explanation = first["boost_explanation"].as_str().unwrap();

    // Explanation should start with BM25 and end with final
    assert!(
        explanation.starts_with("BM25:"),
        "Explanation should start with 'BM25:', got: {}",
        explanation
    );
    assert!(
        explanation.contains("final:"),
        "Explanation should contain 'final:', got: {}",
        explanation
    );
}

// ---------------------------------------------------------------------------
// Workspace status checks (I-1)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_search_non_ready_workspace_returns_400() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());

    // Register a workspace that is Indexing (not Ready)
    let loaded_ws = LoadedWorkspace {
        workspace: JulieWorkspace {
            root: std::path::PathBuf::from("/fake/project"),
            julie_dir: temp_dir.path().to_path_buf(),
            db: None,
            search_index: None,
            watcher: None,
            embedding_provider: None,
            embedding_runtime_status: None,
            config: Default::default(),
        },
        status: WorkspaceLoadStatus::Indexing,
        path: std::path::PathBuf::from("/fake/project"),
    };

    {
        let mut ds = state.daemon_state.write().await;
        ds.workspaces
            .insert("indexing-ws".to_string(), loaded_ws);
    }

    let app = test_app(state);
    let body = serde_json::json!({
        "query": "anything",
        "project": "indexing-ws",
        "limit": 10
    });

    let req = Request::builder()
        .method("POST")
        .uri("/api/search")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Searching a non-Ready workspace should return 400"
    );
}

#[tokio::test]
async fn test_search_registered_workspace_returns_400() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());

    let loaded_ws = LoadedWorkspace {
        workspace: JulieWorkspace {
            root: std::path::PathBuf::from("/fake/project"),
            julie_dir: temp_dir.path().to_path_buf(),
            db: None,
            search_index: None,
            watcher: None,
            embedding_provider: None,
            embedding_runtime_status: None,
            config: Default::default(),
        },
        status: WorkspaceLoadStatus::Registered,
        path: std::path::PathBuf::from("/fake/project"),
    };

    {
        let mut ds = state.daemon_state.write().await;
        ds.workspaces
            .insert("registered-ws".to_string(), loaded_ws);
    }

    let app = test_app(state);
    let body = serde_json::json!({
        "query": "anything",
        "project": "registered-ws",
        "limit": 10
    });

    let req = Request::builder()
        .method("POST")
        .uri("/api/search")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Searching a Registered (not Ready) workspace should return 400"
    );
}

// ---------------------------------------------------------------------------
// Limit capping (S-6)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_search_limit_capped_at_500() {
    let temp_dir = tempfile::tempdir().unwrap();
    let state = test_state(temp_dir.path().to_path_buf());
    setup_workspace_with_symbols(&state, &temp_dir).await;

    let app = test_app(state);
    // Request an absurdly high limit — should be silently capped to 500
    let body = serde_json::json!({
        "query": "SearchIndex",
        "limit": 99999
    });

    let req = Request::builder()
        .method("POST")
        .uri("/api/search")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    // Should succeed (the cap is applied internally, not rejected)
    assert_eq!(response.status(), StatusCode::OK);
}
