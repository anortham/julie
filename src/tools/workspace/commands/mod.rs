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
}

#[mcp_tool(
    name = "manage_workspace",
    description = "🏗️ UNIFIED WORKSPACE MANAGEMENT - Index, add, remove, and manage multiple project workspaces\n\nCommon operations:\n• Index workspace: Use 'index' command to enable fast search capabilities\n• Force reindex: Use 'index' with force=true to rebuild from scratch\n• Multi-workspace: Use 'add' to include reference workspaces for cross-project search\n• Maintenance: Use 'clean' to remove expired workspaces and optimize storage\n\nMust provide command as JSON object with command type + parameters (see examples in parameter docs)",
    title = "Manage Julie Workspaces",
    idempotent_hint = false,
    destructive_hint = false,
    open_world_hint = false,
    read_only_hint = false,
    meta = r#"{"priority": "high", "category": "workspace"}"#
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ManageWorkspaceTool {
    /// Workspace management command to execute.
    ///
    /// Examples:
    /// - Index current directory: {"command": "index", "force": false, "path": null}
    /// - Force reindex workspace: {"command": "index", "force": true, "path": null}
    /// - Index specific path: {"command": "index", "force": false, "path": "/path/to/workspace"}
    /// - Add reference workspace: {"command": "add", "path": "/path/to/other/project", "name": "Optional Display Name"}
    /// - List all workspaces: {"command": "list"}
    /// - Remove workspace: {"command": "remove", "workspace_id": "workspace-id-here"}
    /// - Clean expired workspaces: {"command": "clean", "expired_only": true}
    /// - Show statistics: {"command": "stats", "workspace_id": null}
    /// - Set TTL: {"command": "set_ttl", "days": 30}
    /// - Set storage limit: {"command": "set_limit", "max_size_mb": 1024}
    ///
    /// Note: The command field uses a tagged enum structure where the command type and parameters
    /// are combined in a single JSON object with the command type as the "command" field.
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
        }
    }
}
