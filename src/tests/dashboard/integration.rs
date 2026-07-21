use std::sync::Arc;
use std::sync::RwLock;
use std::time::Instant;

use axum::body::Body;
use axum::http::Request;
use tower::ServiceExt;

use crate::dashboard::state::DashboardState;
use crate::dashboard::{DashboardConfig, create_router};
use crate::database::types::FileInfo;
use crate::extractors::{AnnotationMarker, Symbol, SymbolKind};
use crate::registry::database::DaemonDatabase;
use crate::registry::lifecycle::LifecyclePhase;
use crate::registry::session::SessionTracker;
use crate::search::SearchProjection;
use crate::tools::workspace::indexing::state::{
    IndexingOperation, IndexingRepairReason, IndexingStage,
};
use crate::workspace::registry::generate_workspace_id;

fn test_state() -> DashboardState {
    DashboardState::new(
        Arc::new(SessionTracker::new()),
        None,
        Arc::new(RwLock::new(LifecyclePhase::Ready)),
        Instant::now(),
        None, // no embedding service in tests
        50,
    )
}

fn test_state_with_db() -> (DashboardState, tempfile::TempDir) {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let daemon_db =
        Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).expect("open daemon.db"));
    daemon_db
        .upsert_workspace("ready-a", "/proj/a", "ready")
        .unwrap();
    daemon_db
        .update_workspace_stats("ready-a", 10, 1, None, None, None)
        .unwrap();

    let state = DashboardState::new(
        Arc::new(SessionTracker::new()),
        Some(daemon_db),
        Arc::new(RwLock::new(LifecyclePhase::Ready)),
        Instant::now(),
        None,
        50,
    );

    (state, temp_dir)
}

fn make_file(path: &str, content: &str) -> FileInfo {
    FileInfo {
        path: path.to_string(),
        language: "rust".to_string(),
        hash: format!("hash_{path}"),
        size: content.len() as i64,
        last_modified: 1000,
        last_indexed: 0,
        symbol_count: 1,
        line_count: content.lines().count() as i32,
        content: Some(content.to_string()),
    }
}

fn make_symbol(id: &str, name: &str, file_path: &str) -> Symbol {
    Symbol {
        id: id.to_string(),
        name: name.to_string(),
        kind: SymbolKind::Function,
        language: "rust".to_string(),
        file_path: file_path.to_string(),
        start_line: 1,
        start_column: 0,
        end_line: 1,
        end_column: 24,
        start_byte: 0,
        end_byte: 24,
        signature: Some(format!("fn {}()", name)),
        doc_comment: None,
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: Some(format!("fn {}() {{}}", name)),
        content_type: None,
        body_span: None,
        body_hash: None,
        annotations: Vec::new(),
    }
}

fn make_file_with_language(path: &str, language: &str, content: &str) -> FileInfo {
    FileInfo {
        path: path.to_string(),
        language: language.to_string(),
        hash: format!("hash_{path}"),
        size: content.len() as i64,
        last_modified: 1000,
        last_indexed: 0,
        symbol_count: 1,
        line_count: content.lines().count() as i32,
        content: Some(content.to_string()),
    }
}

fn make_marker(annotation: &str, annotation_key: &str, raw_text: &str) -> AnnotationMarker {
    AnnotationMarker {
        annotation: annotation.to_string(),
        annotation_key: annotation_key.to_string(),
        raw_text: Some(raw_text.to_string()),
        carrier: None,
    }
}

fn make_signal_symbol(
    id: &str,
    name: &str,
    file_path: &str,
    start_line: u32,
    annotations: Vec<AnnotationMarker>,
) -> Symbol {
    Symbol {
        id: id.to_string(),
        name: name.to_string(),
        kind: SymbolKind::Method,
        language: "csharp".to_string(),
        file_path: file_path.to_string(),
        start_line,
        start_column: 4,
        end_line: start_line + 2,
        end_column: 1,
        start_byte: 20,
        end_byte: 80,
        signature: Some(format!("{name}()")),
        doc_comment: None,
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: Some(1.0),
        code_context: Some(format!("{name}() {{}}")),
        content_type: None,
        body_span: None,
        body_hash: None,
        annotations,
    }
}

async fn state_with_projection_lag() -> (DashboardState, tempfile::TempDir, String) {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let workspace_root = temp_dir.path().join("workspace");
    std::fs::create_dir_all(&workspace_root).expect("workspace dir");
    let workspace_id =
        generate_workspace_id(&workspace_root.to_string_lossy()).expect("workspace id");

    let daemon_db =
        Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).expect("open daemon.db"));
    daemon_db
        .upsert_workspace(&workspace_id, &workspace_root.to_string_lossy(), "ready")
        .unwrap();
    daemon_db
        .update_workspace_stats(&workspace_id, 2, 1, None, None, None)
        .unwrap();

    let workspace = Arc::new(
        crate::workspace::JulieWorkspace::initialize(workspace_root.clone())
            .await
            .expect("workspace init"),
    );

    {
        let mut db = workspace
            .db
            .as_ref()
            .expect("workspace db")
            .lock()
            .expect("db lock");
        db.bulk_store_fresh_atomic(
            &[make_file("src/lib.rs", "fn first_symbol() {}\n")],
            &[make_symbol("sym_1", "first_symbol", "src/lib.rs")],
            &[],
            &[],
            &[],
            &workspace_id,
        )
        .unwrap();

        let search_index = workspace
            .search_index
            .as_ref()
            .expect("search index").clone();
        SearchProjection::tantivy(&workspace_id)
            .ensure_current_from_database(&mut db, &search_index)
            .unwrap();
        drop(search_index);

        db.incremental_update_atomic(
            &["src/lib.rs".to_string()],
            &[make_file("src/lib.rs", "fn second_symbol() {}\n")],
            &[make_symbol("sym_2", "second_symbol", "src/lib.rs")],
            &[],
            &[],
            &[],
            &workspace_id,
        )
        .unwrap();
    }

    (
        DashboardState::new(
            Arc::new(SessionTracker::new()),
            Some(daemon_db),
            Arc::new(RwLock::new(LifecyclePhase::Ready)),
            Instant::now(),
            None,
            50,
        ),
        temp_dir,
        workspace_id,
    )
}

async fn state_with_signal_workspace() -> (DashboardState, tempfile::TempDir, String) {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let workspace_root = temp_dir.path().join("signals-workspace");
    std::fs::create_dir_all(&workspace_root).expect("workspace dir");
    let workspace_id =
        generate_workspace_id(&workspace_root.to_string_lossy()).expect("workspace id");

    let daemon_db =
        Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).expect("open daemon.db"));
    daemon_db
        .upsert_workspace(&workspace_id, &workspace_root.to_string_lossy(), "ready")
        .unwrap();
    daemon_db
        .update_workspace_stats(&workspace_id, 2, 1, None, None, None)
        .unwrap();

    let workspace = Arc::new(
        crate::workspace::JulieWorkspace::initialize(workspace_root.clone())
            .await
            .expect("workspace init"),
    );

    {
        let mut db = workspace
            .db
            .as_ref()
            .expect("workspace db")
            .lock()
            .expect("db lock");
        db.bulk_store_fresh_atomic(
            &[make_file_with_language(
                "Controllers/HealthController.cs",
                "csharp",
                "[HttpGet]\npublic string Health() => \"ok\";\n",
            )],
            &[
                make_signal_symbol(
                    "health-route",
                    "Health",
                    "Controllers/HealthController.cs",
                    12,
                    vec![make_marker("HttpGet", "httpget", "[HttpGet(\"/health\")]")],
                ),
                make_signal_symbol(
                    "status-route",
                    "Status",
                    "Controllers/HealthController.cs",
                    20,
                    vec![
                        make_marker("HttpGet", "httpget", "[HttpGet(\"/status\")]"),
                        make_marker("AllowAnonymous", "allowanonymous", "[AllowAnonymous]"),
                    ],
                ),
            ],
            &[],
            &[],
            &[],
            &workspace_id,
        )
        .unwrap();
    }

    (
        DashboardState::new(
            Arc::new(SessionTracker::new()),
            Some(daemon_db),
            Arc::new(RwLock::new(LifecyclePhase::Ready)),
            Instant::now(),
            None,
            50,
        ),
        temp_dir,
        workspace_id,
    )
}

async fn state_with_search_workspace(
    file_path: &str,
    content: &str,
    symbol_name: &str,
) -> (DashboardState, tempfile::TempDir, String) {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let workspace_root = temp_dir.path().join("workspace");
    std::fs::create_dir_all(&workspace_root).expect("workspace dir");
    let workspace_id =
        generate_workspace_id(&workspace_root.to_string_lossy()).expect("workspace id");

    let daemon_db =
        Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).expect("open daemon.db"));
    daemon_db
        .upsert_workspace(&workspace_id, &workspace_root.to_string_lossy(), "ready")
        .unwrap();
    daemon_db
        .update_workspace_stats(&workspace_id, 1, 1, None, None, None)
        .unwrap();

    let workspace = Arc::new(
        crate::workspace::JulieWorkspace::initialize(workspace_root.clone())
            .await
            .expect("workspace init"),
    );

    {
        let mut db = workspace
            .db
            .as_ref()
            .expect("workspace db")
            .lock()
            .expect("db lock");
        db.bulk_store_fresh_atomic(
            &[make_file(file_path, content)],
            &[make_symbol("sym_1", symbol_name, file_path)],
            &[],
            &[],
            &[],
            &workspace_id,
        )
        .unwrap();

        let search_index = workspace
            .search_index
            .as_ref()
            .expect("search index").clone();
        SearchProjection::tantivy(&workspace_id)
            .ensure_current_from_database(&mut db, &search_index)
            .unwrap();
    }

    (
        DashboardState::new(
            Arc::new(SessionTracker::new()),
            Some(daemon_db),
            Arc::new(RwLock::new(LifecyclePhase::Ready)),
            Instant::now(),
            None,
            50,
        ),
        temp_dir,
        workspace_id,
    )
}

mod metrics;
mod signals;
mod static_search;
mod status;
