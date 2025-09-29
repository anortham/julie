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
#[serde(tag = "command", rename_all = "snake_case")]
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
    /// Workspace management command to execute
    ///
    /// COMMON COMMANDS:
    /// • Index: Enable fast search by indexing current directory (use force=true to rebuild)
    /// • Add: Include additional workspace for cross-project search
    /// • List: Show all registered workspaces with status
    /// • Clean: Remove expired workspaces to optimize storage
    /// • Stats: Display workspace statistics and health info
    ///
    /// ADVANCED COMMANDS:
    /// • Remove: Delete specific workspace by ID
    /// • SetTtl: Configure workspace expiration (days)
    /// • SetLimit: Set storage limits (MB)
    /// • Health: System health check with diagnostics
    ///
    /// Most common: Index (to enable search) and List (to see workspaces)
    pub command: WorkspaceCommand,
}

impl ManageWorkspaceTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        info!("🏗️ Managing workspace with command: {:?}", self.command);

        match &self.command {
            WorkspaceCommand::Index { path, force } => {
                self.handle_index_command(handler, path.clone(), *force)
                    .await
            }
            WorkspaceCommand::Add { path, name } => {
                self.handle_add_command(handler, path, name.clone()).await
            }
            WorkspaceCommand::Remove { workspace_id } => {
                self.handle_remove_command(handler, workspace_id).await
            }
            WorkspaceCommand::List => self.handle_list_command(handler).await,
            WorkspaceCommand::Clean { expired_only } => {
                self.handle_clean_command(handler, *expired_only).await
            }
            WorkspaceCommand::Refresh { workspace_id } => {
                self.handle_refresh_command(handler, workspace_id).await
            }
            WorkspaceCommand::Stats { workspace_id } => {
                self.handle_stats_command(handler, workspace_id.clone())
                    .await
            }
            WorkspaceCommand::SetTtl { days } => self.handle_set_ttl_command(handler, *days).await,
            WorkspaceCommand::SetLimit { max_size_mb } => {
                self.handle_set_limit_command(handler, *max_size_mb).await
            }
            WorkspaceCommand::Health { detailed } => {
                self.handle_health_command(handler, detailed.unwrap_or(false))
                    .await
            }
        }
    }
}
