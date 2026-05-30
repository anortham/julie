use std::fs;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use rmcp::{
    ServerHandler,
    model::{CallToolRequestParams, ErrorCode, NumberOrString},
    service::{RequestContext, serve_directly},
};
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::daemon::database::DaemonDatabase;
use crate::daemon::workspace_pool::WorkspacePool;
use crate::handler::JulieServerHandler;
use crate::paths::DaemonPaths;
use crate::tests::helpers::mcp::{
    answer_next_list_roots_request, call_tool_result_text as extract_text_from_result,
};
use crate::tools::FastSearchTool;
use crate::tools::navigation::resolution::{
    WorkspaceResolutionFailureKind, resolve_workspace_filter, workspace_resolution_failure_kind,
};
use crate::tools::workspace::ManageWorkspaceTool;
use crate::workspace::registry::generate_workspace_id;

use crate::tests::helpers::workspace::make_isolated_workspace_root;

fn assert_workspace_resolution_failure(
    error: &anyhow::Error,
    expected_kind: WorkspaceResolutionFailureKind,
    expected_message: &str,
) {
    assert_eq!(
        workspace_resolution_failure_kind(error),
        Some(expected_kind),
        "resolver-created workspace failures should expose typed metadata"
    );
    assert_eq!(
        error.to_string(),
        expected_message,
        "typed metadata must not change displayed error text"
    );
}

async fn mark_index_ready(handler: &JulieServerHandler) {
    handler
        .indexing_status
        .search_ready
        .store(true, Ordering::Relaxed);
    *handler.is_indexed.write().await = true;
}

async fn setup_known_reference_search_workspace() -> (tempfile::TempDir, JulieServerHandler, String)
{
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let primary_root = make_isolated_workspace_root(temp_dir.path(), "primary");
    let target_root = make_isolated_workspace_root(temp_dir.path(), "target");
    fs::write(primary_root.join("main.rs"), "fn primary_marker() {}\n").unwrap();
    fs::write(
        target_root.join("lib.rs"),
        "pub fn target_search_marker() {}\nconst TARGET_ONLY_MARKER: &str = \"target_search_marker\";\n",
    )
    .unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
    ));

    let primary_path = primary_root.canonicalize().unwrap();
    let primary_path_str = primary_path.to_string_lossy().to_string();
    let primary_id = generate_workspace_id(&primary_path_str).unwrap();
    let primary_ws = pool
        .get_or_init(&primary_id, primary_path.clone())
        .await
        .expect("primary workspace should initialize");

    let seed_handler = JulieServerHandler::new_with_shared_workspace(
        Arc::clone(&primary_ws),
        primary_path.clone(),
        Some(Arc::clone(&daemon_db)),
        Some(primary_id.clone()),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await
    .expect("seed handler should initialize");

    ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(primary_path_str),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&seed_handler)
    .await
    .expect("primary workspace should index");
    mark_index_ready(&seed_handler).await;

    let target_path = target_root.canonicalize().unwrap();
    let target_path_str = target_path.to_string_lossy().to_string();
    let target_id = generate_workspace_id(&target_path_str).unwrap();
    daemon_db
        .upsert_workspace(&target_id, &target_path_str, "ready")
        .expect("target workspace should be registered in daemon db");
    ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(target_path_str),
        force: Some(true),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&seed_handler)
    .await
    .expect("target workspace should index");
    mark_index_ready(&seed_handler).await;

    let handler = JulieServerHandler::new_with_shared_workspace(
        primary_ws,
        primary_path,
        Some(Arc::clone(&daemon_db)),
        Some(primary_id),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await
    .expect("fresh handler should initialize");

    (temp_dir, handler, target_id)
}

mod deferred_explicit_targets;
mod deferred_sessions;
mod global_remove;
mod list_stats;
mod open_lifecycle;
mod primary_swap_guards;
mod rebind_index;
mod resolution_failures;
mod target_activation;
