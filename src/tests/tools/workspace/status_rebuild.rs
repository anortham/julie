use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::handler::JulieServerHandler;
use crate::registry::database::DaemonDatabase;
use crate::tests::helpers::mcp::call_tool_result_text;
use crate::tests::helpers::workspace::make_isolated_workspace_root;
use crate::tools::workspace::ManageWorkspaceTool;
use crate::tools::workspace::commands::ManageWorkspaceRequest;
use crate::tools::workspace::commands::registry::CheckoutStatus;
use crate::workspace::registry::generate_workspace_id;

fn tool(operation: &str, path: Option<&Path>) -> ManageWorkspaceTool {
    ManageWorkspaceTool {
        operation: operation.to_string(),
        path: path.map(|p| p.to_string_lossy().to_string()),
        force: None,
        name: None,
        workspace_id: None,
        detailed: None,
    }
}

async fn handler_with_registry(
    temp: &Path,
) -> (JulieServerHandler, Arc<DaemonDatabase>, String, PathBuf) {
    let primary_root = make_isolated_workspace_root(temp, "primary");
    fs::write(primary_root.join("main.rs"), "fn primary() {}\n").unwrap();
    let daemon_db = Arc::new(DaemonDatabase::open(&temp.join("daemon.db")).unwrap());
    let primary_path = primary_root.canonicalize().unwrap();
    let primary_id = generate_workspace_id(&primary_path.to_string_lossy()).unwrap();
    let primary_ws = Arc::new(
        crate::workspace::JulieWorkspace::initialize(primary_path.clone())
            .await
            .unwrap(),
    );
    let handler = JulieServerHandler::new_with_shared_workspace(
        primary_ws,
        primary_path.clone(),
        Some(Arc::clone(&daemon_db)),
        Some(primary_id.clone()),
        None,
    )
    .await
    .unwrap();
    (handler, daemon_db, primary_id, primary_path)
}

fn indexed_target(temp: &Path) -> PathBuf {
    let target_root = make_isolated_workspace_root(temp, "target");
    fs::write(
        target_root.join("lib.rs"),
        "pub fn status_alpha() {}\npub fn status_beta() {}\n",
    )
    .unwrap();
    target_root.canonicalize().unwrap()
}

async fn checkouts(handler: &JulieServerHandler, path: Option<&Path>) -> Vec<CheckoutStatus> {
    let result = tool("status", path).call_tool(handler).await.unwrap();
    assert!(
        !result.is_error.unwrap_or(false),
        "{}",
        call_tool_result_text(&result)
    );
    let structured = result
        .structured_content
        .clone()
        .expect("status must return structured content");
    serde_json::from_value(structured["checkouts"].clone()).unwrap()
}

#[tokio::test]
async fn status_reports_every_field_for_an_indexed_checkout() {
    let temp = tempfile::tempdir().unwrap();
    let (handler, _db, _primary_id, _primary_path) = handler_with_registry(temp.path()).await;
    let target = indexed_target(temp.path());
    tool("index", Some(&target))
        .call_tool(&handler)
        .await
        .unwrap();

    let target_id = generate_workspace_id(&target.to_string_lossy()).unwrap();
    let list = checkouts(&handler, Some(&target)).await;
    assert_eq!(list.len(), 1);
    let status = &list[0];
    assert_eq!(status.workspace_id, target_id);
    assert_eq!(status.root, target.to_string_lossy());
    assert!(status.root_exists);
    assert_eq!(
        status.watcher, "running",
        "index loads the checkout and starts its watcher"
    );
    assert_eq!(status.last_file_event_at, None);
    assert_eq!(status.file_count, 1);
    assert_eq!(status.symbol_count, 2);
    assert_eq!(status.blob_count, 1);
    assert!(status.facts_bytes > 0);
    assert_eq!(status.graph_symbols, 2);
    assert_eq!(status.tantivy, "present");
    assert!(status.tantivy_age_seconds.unwrap() < 60);
    assert_eq!(status.vector_count, 0);
    assert!(status.last_write_at.as_deref().unwrap().contains('T'));

    let text = call_tool_result_text(
        &tool("status", Some(&target))
            .call_tool(&handler)
            .await
            .unwrap(),
    );
    assert!(text.contains(&target_id), "{text}");
    assert!(text.contains("symbols: 2"), "{text}");
}

#[tokio::test]
async fn status_without_a_target_lists_every_known_checkout() {
    let temp = tempfile::tempdir().unwrap();
    let (handler, daemon_db, primary_id, _primary_path) = handler_with_registry(temp.path()).await;
    let target = indexed_target(temp.path());
    tool("index", Some(&target))
        .call_tool(&handler)
        .await
        .unwrap();
    let target_id = generate_workspace_id(&target.to_string_lossy()).unwrap();
    let missing = temp.path().join("gone");
    daemon_db
        .upsert_workspace("gone_00000000", &missing.to_string_lossy(), "ready")
        .unwrap();

    let list = checkouts(&handler, None).await;
    let ids: Vec<&str> = list.iter().map(|c| c.workspace_id.as_str()).collect();
    assert!(ids.contains(&primary_id.as_str()), "{ids:?}");
    assert!(ids.contains(&target_id.as_str()), "{ids:?}");
    let primary = list.iter().find(|c| c.workspace_id == primary_id).unwrap();
    assert_eq!(
        primary.watcher, "stopped",
        "only the loaded checkout has a watcher"
    );

    let gone = list
        .iter()
        .find(|c| c.workspace_id == "gone_00000000")
        .unwrap();
    assert!(!gone.root_exists);
    assert_eq!(gone.tantivy, "absent");
    assert_eq!(gone.tantivy_age_seconds, None);
    assert_eq!(gone.facts_bytes, 0);
    assert_eq!(gone.blob_count, 0);
    assert_eq!(gone.file_count, 0);
    assert_eq!(gone.symbol_count, 0);
    assert_eq!(gone.vector_count, 0);
    assert_eq!(gone.last_write_at, None);
}

#[tokio::test]
async fn rebuild_recreates_the_index_dir_with_the_same_symbol_count() {
    let temp = tempfile::tempdir().unwrap();
    let (handler, _db, _primary_id, _primary_path) = handler_with_registry(temp.path()).await;
    let target = indexed_target(temp.path());
    tool("index", Some(&target))
        .call_tool(&handler)
        .await
        .unwrap();
    let target_id = generate_workspace_id(&target.to_string_lossy()).unwrap();
    let index_dir = handler.workspace_index_dir_for(&target_id).await.unwrap();
    let stale_marker = index_dir.join("stale.marker");
    fs::write(&stale_marker, "old").unwrap();
    let before = checkouts(&handler, Some(&target)).await[0].symbol_count;
    assert!(before > 0);

    let result = tool("rebuild", Some(&target))
        .call_tool(&handler)
        .await
        .unwrap();
    let text = call_tool_result_text(&result);
    assert!(
        text.starts_with(&format!("Rebuilt {}\n", target.display())),
        "{text}"
    );
    assert!(text.contains("Workspace indexing complete"), "{text}");
    assert!(
        !stale_marker.exists(),
        "rebuild must delete the old index dir"
    );
    assert!(index_dir.join("store/facts.sqlite").exists());

    let after = checkouts(&handler, Some(&target)).await;
    assert_eq!(after[0].symbol_count, before);
    assert_eq!(after[0].tantivy, "present");
}

#[test]
fn rebuild_requires_a_workspace_id_or_path() {
    let err = ManageWorkspaceRequest::try_from(&tool("rebuild", None)).unwrap_err();
    assert_eq!(
        err.to_string(),
        "'workspace_id' or 'path' parameter required for 'rebuild' operation"
    );
}
