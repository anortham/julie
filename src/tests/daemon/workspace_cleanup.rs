#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::Arc;
    use std::time::Duration;

    use crate::daemon::database::DaemonDatabase;
    use crate::daemon::workspace_pool::WorkspacePool;
    use crate::handler::JulieServerHandler;
    use crate::tools::workspace::ManageWorkspaceTool;
    use crate::tools::workspace::commands::registry::cleanup::path_missing_after_grace;
    use crate::workspace::registry::generate_workspace_id;

    async fn build_primary_bound_handler(
        temp_dir: &tempfile::TempDir,
    ) -> (
        Arc<DaemonDatabase>,
        Arc<WorkspacePool>,
        JulieServerHandler,
        String,
    ) {
        let indexes_dir = temp_dir.path().join("indexes");
        fs::create_dir_all(&indexes_dir).unwrap();

        let primary_root = temp_dir.path().join("primary");
        fs::create_dir_all(&primary_root).unwrap();
        fs::write(primary_root.join("main.rs"), "fn primary() {}\n").unwrap();

        let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());
        let pool = Arc::new(WorkspacePool::new(
            indexes_dir,
            Some(Arc::clone(&daemon_db)),
            None,
            None,
        ));

        let primary_path = primary_root.canonicalize().unwrap();
        let primary_path_str = primary_path.to_string_lossy().to_string();
        let primary_id = generate_workspace_id(&primary_path_str).unwrap();
        let primary_ws = pool
            .get_or_init(&primary_id, primary_path.clone())
            .await
            .expect("primary workspace should initialize");
        daemon_db
            .upsert_workspace(&primary_id, &primary_path_str, "ready")
            .unwrap();

        let handler = JulieServerHandler::new_with_shared_workspace(
            primary_ws,
            primary_path,
            Some(Arc::clone(&daemon_db)),
            Some(primary_id.clone()),
            None,
            None,
            None,
            None,
            Some(Arc::clone(&pool)),
        )
        .await
        .expect("handler should initialize");

        (daemon_db, pool, handler, primary_id)
    }

    fn extract_text(result: &crate::mcp_compat::CallToolResult) -> String {
        result
            .content
            .iter()
            .filter_map(|content_block| {
                serde_json::to_value(content_block).ok().and_then(|json| {
                    json.get("text")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                })
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[tokio::test]
    async fn test_manage_workspace_open_prunes_missing_inactive_workspace() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let (daemon_db, _pool, handler, _primary_id) = build_primary_bound_handler(&temp_dir).await;
        let indexes_dir = temp_dir.path().join("indexes");

        let target_root = temp_dir.path().join("stale-worktree");
        fs::create_dir_all(&target_root).unwrap();
        fs::write(target_root.join("lib.rs"), "pub fn stale() {}\n").unwrap();

        let target_path = target_root.canonicalize().unwrap();
        let target_path_str = target_path.to_string_lossy().to_string();
        let target_id = generate_workspace_id(&target_path_str).unwrap();
        daemon_db
            .upsert_workspace(&target_id, &target_path_str, "ready")
            .unwrap();
        fs::create_dir_all(indexes_dir.join(&target_id)).unwrap();

        fs::remove_dir_all(&target_root).unwrap();

        let result = ManageWorkspaceTool {
            operation: "open".to_string(),
            path: None,
            force: Some(false),
            name: None,
            workspace_id: Some(target_id.clone()),
            detailed: None,
        }
        .call_tool(&handler)
        .await
        .expect("open should return a cleanup result for a dead workspace");

        let text = extract_text(&result);
        assert!(
            text.to_lowercase().contains("removed") || text.to_lowercase().contains("pruned"),
            "open should report that the dead workspace was pruned: {text}"
        );
        assert!(
            text.to_lowercase().contains("missing") || text.to_lowercase().contains("gone"),
            "open should explain that the path is gone: {text}"
        );
        assert!(
            daemon_db.get_workspace(&target_id).unwrap().is_none(),
            "dead workspace row should be removed from daemon db"
        );
        assert!(
            !indexes_dir.join(&target_id).exists(),
            "dead workspace index directory should be removed"
        );

        let events = daemon_db.list_cleanup_events(10).unwrap();
        assert!(
            events
                .iter()
                .any(|event| event.workspace_id == target_id && event.action == "auto_prune"),
            "auto-prune should record a cleanup event"
        );
    }

    #[tokio::test]
    async fn test_manage_workspace_remove_refuses_active_workspace() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let (daemon_db, _pool, handler, primary_id) = build_primary_bound_handler(&temp_dir).await;

        let result = ManageWorkspaceTool {
            operation: "remove".to_string(),
            path: None,
            force: Some(false),
            name: None,
            workspace_id: Some(primary_id.clone()),
            detailed: None,
        }
        .call_tool(&handler)
        .await
        .expect("remove should return a blocked-delete message");

        let text = extract_text(&result);
        assert!(
            text.to_lowercase().contains("cannot delete")
                || text.to_lowercase().contains("blocked"),
            "remove should explain that deletion is blocked: {text}"
        );
        assert!(
            text.to_lowercase().contains("session") || text.to_lowercase().contains("active"),
            "remove should explain that the workspace is still active: {text}"
        );
        assert!(
            daemon_db.get_workspace(&primary_id).unwrap().is_some(),
            "active workspace row must remain in daemon db"
        );
    }

    #[tokio::test]
    async fn test_path_missing_after_grace_skips_transient_disappearance() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let workspace_path = temp_dir.path().join("transient-workspace");
        fs::create_dir_all(&workspace_path).unwrap();
        fs::remove_dir_all(&workspace_path).unwrap();

        let recreated_path = workspace_path.clone();
        let recreate = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            fs::create_dir_all(&recreated_path).unwrap();
        });

        let missing = path_missing_after_grace(&workspace_path, Duration::from_millis(60))
            .await
            .unwrap();
        recreate.await.unwrap();

        assert!(
            !missing,
            "path should not be treated as missing if it reappears during the grace window"
        );
    }
}
