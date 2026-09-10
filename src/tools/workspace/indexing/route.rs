use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;

use crate::handler::JulieServerHandler;
use crate::tools::workspace::indexing::state::SharedIndexingRuntime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IndexRouteRepairReason {
    PrimaryBindingUnavailable,
    WorkspaceIdentityUnavailable,
    StorageAnchorUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IndexRouteError {
    pub reason: IndexRouteRepairReason,
    detail: String,
}

impl IndexRouteError {
    fn new(reason: IndexRouteRepairReason, detail: impl Into<String>) -> Self {
        Self {
            reason,
            detail: detail.into(),
        }
    }
}

impl fmt::Display for IndexRouteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.detail)
    }
}

impl std::error::Error for IndexRouteError {}

pub(crate) struct IndexRoute {
    pub workspace_id: String,
    pub workspace_root: PathBuf,
    pub index_dir: PathBuf,
    pub tantivy_path: PathBuf,
    pub is_primary: bool,
    pub indexing_runtime: Option<SharedIndexingRuntime>,
}

impl fmt::Debug for IndexRoute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IndexRoute")
            .field("workspace_id", &self.workspace_id)
            .field("workspace_root", &self.workspace_root)
            .field("index_dir", &self.index_dir)
            .field("tantivy_path", &self.tantivy_path)
            .field("is_primary", &self.is_primary)
            .finish()
    }
}

impl IndexRoute {
    async fn path_backed_route(
        handler: &JulieServerHandler,
        workspace_id: String,
        workspace_root: PathBuf,
        is_primary: bool,
    ) -> std::result::Result<Self, IndexRouteError> {
        let index_dir = handler
            .workspace_index_dir_for(&workspace_id)
            .await
            .map_err(|err| {
                IndexRouteError::new(
                    IndexRouteRepairReason::StorageAnchorUnavailable,
                    format!(
                        "failed to resolve index dir for workspace '{}': {err}",
                        workspace_id
                    ),
                )
            })?;
        let tantivy_path = index_dir.join("tantivy");

        Ok(Self {
            workspace_id,
            workspace_root,
            index_dir,
            tantivy_path,
            is_primary,
            indexing_runtime: None,
        })
    }

    pub(crate) async fn for_current_primary(
        handler: &JulieServerHandler,
    ) -> std::result::Result<Self, IndexRouteError> {
        let binding = handler.require_primary_workspace_binding().map_err(|err| {
            IndexRouteError::new(
                IndexRouteRepairReason::PrimaryBindingUnavailable,
                format!("current primary binding unavailable: {err}"),
            )
        })?;

        match handler.primary_workspace_snapshot().await {
            Ok(snapshot) => {
                let snapshot_binding = snapshot.binding.clone();
                let index_dir = handler
                    .workspace_index_dir_for(&snapshot_binding.workspace_id)
                    .await
                    .map_err(|err| {
                        IndexRouteError::new(
                            IndexRouteRepairReason::StorageAnchorUnavailable,
                            format!(
                                "failed to resolve index dir for current primary '{}': {err}",
                                snapshot_binding.workspace_id
                            ),
                        )
                    })?;
                Ok(Self {
                    workspace_id: snapshot_binding.workspace_id,
                    workspace_root: snapshot_binding.workspace_root,
                    tantivy_path: index_dir.join("tantivy"),
                    index_dir,
                    is_primary: true,
                    indexing_runtime: snapshot.indexing_runtime,
                })
            }
            Err(err) => Self::path_backed_route(
                handler,
                binding.workspace_id.clone(),
                binding.workspace_root.clone(),
                true,
            )
            .await
            .map_err(|fallback_err| {
                IndexRouteError::new(
                    IndexRouteRepairReason::StorageAnchorUnavailable,
                    format!(
                        "current primary route unavailable for '{}': {err}; fallback failed: {fallback_err}",
                        binding.workspace_id
                    ),
                )
            }),
        }
    }

    pub(crate) async fn for_workspace_path(
        handler: &JulieServerHandler,
        workspace_path: &Path,
    ) -> std::result::Result<Self, IndexRouteError> {
        let workspace_root = workspace_path
            .canonicalize()
            .unwrap_or_else(|_| workspace_path.to_path_buf());
        let workspace_id =
            crate::workspace::registry::generate_workspace_id(&workspace_root.to_string_lossy())
                .map_err(|err| {
                    IndexRouteError::new(
                        IndexRouteRepairReason::WorkspaceIdentityUnavailable,
                        format!(
                            "failed to resolve workspace identity for '{}': {err}",
                            workspace_root.display()
                        ),
                    )
                })?;

        if handler.current_workspace_id().as_deref() == Some(workspace_id.as_str()) {
            return Self::for_current_primary(handler).await;
        }

        Self::path_backed_route(handler, workspace_id, workspace_root, false).await
    }
}
