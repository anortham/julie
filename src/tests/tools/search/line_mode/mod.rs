//! Tests for fast_search line-level output mode.

mod basic;
mod filters;
mod missing_index;
mod primary_rebind;

use crate::handler::JulieServerHandler;
use crate::tools::workspace::ManageWorkspaceTool;
use anyhow::Result;
use std::fs;
use std::sync::atomic::Ordering;
use tempfile::TempDir;

async fn mark_index_ready(handler: &JulieServerHandler) {
    handler
        .indexing_status
        .search_ready
        .store(true, Ordering::Relaxed);
    *handler.is_indexed.write().await = true;
}

async fn setup_loaded_primary_without_tantivy() -> Result<(TempDir, JulieServerHandler)> {
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }

    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_path_buf();
    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;
    fs::write(
        src_dir.join("example.rs"),
        "pub fn loaded_primary_missing_tantivy() {}
",
    )?;

    let handler = JulieServerHandler::new_for_test().await?;
    handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await?;

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;

    let workspace_id =
        crate::workspace::registry::generate_workspace_id(&workspace_path.to_string_lossy())?;
    let tantivy_dir = handler.workspace_tantivy_dir_for(&workspace_id).await?;
    let meta_path = tantivy_dir.join("meta.json");
    if meta_path.exists() {
        fs::remove_file(meta_path)?;
    }

    mark_index_ready(&handler).await;

    Ok((temp_dir, handler))
}
