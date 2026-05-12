use std::fs;
use std::sync::Arc;

use crate::daemon::database::DaemonDatabase;
use crate::daemon::workspace_pool::WorkspacePool;
use crate::handler::JulieServerHandler;
use crate::mcp_compat::CallToolResult;
use crate::tools::workspace::ManageWorkspaceTool;
use crate::workspace::registry::generate_workspace_id;
use crate::workspace::startup_hint::{WorkspaceStartupHint, WorkspaceStartupSource};

fn result_text(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|item| {
            serde_json::to_value(item).ok().and_then(|value| {
                value
                    .get("text")
                    .and_then(|text| text.as_str().map(str::to_owned))
            })
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
async fn test_manage_workspace_open_path_succeeds_without_bound_primary_in_deferred_session() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let startup_root = temp_dir.path().join("startup-cwd");
    let target_root = temp_dir.path().join("target");
    fs::create_dir_all(startup_root.join("src")).unwrap();
    fs::create_dir_all(target_root.join("src")).unwrap();
    fs::write(
        startup_root.join("src/lib.rs"),
        "pub fn startup_marker() {}\n",
    )
    .unwrap();
    fs::write(
        target_root.join("src/lib.rs"),
        "pub fn target_marker() {}\n",
    )
    .unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
    ));

    let startup_path = startup_root.canonicalize().unwrap();
    let handler = JulieServerHandler::new_deferred_daemon_startup_hint_without_project_log(
        WorkspaceStartupHint {
            path: startup_path,
            source: Some(WorkspaceStartupSource::Cwd),
        },
        Some(Arc::clone(&daemon_db)),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await
    .expect("deferred daemon handler should initialize");
    assert_eq!(handler.current_workspace_id(), None);
    assert_eq!(handler.loaded_workspace_id(), None);

    let target_path = target_root.canonicalize().unwrap();
    let target_path_str = target_path.to_string_lossy().to_string();
    let target_id = generate_workspace_id(&target_path_str).unwrap();
    daemon_db
        .upsert_workspace(&target_id, &target_path_str, "ready")
        .unwrap();

    let result = ManageWorkspaceTool {
        operation: "open".to_string(),
        path: Some(target_path_str.clone()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect("open(path) should not require an already-bound primary workspace");

    let text = result_text(&result);
    assert!(
        text.contains("Workspace Opened") && text.contains(&format!("Workspace ID: {target_id}")),
        "open(path) should return the requested workspace: {text}"
    );
    assert!(
        !text.contains("Workspace Refresh Failed"),
        "open(path) should not fail refresh before binding the target: {text}"
    );
    assert!(
        handler.is_workspace_active(&target_id).await,
        "open(path) should activate the target workspace"
    );
}
