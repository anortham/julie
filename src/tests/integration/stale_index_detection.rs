//! TDD Tests for Stale Index Detection
//!
//! These tests verify that Julie correctly detects when the index is stale and needs re-indexing.

use anyhow::Result;
use std::fs;
use std::sync::Arc;
use tempfile::TempDir;

use crate::tools::workspace::indexing::engine_version::{
    SEMANTIC_INDEX_ENGINE_COMPONENT, SEMANTIC_INDEX_ENGINE_VERSION,
};
use crate::tools::workspace::indexing::route::{IndexRoute, IndexRouteRepairReason};
use crate::tools::workspace::indexing::state::IndexingRepairReason;

mod freshness;
mod reconnect;
mod repair_reasons;
mod scan_workspace;
mod upgrade;

use crate::handler::JulieServerHandler;
use crate::tools::search::FastSearchTool;
use crate::tools::workspace::ManageWorkspaceTool;

async fn create_test_handler(workspace_path: &std::path::Path) -> Result<JulieServerHandler> {
    let handler = JulieServerHandler::new_for_test().await?;
    handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await?;
    Ok(handler)
}

async fn index_workspace(
    handler: &JulieServerHandler,
    workspace_path: &std::path::Path,
) -> Result<()> {
    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string_lossy().to_string()),
        force: Some(true),
        name: None,
        workspace_id: None,
        detailed: None,
    };

    index_tool.call_tool(handler).await?;
    Ok(())
}

async fn fast_search_text(handler: &JulieServerHandler, query: &str) -> Result<String> {
    let result = FastSearchTool {
        query: query.to_string(),
        limit: 10,
        ..Default::default()
    }
    .call_tool(handler)
    .await?;

    Ok(result
        .content
        .iter()
        .filter_map(|content| {
            serde_json::to_value(content).ok().and_then(|json| {
                json.get("text")
                    .and_then(|value| value.as_str())
                    .map(str::to_owned)
            })
        })
        .collect::<Vec<_>>()
        .join("\n"))
}
