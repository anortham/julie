use std::collections::HashSet;
use std::path::PathBuf;

use crate::workspace::startup_hint::WorkspaceStartupHint;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrimaryWorkspaceBinding {
    pub workspace_id: String,
    pub workspace_root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionWorkspaceState {
    pub startup_hint: WorkspaceStartupHint,
    primary_binding: Option<PrimaryWorkspaceBinding>,
    secondary_workspace_ids: HashSet<String>,
}

impl SessionWorkspaceState {
    pub fn new(startup_hint: WorkspaceStartupHint) -> Self {
        Self {
            startup_hint,
            primary_binding: None,
            secondary_workspace_ids: HashSet::new(),
        }
    }

    pub fn primary_binding(&self) -> Option<PrimaryWorkspaceBinding> {
        self.primary_binding.clone()
    }

    pub fn bind_primary(&mut self, workspace_id: impl Into<String>, workspace_root: PathBuf) {
        let workspace_id = workspace_id.into();
        self.secondary_workspace_ids.remove(&workspace_id);
        self.primary_binding = Some(PrimaryWorkspaceBinding {
            workspace_id,
            workspace_root,
        });
    }

    pub fn current_workspace_root(&self) -> PathBuf {
        self.primary_binding
            .as_ref()
            .map(|binding| binding.workspace_root.clone())
            .unwrap_or_else(|| self.startup_hint.path.clone())
    }

    pub fn current_workspace_id(&self) -> Option<String> {
        self.primary_binding
            .as_ref()
            .map(|binding| binding.workspace_id.clone())
    }

    pub fn active_workspace_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.secondary_workspace_ids.iter().cloned().collect();
        if let Some(primary_id) = self.current_workspace_id() {
            ids.push(primary_id);
        }
        ids.sort();
        ids.dedup();
        ids
    }

    pub fn is_workspace_active(&self, workspace_id: &str) -> bool {
        self.current_workspace_id().as_deref() == Some(workspace_id)
            || self.secondary_workspace_ids.contains(workspace_id)
    }

    pub fn mark_workspace_active(&mut self, workspace_id: impl Into<String>) -> bool {
        let workspace_id = workspace_id.into();
        if self.current_workspace_id().as_deref() == Some(workspace_id.as_str()) {
            return false;
        }

        self.secondary_workspace_ids.insert(workspace_id)
    }
}
