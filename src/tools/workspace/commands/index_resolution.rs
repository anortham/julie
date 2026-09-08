//! Workspace index path and target resolution logic.

use super::ManageWorkspaceTool;
use super::force_safeguards::workspace_ids_for_force_reindex;
use crate::handler::JulieServerHandler;
use anyhow::Result;
use std::path::PathBuf;
use tracing::{debug, info};

pub(crate) struct ResolvedIndexTarget {
    pub canonical_path: PathBuf,
    pub gate_workspace_id: String,
    pub current_primary_root: PathBuf,
    pub current_primary_id: Option<String>,
    pub is_non_primary_target: bool,
    pub effective_force_reindex: bool,
    pub force_reindex_workspace_ids: Vec<String>,
}

impl ManageWorkspaceTool {
    pub(crate) async fn resolve_index_target(
        &self,
        handler: &JulieServerHandler,
        path: Option<String>,
        force: bool,
    ) -> Result<ResolvedIndexTarget> {
        let explicit_path_requested = path.is_some();

        let loaded_workspace = handler.get_workspace().await?;
        let current_primary_root = if explicit_path_requested || loaded_workspace.is_none() {
            handler.current_workspace_root()
        } else {
            handler.require_primary_workspace_root()?
        };
        let current_primary_id = handler.current_workspace_id().or_else(|| {
            crate::workspace::registry::generate_workspace_id(
                &current_primary_root.to_string_lossy(),
            )
            .ok()
        });
        let bound_primary_id = handler.current_workspace_id();

        let original_path = match path {
            Some(ref p) => {
                let expanded = shellexpand::tilde(p).to_string();
                PathBuf::from(expanded)
            }
            None => current_primary_root.clone(),
        };

        let original_workspace_candidate = if original_path.is_file() {
            original_path
                .parent()
                .ok_or_else(|| anyhow::anyhow!("Cannot determine parent directory"))?
                .to_path_buf()
        } else {
            original_path.clone()
        };
        let primary_canonical = current_primary_root
            .canonicalize()
            .unwrap_or_else(|_| current_primary_root.clone());
        let request_canonical = original_workspace_candidate
            .canonicalize()
            .unwrap_or_else(|_| original_workspace_candidate.clone());
        let explicit_path_outside_primary = explicit_path_requested
            && request_canonical != primary_canonical
            && !request_canonical.starts_with(&primary_canonical);

        let registered_secondary = if let Some(ref db) = handler.daemon_db {
            if let Some(ref primary_id) = bound_primary_id {
                db.get_workspace_by_path(request_canonical.to_string_lossy().as_ref())
                    .ok()
                    .flatten()
                    .map(|row| row.workspace_id != *primary_id)
                    .unwrap_or(false)
            } else {
                false
            }
        } else {
            false
        };

        let is_non_primary_target =
            registered_secondary || (handler.daemon_db.is_none() && explicit_path_outside_primary);
        let explicit_current_root_requested =
            explicit_path_requested && request_canonical == primary_canonical;
        let use_requested_root_directly = explicit_current_root_requested
            || explicit_path_outside_primary
            || is_non_primary_target;

        let workspace_path = if use_requested_root_directly {
            debug!(
                "Using requested/current workspace root directly without ancestor marker discovery"
            );
            original_workspace_candidate.clone()
        } else {
            self.resolve_workspace_path(path, Some(&current_primary_root))?
        };

        let canonical_path = workspace_path
            .canonicalize()
            .unwrap_or_else(|_| workspace_path.clone());
        crate::workspace::root_safety::reject_sensitive_workspace_root(&canonical_path)?;

        let gate_workspace_id = if explicit_path_requested || is_non_primary_target {
            crate::workspace::registry::generate_workspace_id(&canonical_path.to_string_lossy())
                .unwrap_or_else(|_| canonical_path.to_string_lossy().to_string())
        } else {
            current_primary_id
                .clone()
                .unwrap_or_else(|| canonical_path.to_string_lossy().to_string())
        };

        let semantic_engine_refresh_needed = self
            .semantic_index_engine_refresh_needed_for_path(handler, &canonical_path)
            .await?;
        let effective_force_reindex = force || semantic_engine_refresh_needed;

        info!("🎯 Resolved workspace path: {}", canonical_path.display());
        if semantic_engine_refresh_needed {
            info!(
                "Index semantic version changed or missing; treating index request as an effective full re-index"
            );
        }
        let force_reindex_workspace_ids = if effective_force_reindex {
            workspace_ids_for_force_reindex(
                &canonical_path,
                current_primary_id.as_deref(),
                is_non_primary_target,
            )?
        } else {
            Vec::new()
        };

        Ok(ResolvedIndexTarget {
            canonical_path,
            gate_workspace_id,
            current_primary_root,
            current_primary_id,
            is_non_primary_target,
            effective_force_reindex,
            force_reindex_workspace_ids,
        })
    }
}
