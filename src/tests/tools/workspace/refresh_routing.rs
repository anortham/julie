use std::fs;
use std::sync::Arc;

use crate::daemon::database::DaemonDatabase;
use crate::daemon::workspace_pool::WorkspacePool;
use crate::handler::JulieServerHandler;
use crate::mcp_compat::CallToolResult;
use crate::tools::workspace::ManageWorkspaceTool;
use crate::workspace::registry::generate_workspace_id;

fn extract_text_from_result(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|content_block| {
            serde_json::to_value(content_block).ok().and_then(|json| {
                json.get("text")
                    .and_then(|value| value.as_str())
                    .map(|text| text.to_string())
            })
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
async fn test_primary_force_refresh_uses_full_index_path() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let primary_root = temp_dir.path().join("primary");
    fs::create_dir_all(&primary_root).unwrap();
    fs::write(primary_root.join("main.rs"), "fn primary_marker() {}\n").unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));

    let primary_path = primary_root.canonicalize().unwrap();
    let primary_path_str = primary_path.to_string_lossy().to_string();
    let primary_id = generate_workspace_id(&primary_path_str).unwrap();
    let primary_ws = pool
        .get_or_init(&primary_id, primary_path.clone())
        .await
        .expect("primary workspace should initialize");

    let handler = JulieServerHandler::new_with_shared_workspace(
        primary_ws,
        primary_path,
        Some(Arc::clone(&daemon_db)),
        Some(primary_id.clone()),
        None,
        None,
        None,
        None,
        Some(pool),
    )
    .await
    .expect("handler should initialize");

    daemon_db
        .upsert_workspace(&primary_id, &primary_path_str, "ready")
        .unwrap();

    let result = ManageWorkspaceTool {
        operation: "refresh".to_string(),
        path: None,
        force: Some(true),
        name: None,
        workspace_id: Some(primary_id),
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect("primary force refresh should succeed");

    let text = extract_text_from_result(&result);
    assert!(
        text.contains("Workspace indexing complete"),
        "primary force refresh should delegate to full index path: {text}"
    );
    assert!(
        !text.contains("Workspace Refresh:"),
        "primary force refresh should not use refresh payload: {text}"
    );
}
