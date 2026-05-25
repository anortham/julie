use crate::mcp_compat::CallToolResult;
use anyhow::{Result, anyhow};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::handler::JulieServerHandler;

pub(crate) mod force_safeguards;
mod index;
pub(crate) mod registry;

//******************//
// Workspace Management Commands //
//******************//

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ManageWorkspaceOperation {
    Index,
    Register,
    Remove,
    List,
    Clean,
    Refresh,
    Open,
    Stats,
    Health,
}

impl ManageWorkspaceOperation {
    /// Single source of truth for operation names. Ordered the way the help
    /// string is presented to users — `valid_operations_help()` and `parse()`
    /// both derive from this table so they cannot drift apart.
    const OPERATIONS: &'static [(&'static str, Self)] = &[
        ("index", Self::Index),
        ("list", Self::List),
        ("register", Self::Register),
        ("remove", Self::Remove),
        ("stats", Self::Stats),
        ("clean", Self::Clean),
        ("refresh", Self::Refresh),
        ("open", Self::Open),
        ("health", Self::Health),
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

    pub(crate) fn primary_index_request(
        arguments: Option<&serde_json::Map<String, serde_json::Value>>,
    ) -> bool {
        let Some(arguments) = arguments else {
            return false;
        };

        matches!(Self::from_arguments(Some(arguments)), Some(Self::Index))
            && arguments.get("path").is_none_or(serde_json::Value::is_null)
    }

    pub(crate) fn request_targets_primary(
        arguments: Option<&serde_json::Map<String, serde_json::Value>>,
    ) -> bool {
        let Some(arguments) = arguments else {
            return false;
        };

        match Self::from_arguments(Some(arguments)) {
            // `register` is intentionally excluded: it must not silently bind
            // the startup-hint/CWD as primary on the user's behalf. The tool
            // body resolves the target path without treating the request as a
            // primary-targeting operation.
            Some(Self::List | Self::Remove | Self::Health) => true,
            Some(Self::Stats) => arguments
                .get("workspace_id")
                .and_then(serde_json::Value::as_str)
                .is_none_or(|workspace_id| workspace_id == "primary"),
            Some(Self::Index) => arguments.get("path").is_none_or(serde_json::Value::is_null),
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
    Register {
        path: String,
        name: Option<String>,
        force: bool,
    },
    Remove {
        workspace_id: String,
    },
    List,
    Clean,
    Refresh {
        workspace_id: String,
        force: bool,
    },
    Open {
        path: Option<String>,
        workspace_id: Option<String>,
        force: bool,
    },
    Stats {
        workspace_id: Option<String>,
    },
    Health {
        detailed: bool,
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
            ManageWorkspaceOperation::Register => {
                let path = tool
                    .path
                    .clone()
                    .ok_or_else(|| anyhow!("'path' parameter required for 'register' operation"))?;
                Ok(Self::Register {
                    path,
                    name: tool.name.clone(),
                    force,
                })
            }
            ManageWorkspaceOperation::Remove => {
                let workspace_id = tool.workspace_id.clone().ok_or_else(|| {
                    anyhow!("'workspace_id' parameter required for 'remove' operation")
                })?;
                Ok(Self::Remove { workspace_id })
            }
            ManageWorkspaceOperation::List => Ok(Self::List),
            ManageWorkspaceOperation::Clean => Ok(Self::Clean),
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
            ManageWorkspaceOperation::Stats => Ok(Self::Stats {
                workspace_id: tool.workspace_id.clone(),
            }),
            ManageWorkspaceOperation::Health => Ok(Self::Health {
                detailed: tool.detailed.unwrap_or(false),
            }),
        }
    }
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

    /// Force complete re-indexing, bypassing incremental check (used by: index, register, refresh, open). Use when indexing code changed but source files are unchanged on disk
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
            ManageWorkspaceRequest::Register { path, name, force } => {
                self.handle_register_command(handler, &path, name, force)
                    .await
            }
            ManageWorkspaceRequest::Remove { workspace_id } => {
                self.handle_remove_command(handler, &workspace_id).await
            }
            ManageWorkspaceRequest::List => self.handle_list_command(handler).await,
            ManageWorkspaceRequest::Clean => self.handle_clean_command(handler).await,
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
            ManageWorkspaceRequest::Stats { workspace_id } => {
                self.handle_stats_command(handler, workspace_id).await
            }
            ManageWorkspaceRequest::Health { detailed } => {
                self.handle_health_command(handler, detailed).await
            }
        }
    }
}
