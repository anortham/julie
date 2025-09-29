use anyhow::Result;
use rust_mcp_sdk::macros::{mcp_tool, JsonSchema};
use rust_mcp_sdk::schema::CallToolResult;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::handler::JulieServerHandler;

mod index;
mod limits;
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
    /// Clean up expired or orphaned workspaces
    Clean {
        /// Only clean expired workspaces, not orphaned ones
        expired_only: bool,
    },
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
    /// Set TTL for reference workspaces
    SetTtl {
        /// Number of days before reference workspaces expire
        days: u32,
    },
    /// Set storage size limit
    SetLimit {
        /// Maximum total index size in MB
        max_size_mb: u64,
    },
    /// Show comprehensive system health status
    Health {
        /// Include detailed diagnostic information
        detailed: Option<bool>,
    },
}

#[mcp_tool(
    name = "manage_workspace",
    description = "MANAGE PROJECT WORKSPACES - Index, add, remove, and configure multiple project workspaces",
    title = "Manage Julie Workspaces",
    idempotent_hint = false,
    destructive_hint = false,
    open_world_hint = false,
    read_only_hint = false,
    meta = r#"{"priority": "high", "category": "workspace"}"#
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ManageWorkspaceTool {
    /// Operation to perform: "index", "list", "add", "remove", "stats", "clean", "refresh", "health", "set_ttl", "set_limit"
    ///
    /// EXAMPLES:
    /// Index workspace:      {"operation": "index", "path": null, "force": false}
    /// List workspaces:      {"operation": "list"}
    /// Show stats:           {"operation": "stats", "workspace_id": null}
    /// Add workspace:        {"operation": "add", "path": "/path/to/project", "name": "My Project"}
    /// Clean expired:        {"operation": "clean", "expired_only": true}
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

    /// Only clean expired workspaces (used by: clean)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expired_only: Option<bool>,

    /// TTL in days (used by: set_ttl)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub days: Option<u32>,

    /// Max size in MB (used by: set_limit)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_size_mb: Option<u64>,

    /// Include detailed diagnostics (used by: health)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detailed: Option<bool>,
}

impl ManageWorkspaceTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        info!("🏗️ Managing workspace with operation: {}", self.operation);

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
            "clean" => {
                self.handle_clean_command(handler, self.expired_only.unwrap_or(false))
                    .await
            }
            "refresh" => {
                let workspace_id = self.workspace_id.as_ref()
                    .ok_or_else(|| anyhow::anyhow!("'workspace_id' parameter required for 'refresh' operation"))?;
                self.handle_refresh_command(handler, workspace_id).await
            }
            "stats" => {
                self.handle_stats_command(handler, self.workspace_id.clone())
                    .await
            }
            "set_ttl" => {
                let days = self.days
                    .ok_or_else(|| anyhow::anyhow!("'days' parameter required for 'set_ttl' operation"))?;
                self.handle_set_ttl_command(handler, days).await
            }
            "set_limit" => {
                let max_size_mb = self.max_size_mb
                    .ok_or_else(|| anyhow::anyhow!("'max_size_mb' parameter required for 'set_limit' operation"))?;
                self.handle_set_limit_command(handler, max_size_mb).await
            }
            "health" => {
                self.handle_health_command(handler, self.detailed.unwrap_or(false))
                    .await
            }
            _ => Err(anyhow::anyhow!(
                "Unknown operation: '{}'. Valid operations: index, list, add, remove, stats, clean, refresh, health, set_ttl, set_limit",
                self.operation
            )),
        }
    }
}
