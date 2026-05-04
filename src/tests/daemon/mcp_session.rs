#[cfg(test)]
mod tests {
    use crate::daemon::mcp_session::workspace_ids_to_disconnect;

    #[test]
    fn test_workspace_ids_to_disconnect_includes_startup_when_attached() {
        let ids = workspace_ids_to_disconnect("startup", vec!["rebound".to_string()], true);

        assert_eq!(ids, vec!["rebound".to_string(), "startup".to_string()]);
    }

    #[test]
    fn test_workspace_ids_to_disconnect_deduplicates_and_sorts_rebound_ids() {
        let ids = workspace_ids_to_disconnect(
            "startup",
            vec![
                "workspace-c".to_string(),
                "workspace-b".to_string(),
                "workspace-c".to_string(),
                "startup".to_string(),
            ],
            true,
        );

        assert_eq!(
            ids,
            vec![
                "startup".to_string(),
                "workspace-b".to_string(),
                "workspace-c".to_string(),
            ]
        );
    }

    #[test]
    fn test_workspace_ids_to_disconnect_omits_deferred_startup_when_not_attached() {
        let ids = workspace_ids_to_disconnect("startup", vec!["rebound".to_string()], false);

        assert_eq!(ids, vec!["rebound".to_string()]);
    }
}
