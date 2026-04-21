use std::fs;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use anyhow::Result;
use tempfile::TempDir;

use crate::daemon::database::DaemonDatabase;
use crate::daemon::workspace_pool::WorkspacePool;
use crate::handler::JulieServerHandler;
use crate::tools::get_context::GetContextTool;
use crate::tools::workspace::ManageWorkspaceTool;
use crate::workspace::registry::generate_workspace_id;

async fn mark_index_ready(handler: &JulieServerHandler) {
    handler
        .indexing_status
        .search_ready
        .store(true, Ordering::Relaxed);
    *handler.is_indexed.write().await = true;
}

async fn setup_rebound_primary_get_context_handler()
-> Result<(JulieServerHandler, String, std::path::PathBuf)> {
    let temp_dir = TempDir::new()?;
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir)?;

    let original_root = temp_dir.path().join("original-primary");
    let rebound_root = temp_dir.path().join("rebound-primary");
    fs::create_dir_all(original_root.join("src"))?;
    fs::create_dir_all(rebound_root.join("src"))?;
    fs::write(
        original_root.join("src").join("old.rs"),
        "fn old_root_only() {}\n",
    )?;
    fs::write(
        rebound_root.join("src").join("rebound.rs"),
        "/// rebound context phrase\npub fn rebound_primary_symbol() {}\n",
    )?;

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db"))?);
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));

    let original_path = original_root.canonicalize()?;
    let original_path_str = original_path.to_string_lossy().to_string();
    let original_id = generate_workspace_id(&original_path_str)?;
    let original_ws = pool
        .get_or_init(&original_id, original_path.clone())
        .await?;

    let handler = JulieServerHandler::new_with_shared_workspace(
        original_ws,
        original_path.clone(),
        Some(Arc::clone(&daemon_db)),
        Some(original_id.clone()),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await?;

    daemon_db.upsert_workspace(&original_id, &original_path_str, "ready")?;

    let rebound_path = rebound_root.canonicalize()?;
    let rebound_path_str = rebound_path.to_string_lossy().to_string();
    let rebound_id = generate_workspace_id(&rebound_path_str)?;
    daemon_db.upsert_workspace(&rebound_id, &rebound_path_str, "ready")?;

    let rebound_ws = pool.get_or_init(&rebound_id, rebound_path.clone()).await?;
    let seed_handler = JulieServerHandler::new_with_shared_workspace(
        rebound_ws,
        rebound_path.clone(),
        Some(Arc::clone(&daemon_db)),
        Some(rebound_id.clone()),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await?;

    ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(rebound_path_str.clone()),
        force: Some(true),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&seed_handler)
    .await?;

    handler.set_current_primary_binding(rebound_id.clone(), rebound_path.clone());
    mark_index_ready(&handler).await;

    std::mem::forget(temp_dir);

    Ok((handler, rebound_id, rebound_path))
}

#[tokio::test]
async fn test_get_context_primary_uses_rebound_current_primary_store() -> Result<()> {
    let (handler, _rebound_id, _rebound_path) = setup_rebound_primary_get_context_handler().await?;

    let result = GetContextTool {
        query: "rebound context phrase".to_string(),
        max_tokens: Some(1200),
        workspace: Some("primary".to_string()),
        language: Some("rust".to_string()),
        file_pattern: None,
        format: Some("readable".to_string()),
        edited_files: None,
        entry_symbols: None,
        stack_trace: None,
        failing_test: None,
        max_hops: None,
        prefer_tests: None,
    }
    .call_tool(&handler)
    .await?;

    let result_text = format!("{:?}", result);
    assert!(
        result_text.contains("rebound_primary_symbol") && result_text.contains("src/rebound.rs"),
        "get_context should use the rebound current-primary store instead of the stale loaded workspace: {result_text}"
    );

    Ok(())
}

#[tokio::test]
async fn test_get_context_primary_rejects_swap_gap() -> Result<()> {
    let (handler, _rebound_id, _rebound_path) = setup_rebound_primary_get_context_handler().await?;
    handler.publish_loaded_workspace_swap_intent_for_test();

    let err = GetContextTool {
        query: "rebound context phrase".to_string(),
        max_tokens: Some(1200),
        workspace: Some("primary".to_string()),
        language: Some("rust".to_string()),
        file_pattern: None,
        format: Some("readable".to_string()),
        edited_files: None,
        entry_symbols: None,
        stack_trace: None,
        failing_test: None,
        max_hops: None,
        prefer_tests: None,
    }
    .call_tool(&handler)
    .await
    .expect_err("swap gap should reject primary get_context");

    assert!(
        err.to_string()
            .contains("Primary workspace identity unavailable during swap"),
        "unexpected error: {err:#}"
    );

    Ok(())
}
