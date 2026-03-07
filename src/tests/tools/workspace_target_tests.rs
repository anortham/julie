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
}
