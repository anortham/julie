use crate::mcp_compat::CallToolResult;
use anyhow::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::handler::JulieServerHandler;

mod index;
pub(crate) mod registry;

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
    /// Register a known workspace and build its index without activating it
    Register {
        /// Path to the workspace to register
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
    /// Open and activate a workspace for the current daemon session
    Open {
        /// Optional path to the workspace to open
        path: Option<String>,
        /// Optional workspace ID to open
        workspace_id: Option<String>,
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

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct ManageWorkspaceTool {
    /// Operation to perform: "index", "list", "register", "remove", "stats", "clean", "refresh", "open", "health"
    ///
    /// EXAMPLES:
    /// Index workspace:      {"operation": "index", "path": null, "force": false}
    /// List workspaces:      {"operation": "list"}
    /// Show stats:           {"operation": "stats", "workspace_id": null}
    /// Register workspace:   {"operation": "register", "path": "/path/to/project", "name": "My Project"}
    /// Open workspace:       {"operation": "open", "workspace_id": "workspace-id"}
    /// Open by path:         {"operation": "open", "path": "/path/to/project"}
    /// Clean workspaces:     {"operation": "clean"}
    /// Refresh workspace:    {"operation": "refresh", "workspace_id": "workspace-id", "force": true}
    /// Open and force sync:   {"operation": "open", "workspace_id": "workspace-id", "force": true}
    /// Health check:         {"operation": "health", "detailed": true}
    pub operation: String,

    // Optional parameters used by various operations
    /// Path to workspace (used by: index, register, open)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,

    /// Force complete re-indexing, bypassing incremental check (used by: index, refresh, open). Use when indexing code changed but source files are unchanged on disk
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        deserialize_with = "crate::utils::serde_lenient::deserialize_option_bool_lenient"
    )]
    pub force: Option<bool>,

    /// Display name for workspace metadata (used by: register)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Workspace ID (used by: remove, refresh, open, stats)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,

    /// Include detailed diagnostics (used by: health)
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        deserialize_with = "crate::utils::serde_lenient::deserialize_option_bool_lenient"
    )]
    pub detailed: Option<bool>,
}

impl ManageWorkspaceTool {
    /// Call with `skip_embeddings: true` to suppress the embedding pipeline
    /// (used by auto-indexing to avoid expensive sidecar startup on init).
    pub async fn call_tool_with_options(
        &self,
        handler: &JulieServerHandler,
        skip_embeddings: bool,
    ) -> Result<CallToolResult> {
        match self.operation.as_str() {
            "index" => {
                self.handle_index_command(
                    handler,
                    self.path.clone(),
                    self.force.unwrap_or(false),
                    skip_embeddings,
                )
                .await
            }
            _ => self.call_tool(handler).await,
        }
    }

    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        info!("🏗️ Managing workspace with operation: {}", self.operation);

        match self.operation.as_str() {
            "index" => {
                self.handle_index_command(
                    handler,
                    self.path.clone(),
                    self.force.unwrap_or(false),
                    false,
                )
                .await
            }
            "register" => {
                let path = self.path.as_ref().ok_or_else(|| {
                    anyhow::anyhow!("'path' parameter required for 'register' operation")
                })?;
                self.handle_register_command(handler, path, self.name.clone())
                    .await
            }
            "remove" => {
                let workspace_id = self.workspace_id.as_ref().ok_or_else(|| {
                    anyhow::anyhow!("'workspace_id' parameter required for 'remove' operation")
                })?;
                self.handle_remove_command(handler, workspace_id).await
            }
            "list" => self.handle_list_command(handler).await,
            "clean" => self.handle_clean_command(handler).await,
            "refresh" => {
                let workspace_id = self.workspace_id.as_ref().ok_or_else(|| {
                    anyhow::anyhow!("'workspace_id' parameter required for 'refresh' operation")
                })?;
                self.handle_refresh_command(handler, workspace_id).await
            }
            "open" => self.handle_open_command(handler).await,
            "stats" => {
                self.handle_stats_command(handler, self.workspace_id.clone())
                    .await
            }
            "health" => {
                self.handle_health_command(handler, self.detailed.unwrap_or(false))
                    .await
            }
            _ => Err(anyhow::anyhow!(
                "Unknown operation: '{}'. Valid operations: index, list, register, remove, stats, clean, refresh, open, health",
                self.operation
            )),
        }
    }
}
