//! Tests for WorkspaceTarget enum and resolve_workspace_filter functions.
//!
//! These tests verify the workspace resolution logic that maps user-provided
//! workspace parameters ("primary", "all", or a workspace ID) to the
//! WorkspaceTarget enum used by all tool callers.

#[cfg(test)]
mod tests {
    use crate::tools::navigation::resolution::WorkspaceTarget;

    // =========================================================================
    // WorkspaceTarget enum tests
    // =========================================================================

    #[test]
    fn test_workspace_target_primary_variant() {
        let target = WorkspaceTarget::Primary;
        assert!(matches!(target, WorkspaceTarget::Primary));
    }

    #[test]
    fn test_workspace_target_reference_variant() {
        let target = WorkspaceTarget::Reference("some_workspace_id".to_string());
        match &target {
            WorkspaceTarget::Reference(id) => assert_eq!(id, "some_workspace_id"),
            _ => panic!("Expected Reference variant"),
        }
    }

    #[test]
    fn test_workspace_target_all_variant() {
        let target = WorkspaceTarget::All;
        assert!(matches!(target, WorkspaceTarget::All));
    }

    #[test]
    fn test_workspace_target_debug_impl() {
        // Ensure Debug is derived
        let primary = WorkspaceTarget::Primary;
        let reference = WorkspaceTarget::Reference("test_id".to_string());
        let all = WorkspaceTarget::All;

        assert_eq!(format!("{:?}", primary), "Primary");
        assert_eq!(
            format!("{:?}", reference),
            "Reference(\"test_id\")"
        );
        assert_eq!(format!("{:?}", all), "All");
    }

    #[test]
    fn test_workspace_target_clone() {
        let original = WorkspaceTarget::Reference("ws_123".to_string());
        let cloned = original.clone();
        match (&original, &cloned) {
            (WorkspaceTarget::Reference(a), WorkspaceTarget::Reference(b)) => {
                assert_eq!(a, b);
            }
            _ => panic!("Clone should preserve variant"),
        }
    }

    #[test]
    fn test_workspace_target_eq() {
        assert_eq!(WorkspaceTarget::Primary, WorkspaceTarget::Primary);
        assert_eq!(WorkspaceTarget::All, WorkspaceTarget::All);
        assert_eq!(
            WorkspaceTarget::Reference("abc".to_string()),
            WorkspaceTarget::Reference("abc".to_string())
        );
        assert_ne!(WorkspaceTarget::Primary, WorkspaceTarget::All);
        assert_ne!(
            WorkspaceTarget::Reference("a".to_string()),
            WorkspaceTarget::Reference("b".to_string())
        );
    }

    // =========================================================================
    // resolve_workspace_filter function tests
    // =========================================================================

    use crate::handler::JulieServerHandler;
    use crate::tools::navigation::resolution::resolve_workspace_filter;

    #[tokio::test]
    async fn test_resolve_workspace_filter_all_returns_all() {
        let handler = JulieServerHandler::new_for_test().await.unwrap();
        let result = resolve_workspace_filter(Some("all"), &handler).await.unwrap();
        assert_eq!(result, WorkspaceTarget::All);
    }

    #[tokio::test]
    async fn test_resolve_workspace_filter_primary_returns_primary() {
        let handler = JulieServerHandler::new_for_test().await.unwrap();
        let result = resolve_workspace_filter(Some("primary"), &handler).await.unwrap();
        assert_eq!(result, WorkspaceTarget::Primary);
    }

    #[tokio::test]
    async fn test_resolve_workspace_filter_none_defaults_to_primary() {
        let handler = JulieServerHandler::new_for_test().await.unwrap();
        let result = resolve_workspace_filter(None, &handler).await.unwrap();
        assert_eq!(result, WorkspaceTarget::Primary);
    }

    // =========================================================================
    // resolve_workspace_filter — daemon mode tests
    // =========================================================================

    use std::path::PathBuf;
    use std::sync::Arc;
    use tokio::sync::RwLock;
    use tokio_util::sync::CancellationToken;

    use crate::daemon_state::{DaemonState, LoadedWorkspace, WorkspaceLoadStatus};
    use crate::registry::GlobalRegistry;
    use crate::workspace::JulieWorkspace;

    /// Create a handler with daemon state (empty). Use `insert_daemon_workspaces`
    /// to add workspace entries from an async context.
    fn daemon_handler() -> (Arc<RwLock<DaemonState>>, JulieServerHandler) {
        let registry = Arc::new(RwLock::new(GlobalRegistry::new()));
        let ct = CancellationToken::new();
        let julie_home = PathBuf::from("/tmp/test-julie-home");
        let ds = Arc::new(RwLock::new(DaemonState::new(registry, julie_home, ct)));
        let handler =
            JulieServerHandler::new_with_daemon_state(PathBuf::from("/tmp/test"), ds.clone())
                .unwrap();
        (ds, handler)
    }

    /// Helper: insert workspace IDs into daemon state (call from async context).
    async fn insert_daemon_workspaces(ds: &Arc<RwLock<DaemonState>>, workspace_ids: &[&str]) {
        let mut state = ds.write().await;
        for ws_id in workspace_ids {
            state.workspaces.insert(
                ws_id.to_string(),
                LoadedWorkspace {
                    workspace: JulieWorkspace::empty_shell(PathBuf::from(format!(
                        "/tmp/{}",
                        ws_id
                    ))),
                    status: WorkspaceLoadStatus::Ready,
                    path: PathBuf::from(format!("/tmp/{}", ws_id)),
                },
            );
        }
    }

    #[tokio::test]
    async fn test_daemon_mode_valid_workspace_id_resolves_to_reference() {
        let (ds, handler) = daemon_handler();
        insert_daemon_workspaces(&ds, &["project_alpha", "project_beta"]).await;

        let result = resolve_workspace_filter(Some("project_alpha"), &handler)
            .await
            .unwrap();
        assert_eq!(
            result,
            WorkspaceTarget::Reference("project_alpha".to_string())
        );
    }

    #[tokio::test]
    async fn test_daemon_mode_second_valid_workspace_id() {
        let (ds, handler) = daemon_handler();
        insert_daemon_workspaces(&ds, &["project_alpha", "project_beta"]).await;

        let result = resolve_workspace_filter(Some("project_beta"), &handler)
            .await
            .unwrap();
        assert_eq!(
            result,
            WorkspaceTarget::Reference("project_beta".to_string())
        );
    }

    #[tokio::test]
    async fn test_daemon_mode_invalid_workspace_id_returns_error() {
        let (ds, handler) = daemon_handler();
        insert_daemon_workspaces(&ds, &["project_alpha", "project_beta"]).await;

        let result = resolve_workspace_filter(Some("nonexistent"), &handler).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("not found"),
            "Error should mention 'not found'. Got: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_daemon_mode_fuzzy_match_suggestion() {
        let (ds, handler) = daemon_handler();
        insert_daemon_workspaces(&ds, &["project_alpha", "project_beta"]).await;

        // "project_alphx" is 1 edit away from "project_alpha" — should get suggestion
        let result = resolve_workspace_filter(Some("project_alphx"), &handler).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Did you mean 'project_alpha'"),
            "Should suggest 'project_alpha'. Got: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_daemon_mode_primary_still_works() {
        let (ds, handler) = daemon_handler();
        insert_daemon_workspaces(&ds, &["project_alpha"]).await;

        let result = resolve_workspace_filter(Some("primary"), &handler)
            .await
            .unwrap();
        assert_eq!(result, WorkspaceTarget::Primary);
    }

    #[tokio::test]
    async fn test_daemon_mode_all_still_works() {
        let (ds, handler) = daemon_handler();
        insert_daemon_workspaces(&ds, &["project_alpha"]).await;

        let result = resolve_workspace_filter(Some("all"), &handler)
            .await
            .unwrap();
        assert_eq!(result, WorkspaceTarget::All);
    }

    #[tokio::test]
    async fn test_daemon_mode_none_defaults_to_primary() {
        let (ds, handler) = daemon_handler();
        insert_daemon_workspaces(&ds, &["project_alpha"]).await;

        let result = resolve_workspace_filter(None, &handler).await.unwrap();
        assert_eq!(result, WorkspaceTarget::Primary);
    }

    #[tokio::test]
    async fn test_daemon_mode_empty_state_rejects_unknown_id() {
        let (_ds, handler) = daemon_handler();
        // No workspaces inserted — any workspace ID should fail

        let result = resolve_workspace_filter(Some("anything"), &handler).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("not found"),
            "Should say not found. Got: {}",
            err
        );
    }
}
