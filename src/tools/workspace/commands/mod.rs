use crate::mcp_compat::CallToolResult;
use anyhow::{Result, anyhow};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::handler::JulieServerHandler;

mod dashboard;
pub(crate) mod force_safeguards;
mod index;
pub(crate) mod index_resolution;
pub(crate) mod registry;

pub(crate) mod recover_edit;

//******************//
// Workspace Management Commands //
//******************//

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ManageWorkspaceOperation {
    Index,
    List,
    Open,
    Remove,
    Refresh,
    Health,
    Rebuild,
    Status,
    RecoverEdit,
    Dashboard,
}

impl ManageWorkspaceOperation {
    /// Single source of truth for operation names. Ordered the way the help
    /// string is presented to users — `valid_operations_help()` and `parse()`
    /// both derive from this table so they cannot drift apart.
    const OPERATIONS: &'static [(&'static str, Self)] = &[
        ("index", Self::Index),
        ("list", Self::List),
        ("open", Self::Open),
        ("remove", Self::Remove),
        ("refresh", Self::Refresh),
        ("health", Self::Health),
        ("rebuild", Self::Rebuild),
        ("status", Self::Status),
        ("recover_edit", Self::RecoverEdit),
        ("recover-edit", Self::RecoverEdit),
        ("dashboard", Self::Dashboard),
    ];

    pub(crate) fn parse(operation: &str) -> Result<Self> {
        Self::OPERATIONS
            .iter()
            .find(|(name, _)| *name == operation)
            .map(|(_, op)| *op)
            .ok_or_else(|| Self::unknown_operation_error(operation))
    }

    fn valid_operations_help() -> String {
        Self::OPERATIONS
            .iter()
            .map(|(name, _)| *name)
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn from_arguments(
        arguments: Option<&serde_json::Map<String, serde_json::Value>>,
    ) -> Option<Self> {
        let operation = arguments?
            .get("operation")
            .and_then(serde_json::Value::as_str)?;
        Self::parse(operation).ok()
    }

    pub(crate) fn request_targets_primary(
        arguments: Option<&serde_json::Map<String, serde_json::Value>>,
    ) -> bool {
        let Some(arguments) = arguments else {
            return false;
        };

        let unset = |key: &str| arguments.get(key).is_none_or(serde_json::Value::is_null);
        match Self::from_arguments(Some(arguments)) {
            Some(Self::List | Self::Remove | Self::Health) => true,
            Some(Self::Status) => unset("workspace_id") && unset("path"),
            Some(Self::Index) => unset("path"),
            _ => false,
        }
    }

    fn unknown_operation_error(operation: &str) -> anyhow::Error {
        anyhow!(
            "Unknown operation: '{}'. Valid operations: {}",
            operation,
            Self::valid_operations_help()
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ManageWorkspaceRequest {
    Index {
        path: Option<String>,
        force: bool,
    },
    Remove {
        workspace_id: String,
    },
    List,
    Refresh {
        workspace_id: String,
        force: bool,
    },
    Open {
        path: Option<String>,
        workspace_id: Option<String>,
        force: bool,
    },
    Rebuild {
        path: Option<String>,
        workspace_id: Option<String>,
    },
    Status {
        workspace_id: Option<String>,
        path: Option<String>,
    },
    Health {
        detailed: bool,
    },
    Dashboard,
    RecoverEdit {
        edit_id: String,
        recovery_action: String,
    },
}

impl TryFrom<&ManageWorkspaceTool> for ManageWorkspaceRequest {
    type Error = anyhow::Error;

    fn try_from(tool: &ManageWorkspaceTool) -> Result<Self> {
        let force = tool.force.unwrap_or(false);
        let operation = ManageWorkspaceOperation::parse(tool.operation.as_str())?;

        match operation {
            ManageWorkspaceOperation::Index => Ok(Self::Index {
                path: tool.path.clone(),
                force,
            }),
            ManageWorkspaceOperation::Remove => {
                let workspace_id = tool.workspace_id.clone().ok_or_else(|| {
                    anyhow!("'workspace_id' parameter required for 'remove' operation")
                })?;
                Ok(Self::Remove { workspace_id })
            }
            ManageWorkspaceOperation::List => Ok(Self::List),
            ManageWorkspaceOperation::Refresh => {
                let workspace_id = tool.workspace_id.clone().ok_or_else(|| {
                    anyhow!("'workspace_id' parameter required for 'refresh' operation")
                })?;
                Ok(Self::Refresh {
                    workspace_id,
                    force,
                })
            }
            ManageWorkspaceOperation::Open => Ok(Self::Open {
                path: tool.path.clone(),
                workspace_id: tool.workspace_id.clone(),
                force,
            }),
            ManageWorkspaceOperation::Rebuild => {
                if tool.path.is_none() && tool.workspace_id.is_none() {
                    return Err(anyhow!(
                        "'workspace_id' or 'path' parameter required for 'rebuild' operation"
                    ));
                }
                Ok(Self::Rebuild {
                    path: tool.path.clone(),
                    workspace_id: tool.workspace_id.clone(),
                })
            }
            ManageWorkspaceOperation::Status => Ok(Self::Status {
                workspace_id: tool.workspace_id.clone(),
                path: tool.path.clone(),
            }),
            ManageWorkspaceOperation::Health => Ok(Self::Health {
                detailed: tool.detailed.unwrap_or(false),
            }),
            ManageWorkspaceOperation::Dashboard => Ok(Self::Dashboard),
            ManageWorkspaceOperation::RecoverEdit => {
                let edit_id = tool
                    .edit_id()
                    .ok_or_else(|| {
                        anyhow!("'edit_id' parameter required for 'recover_edit' operation")
                    })?
                    .to_string();
                let recovery_action = tool.recovery_action().unwrap_or("resume").to_string();
                Ok(Self::RecoverEdit {
                    edit_id,
                    recovery_action,
                })
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct ManageWorkspaceTool {
    /// Operation to perform: "index", "list", "open", "remove", "refresh", "health", "rebuild", "status", "recover_edit", "dashboard"
    ///
    /// EXAMPLES:
    /// Index workspace:      {"operation": "index", "path": null, "force": false}
    /// List workspaces:      {"operation": "list"}
    /// Status of every checkout: {"operation": "status"}
    /// Status of one checkout:   {"operation": "status", "path": "/path/to/project"}
    /// Rebuild an index from scratch: {"operation": "rebuild", "path": "/path/to/project"}
    /// Open workspace:       {"operation": "open", "workspace_id": "workspace-id"}
    /// Open by path:         {"operation": "open", "path": "/path/to/project"}
    /// Refresh workspace:    {"operation": "refresh", "workspace_id": "workspace-id", "force": true}
    /// Open and force sync:   {"operation": "open", "workspace_id": "workspace-id", "force": true}
    /// Health check:         {"operation": "health", "detailed": true}
    /// Launch dashboard:      {"operation": "dashboard"}
    pub operation: String,

    // Optional parameters used by various operations
    /// Path to workspace (used by: index, open, rebuild, status)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,

    /// Force complete re-indexing, bypassing incremental check (used by: index, refresh, open). Use when indexing code changed but source files are unchanged on disk
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        deserialize_with = "crate::utils::serde_lenient::deserialize_option_bool_lenient"
    )]
    pub force: Option<bool>,

    /// edit_id (used by: recover_edit)
    #[serde(skip_serializing_if = "Option::is_none", alias = "edit_id")]
    pub name: Option<String>,

    /// Workspace ID (used by: remove, refresh, open, rebuild, status) or recovery_action (used by: recover_edit)
    #[serde(skip_serializing_if = "Option::is_none", alias = "recovery_action")]
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
    pub fn edit_id(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn recovery_action(&self) -> Option<&str> {
        self.workspace_id.as_deref()
    }
    /// Call with `skip_embeddings: true` to suppress the embedding pipeline
    /// (used by auto-indexing to avoid expensive sidecar startup on init).
    pub async fn call_tool_with_options(
        &self,
        handler: &JulieServerHandler,
        skip_embeddings: bool,
    ) -> Result<CallToolResult> {
        info!("🏗️ Managing workspace with operation: {}", self.operation);
        let request = ManageWorkspaceRequest::try_from(self)?;
        self.dispatch_request(handler, request, skip_embeddings)
            .await
    }

    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        self.call_tool_with_options(handler, false).await
    }

    async fn dispatch_request(
        &self,
        handler: &JulieServerHandler,
        request: ManageWorkspaceRequest,
        skip_embeddings: bool,
    ) -> Result<CallToolResult> {
        match request {
            ManageWorkspaceRequest::Index { path, force } => {
                self.handle_index_command(handler, path, force, skip_embeddings)
                    .await
            }
            ManageWorkspaceRequest::Remove { workspace_id } => {
                self.handle_remove_command(handler, &workspace_id).await
            }
            ManageWorkspaceRequest::List => self.handle_list_command(handler).await,
            ManageWorkspaceRequest::Refresh {
                workspace_id,
                force,
            } => {
                self.handle_refresh_command(handler, &workspace_id, force)
                    .await
            }
            ManageWorkspaceRequest::Open {
                path,
                workspace_id,
                force,
            } => {
                self.handle_open_command(handler, path, workspace_id, force)
                    .await
            }
            ManageWorkspaceRequest::Rebuild { path, workspace_id } => {
                self.handle_rebuild_command(handler, path, workspace_id)
                    .await
            }
            ManageWorkspaceRequest::Status { workspace_id, path } => {
                self.handle_status_command(handler, workspace_id, path)
                    .await
            }
            ManageWorkspaceRequest::Health { detailed } => {
                self.handle_health_command(handler, detailed).await
            }
            ManageWorkspaceRequest::Dashboard => self.handle_dashboard_command().await,
            ManageWorkspaceRequest::RecoverEdit {
                edit_id,
                recovery_action,
            } => {
                recover_edit::handle_recover_edit_command(handler, &edit_id, &recovery_action).await
            }
        }
    }
}
