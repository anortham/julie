use std::collections::HashSet;
use std::path::PathBuf;

use crate::daemon::session::SessionLifecyclePhase;
use crate::workspace::startup_hint::WorkspaceStartupHint;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrimaryWorkspaceBinding {
    pub workspace_id: String,
    pub workspace_root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionWorkspaceState {
    pub startup_hint: WorkspaceStartupHint,
    pub client_supports_workspace_roots: bool,
    pub roots_dirty: bool,
    pub last_roots_snapshot: Option<Vec<PathBuf>>,
    serving_active: bool,
    closing: bool,
    primary_swap_in_progress: bool,
    primary_binding: Option<PrimaryWorkspaceBinding>,
    secondary_workspace_ids: HashSet<String>,
    attached_workspace_ids: HashSet<String>,
}

impl SessionWorkspaceState {
    pub fn new(startup_hint: WorkspaceStartupHint) -> Self {
        Self {
            startup_hint,
            client_supports_workspace_roots: false,
            roots_dirty: false,
            last_roots_snapshot: None,
            serving_active: false,
            closing: false,
            primary_swap_in_progress: false,
            primary_binding: None,
            secondary_workspace_ids: HashSet::new(),
            attached_workspace_ids: HashSet::new(),
        }
    }

    pub fn lifecycle_phase(&self) -> SessionLifecyclePhase {
        if self.closing {
            SessionLifecyclePhase::Closing
        } else if self.primary_swap_in_progress || self.primary_binding.is_none() {
            SessionLifecyclePhase::Connecting
        } else if self.serving_active {
            SessionLifecyclePhase::Serving
        } else {
            SessionLifecyclePhase::Bound
        }
    }

    pub fn begin_primary_swap(&mut self) {
        self.primary_swap_in_progress = true;
    }

    pub fn complete_primary_swap(&mut self) {
        self.primary_swap_in_progress = false;
    }

    pub fn primary_swap_in_progress(&self) -> bool {
        self.primary_swap_in_progress
    }

    pub fn primary_binding(&self) -> Option<PrimaryWorkspaceBinding> {
        self.primary_binding.clone()
    }

    pub fn roots_dirty(&self) -> bool {
        self.roots_dirty
    }

    pub fn mark_roots_dirty(&mut self) {
        self.roots_dirty = true;
    }

    pub fn bind_primary(&mut self, workspace_id: impl Into<String>, workspace_root: PathBuf) {
        let workspace_id = workspace_id.into();
        self.secondary_workspace_ids.remove(&workspace_id);
        self.primary_binding = Some(PrimaryWorkspaceBinding {
            workspace_id,
            workspace_root,
        });
    }

    pub fn clear_primary_binding(&mut self) {
        self.primary_binding = None;
    }

    pub fn mark_serving(&mut self) {
        self.serving_active = true;
    }

    pub fn mark_closing(&mut self) {
        self.closing = true;
    }

    pub fn apply_root_snapshot(
        &mut self,
        primary: PrimaryWorkspaceBinding,
        secondary_workspace_ids: HashSet<String>,
        roots: Vec<PathBuf>,
    ) {
        let primary_workspace_id = primary.workspace_id.clone();
        self.primary_binding = Some(primary);
        self.secondary_workspace_ids = secondary_workspace_ids;
        self.secondary_workspace_ids.remove(&primary_workspace_id);
        self.last_roots_snapshot = Some(roots);
        self.roots_dirty = false;
    }

    pub fn current_workspace_root(&self) -> PathBuf {
        if self.primary_swap_in_progress {
            return self.startup_hint.path.clone();
        }

        self.primary_binding
            .as_ref()
            .map(|binding| binding.workspace_root.clone())
            .unwrap_or_else(|| self.startup_hint.path.clone())
    }

    pub fn current_workspace_id(&self) -> Option<String> {
        if self.primary_swap_in_progress {
            return None;
        }

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

    pub fn has_secondary_workspace(&self, workspace_id: &str) -> bool {
        self.secondary_workspace_ids.contains(workspace_id)
    }

    /// Returns the set of workspace ids that were attached at any point during
    /// this session. This is append-only session bookkeeping for cleanup, not a
    /// statement about what is currently loaded.
    pub fn session_attached_workspace_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.attached_workspace_ids.iter().cloned().collect();
        ids.sort();
        ids.dedup();
        ids
    }

    /// Returns whether this workspace was attached at some point during the
    /// current session. This does not mean it is the currently loaded workspace.
    pub fn was_workspace_attached_in_session(&self, workspace_id: &str) -> bool {
        self.attached_workspace_ids.contains(workspace_id)
    }

    pub fn mark_workspace_attached(&mut self, workspace_id: impl Into<String>) -> bool {
        self.attached_workspace_ids.insert(workspace_id.into())
    }

    pub fn mark_workspace_active(&mut self, workspace_id: impl Into<String>) -> bool {
        let workspace_id = workspace_id.into();
        if self.current_workspace_id().as_deref() == Some(workspace_id.as_str()) {
            return false;
        }

        self.secondary_workspace_ids.insert(workspace_id)
    }
}
