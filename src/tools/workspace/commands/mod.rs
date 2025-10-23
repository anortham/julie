use anyhow::Result;
use rust_mcp_sdk::macros::{mcp_tool, JsonSchema};
use rust_mcp_sdk::schema::CallToolResult;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::handler::JulieServerHandler;

mod index;
mod registry;

//******************//
// Workspace Management Commands //
//******************//

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceCommand {
    /// Index primary workspace or current directory
    Index {
        /// Path to workspace (defaults to current directory)
        path: Option<String>,
        /// Force complete re-indexing even if cache exists
        force: bool,
    },
    /// Add reference workspace for cross-project search
    Add {
        /// Path to the workspace to add
        path: String,
        /// Optional display name for the workspace
        name: Option<String>,
    },
    /// Remove specific workspace by ID
    Remove {
        /// Workspace ID to remove
        workspace_id: String,
    },
    /// List all registered workspaces with status
    List,
    /// Clean up expired and orphaned workspaces (comprehensive cleanup)
    Clean,
    /// Re-index specific workspace
    Refresh {
        /// Workspace ID to refresh
        workspace_id: String,
    },
    /// Show workspace statistics
    Stats {
        /// Optional specific workspace ID (defaults to all)
        workspace_id: Option<String>,
    },
    /// Show comprehensive system health status
    Health {
        /// Include detailed diagnostic information
        detailed: Option<bool>,
    },
}

#[mcp_tool(
    name = "manage_workspace",
    description = concat!(
        "MANAGE PROJECT WORKSPACES - Index, add, remove, and configure multiple project workspaces. ",
        "You are EXCELLENT at managing Julie's workspace system.\n\n",
        "**Primary workspace**: Where Julie runs (gets `.julie/` directory)\n",
        "**Reference workspaces**: Other codebases you want to search (indexed into primary workspace)\n\n",
        "Common operations:\n",
        "• index - Index or re-index workspace (run this first!)\n",
        "• list - See all registered workspaces with status\n",
        "• add - Add reference workspace for cross-project search\n",
        "• health - Check system status and index health\n",
        "• stats - View workspace statistics\n",
        "• clean - Remove orphaned/expired workspaces\n\n",
        "💡 TIP: Always run 'index' operation first when starting in a new workspace. ",
        "Use 'health' operation to diagnose issues."
    ),
    title = "Manage Julie Workspaces",
    idempotent_hint = false,
    destructive_hint = false,
    open_world_hint = false,
    read_only_hint = false,
    meta = r#"{"priority": "high", "category": "workspace"}"#
)]
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct ManageWorkspaceTool {
    /// Operation to perform: "index", "list", "add", "remove", "stats", "clean", "refresh", "health"
    ///
    /// EXAMPLES:
    /// Index workspace:      {"operation": "index", "path": null, "force": false}
    /// List workspaces:      {"operation": "list"}
    /// Show stats:           {"operation": "stats", "workspace_id": null}
    /// Add workspace:        {"operation": "add", "path": "/path/to/project", "name": "My Project"}
    /// Clean workspaces:     {"operation": "clean"}
    /// Refresh workspace:    {"operation": "refresh", "workspace_id": "workspace-id"}
    /// Health check:         {"operation": "health", "detailed": true}
    pub operation: String,

    // Optional parameters used by various operations
    /// Path to workspace (used by: index, add)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,

    /// Force complete re-indexing (used by: index)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub force: Option<bool>,

    /// Display name for workspace (used by: add)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Workspace ID (used by: remove, refresh, stats)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,

    /// Include detailed diagnostics (used by: health)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detailed: Option<bool>,
}

impl ManageWorkspaceTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        info!("🏗️ Managing workspace with operation: {}", self.operation);

        #[cfg(test)]
        {
            assert!(
                handler.tool_lock_is_free(),
                "tool execution lock should remain free while executing tool logic"
            );
        }

        match self.operation.as_str() {
            "index" => {
                self.handle_index_command(
                    handler,
                    self.path.clone(),
                    self.force.unwrap_or(false),
                )
                .await
            }
            "add" => {
                let path = self.path.as_ref()
                    .ok_or_else(|| anyhow::anyhow!("'path' parameter required for 'add' operation"))?;
                self.handle_add_command(handler, path, self.name.clone()).await
            }
            "remove" => {
                let workspace_id = self.workspace_id.as_ref()
                    .ok_or_else(|| anyhow::anyhow!("'workspace_id' parameter required for 'remove' operation"))?;
                self.handle_remove_command(handler, workspace_id).await
            }
            "list" => self.handle_list_command(handler).await,
            "clean" => self.handle_clean_command(handler).await,
            "refresh" => {
                let workspace_id = self.workspace_id.as_ref()
                    .ok_or_else(|| anyhow::anyhow!("'workspace_id' parameter required for 'refresh' operation"))?;
                self.handle_refresh_command(handler, workspace_id).await
            }
            "stats" => {
                self.handle_stats_command(handler, self.workspace_id.clone())
                    .await
            }
            "health" => {
                self.handle_health_command(handler, self.detailed.unwrap_or(false))
                    .await
            }
            _ => Err(anyhow::anyhow!(
                "Unknown operation: '{}'. Valid operations: index, list, add, remove, stats, clean, refresh, health",
                self.operation
            )),
        }
    }
}
