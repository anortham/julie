use std::collections::HashSet;
use std::path::PathBuf;

use crate::handler::session_workspace::SessionWorkspaceState;
use crate::workspace::startup_hint::{WorkspaceStartupHint, WorkspaceStartupSource};

#[test]
fn test_session_workspace_state_defaults_to_startup_hint_root() {
    let startup_hint = WorkspaceStartupHint {
        path: PathBuf::from("/tmp/startup-root"),
        source: Some(WorkspaceStartupSource::Cwd),
    };

    let state = SessionWorkspaceState::new(startup_hint.clone());

    assert_eq!(state.startup_hint, startup_hint);
    assert_eq!(
        state.current_workspace_root(),
        PathBuf::from("/tmp/startup-root")
    );
    assert_eq!(state.current_workspace_id(), None);
    assert!(!state.client_supports_workspace_roots);
    assert!(!state.roots_dirty);
    assert_eq!(state.last_roots_snapshot, None);
    assert!(!state.has_secondary_workspace("any-workspace"));
}

#[test]
fn test_session_workspace_state_tracks_primary_and_secondary_ids() {
    let startup_hint = WorkspaceStartupHint {
        path: PathBuf::from("/tmp/startup-root"),
        source: Some(WorkspaceStartupSource::Cli),
    };

    let mut state = SessionWorkspaceState::new(startup_hint);
    state.bind_primary("primary_ws", PathBuf::from("/tmp/primary-root"));
    state.mark_workspace_active("primary_ws");
    state.mark_workspace_active("secondary_ws");

    assert_eq!(
        state.current_workspace_root(),
        PathBuf::from("/tmp/primary-root")
    );
    assert_eq!(state.current_workspace_id(), Some("primary_ws".to_string()));
    assert_eq!(
        state.active_workspace_ids(),
        vec!["primary_ws".to_string(), "secondary_ws".to_string()]
    );
    assert!(state.is_workspace_active("primary_ws"));
    assert!(state.is_workspace_active("secondary_ws"));
    assert!(!state.has_secondary_workspace("primary_ws"));
}

#[test]
fn test_session_workspace_state_distinguishes_attached_from_current_across_rebind() {
    let startup_hint = WorkspaceStartupHint {
        path: PathBuf::from("/tmp/startup-root"),
        source: Some(WorkspaceStartupSource::Cli),
    };

    let mut state = SessionWorkspaceState::new(startup_hint);
    state.bind_primary("workspace_a", PathBuf::from("/tmp/workspace-a"));
    state.mark_workspace_attached("workspace_a");

    state.bind_primary("workspace_b", PathBuf::from("/tmp/workspace-b"));
    state.mark_workspace_attached("workspace_b");

    assert_eq!(
        state.current_workspace_id(),
        Some("workspace_b".to_string())
    );
    assert!(state.was_workspace_attached_in_session("workspace_a"));
    assert!(state.was_workspace_attached_in_session("workspace_b"));
    assert_eq!(
        state.session_attached_workspace_ids(),
        vec!["workspace_a".to_string(), "workspace_b".to_string()]
    );
}

#[test]
fn test_session_workspace_state_hides_partially_published_primary_binding_during_swap() {
    let startup_hint = WorkspaceStartupHint {
        path: PathBuf::from("/tmp/startup-root"),
        source: Some(WorkspaceStartupSource::Cli),
    };

    let mut state = SessionWorkspaceState::new(startup_hint);
    state.begin_primary_swap();
    state.bind_primary("workspace_b", PathBuf::from("/tmp/workspace-b"));

    assert!(state.primary_swap_in_progress());
    assert_eq!(state.current_workspace_id(), None);
    assert_eq!(
        state.current_workspace_root(),
        PathBuf::from("/tmp/startup-root")
    );

    state.complete_primary_swap();

    assert_eq!(
        state.current_workspace_id(),
        Some("workspace_b".to_string())
    );
    assert_eq!(
        state.current_workspace_root(),
        PathBuf::from("/tmp/workspace-b")
    );
}

#[test]
fn test_session_workspace_state_apply_root_snapshot_replaces_secondary_workspace_ids() {
    let startup_hint = WorkspaceStartupHint {
        path: PathBuf::from("/tmp/startup-root"),
        source: Some(WorkspaceStartupSource::Cli),
    };

    let mut state = SessionWorkspaceState::new(startup_hint);
    state.apply_root_snapshot(
        crate::handler::session_workspace::PrimaryWorkspaceBinding {
            workspace_id: "primary_ws".to_string(),
            workspace_root: PathBuf::from("/tmp/primary-root"),
        },
        HashSet::from(["secondary_a".to_string(), "secondary_b".to_string()]),
        vec![
            PathBuf::from("/tmp/primary-root"),
            PathBuf::from("/tmp/secondary-a"),
            PathBuf::from("/tmp/secondary-b"),
        ],
    );

    state.apply_root_snapshot(
        crate::handler::session_workspace::PrimaryWorkspaceBinding {
            workspace_id: "primary_ws".to_string(),
            workspace_root: PathBuf::from("/tmp/primary-root"),
        },
        HashSet::from(["secondary_c".to_string()]),
        vec![
            PathBuf::from("/tmp/primary-root"),
            PathBuf::from("/tmp/secondary-c"),
        ],
    );

    assert_eq!(
        state.active_workspace_ids(),
        vec!["primary_ws".to_string(), "secondary_c".to_string()]
    );
    assert!(state.has_secondary_workspace("secondary_c"));
    assert!(!state.has_secondary_workspace("secondary_a"));
    assert!(!state.has_secondary_workspace("secondary_b"));
}
