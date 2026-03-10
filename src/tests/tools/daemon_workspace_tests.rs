//! Tests for daemon workspace registration/deregistration and handler helpers.
//!
//! These tests cover:
//! 1. DaemonState::register_project — new registration, idempotent re-registration
//! 2. DaemonState::deregister_project — removal, nonexistent workspace handling
//! 3. Handler helpers — get_database_for_workspace, get_search_index_for_workspace,
//!    get_workspace_root_for_target in daemon mode (happy + error paths)

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use tokio::sync::RwLock;
    use tokio_util::sync::CancellationToken;

    use crate::daemon_state::{DaemonState, LoadedWorkspace, WorkspaceLoadStatus};
    use crate::handler::JulieServerHandler;
    use crate::registry::GlobalRegistry;
    use crate::workspace::JulieWorkspace;

    // =========================================================================
    // Helper: create a DaemonState with a real temp julie_home
    // =========================================================================

    fn test_daemon_state_with_home(julie_home: PathBuf) -> Arc<RwLock<DaemonState>> {
        let registry = Arc::new(RwLock::new(GlobalRegistry::new()));
        let ct = CancellationToken::new();
        Arc::new(RwLock::new(DaemonState::new(registry, julie_home, ct)))
    }

    /// Create a handler wired to the given DaemonState.
    fn daemon_handler(ds: Arc<RwLock<DaemonState>>) -> JulieServerHandler {
        JulieServerHandler::new_with_daemon_state(PathBuf::from("/tmp/daemon-ws-test"), ds)
            .expect("failed to create daemon handler")
    }

    // =========================================================================
    // Registration tests
    // =========================================================================

    #[tokio::test]
    async fn test_daemon_register_project_creates_workspace() {
        let temp = tempfile::tempdir().unwrap();
        let julie_home = temp.path().join("julie-home");
        std::fs::create_dir_all(&julie_home).unwrap();

        let project_dir = temp.path().join("my-project");
        std::fs::create_dir_all(&project_dir).unwrap();

        let ds = test_daemon_state_with_home(julie_home);

        let result = DaemonState::register_project(&ds, &project_dir)
            .await
            .expect("register_project should succeed");

        // Should be newly created
        assert!(
            !result.already_existed,
            "First registration should report already_existed=false"
        );
        assert_eq!(result.name, "my-project");
        assert!(!result.workspace_id.is_empty());

        // Verify it's in both DaemonState.workspaces and GlobalRegistry
        let state = ds.read().await;
        assert!(
            state.workspaces.contains_key(&result.workspace_id),
            "Workspace should be in DaemonState.workspaces"
        );

        let registry = state.registry.read().await;
        assert!(
            registry.get_project(&result.workspace_id).is_some(),
            "Workspace should be in GlobalRegistry"
        );
    }

    #[tokio::test]
    async fn test_daemon_register_project_idempotent() {
        let temp = tempfile::tempdir().unwrap();
        let julie_home = temp.path().join("julie-home");
        std::fs::create_dir_all(&julie_home).unwrap();

        let project_dir = temp.path().join("idempotent-project");
        std::fs::create_dir_all(&project_dir).unwrap();

        let ds = test_daemon_state_with_home(julie_home);

        let first = DaemonState::register_project(&ds, &project_dir)
            .await
            .expect("first register should succeed");
        assert!(!first.already_existed);

        let second = DaemonState::register_project(&ds, &project_dir)
            .await
            .expect("second register should succeed");
        assert!(
            second.already_existed,
            "Re-registering the same path should return already_existed=true"
        );
        assert_eq!(
            first.workspace_id, second.workspace_id,
            "Re-registration should return the same workspace_id"
        );
    }

    #[tokio::test]
    async fn test_daemon_register_project_rejects_nonexistent_path() {
        let temp = tempfile::tempdir().unwrap();
        let julie_home = temp.path().join("julie-home");
        std::fs::create_dir_all(&julie_home).unwrap();

        let ds = test_daemon_state_with_home(julie_home);

        let bogus = temp.path().join("does-not-exist");
        let result = DaemonState::register_project(&ds, &bogus).await;
        assert!(
            result.is_err(),
            "Registering a nonexistent path should fail"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("does not exist"),
            "Error should mention 'does not exist'. Got: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_daemon_register_project_rejects_file_path() {
        let temp = tempfile::tempdir().unwrap();
        let julie_home = temp.path().join("julie-home");
        std::fs::create_dir_all(&julie_home).unwrap();

        let file_path = temp.path().join("not-a-dir.txt");
        std::fs::write(&file_path, "hello").unwrap();

        let ds = test_daemon_state_with_home(julie_home);

        let result = DaemonState::register_project(&ds, &file_path).await;
        assert!(result.is_err(), "Registering a file path should fail");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("not a directory"),
            "Error should mention 'not a directory'. Got: {}",
            err
        );
    }

    // =========================================================================
    // Deregistration tests
    // =========================================================================

    #[tokio::test]
    async fn test_daemon_deregister_project_removes_workspace() {
        let temp = tempfile::tempdir().unwrap();
        let julie_home = temp.path().join("julie-home");
        std::fs::create_dir_all(&julie_home).unwrap();

        let project_dir = temp.path().join("to-remove");
        std::fs::create_dir_all(&project_dir).unwrap();

        let ds = test_daemon_state_with_home(julie_home);

        // Register first
        let reg = DaemonState::register_project(&ds, &project_dir)
            .await
            .expect("register should succeed");
        let ws_id = reg.workspace_id.clone();

        // Verify it exists
        {
            let state = ds.read().await;
            assert!(state.workspaces.contains_key(&ws_id));
        }

        // Deregister
        let dereg = DaemonState::deregister_project(&ds, &ws_id)
            .await
            .expect("deregister should succeed");
        assert!(
            dereg.is_some(),
            "Deregistering an existing workspace should return Some"
        );

        let result = dereg.unwrap();
        assert_eq!(result.workspace_id, ws_id);
        assert_eq!(result.name, "to-remove");

        // Verify removed from both DaemonState.workspaces and GlobalRegistry
        let state = ds.read().await;
        assert!(
            !state.workspaces.contains_key(&ws_id),
            "Workspace should be removed from DaemonState.workspaces"
        );

        let registry = state.registry.read().await;
        assert!(
            registry.get_project(&ws_id).is_none(),
            "Workspace should be removed from GlobalRegistry"
        );
    }

    #[tokio::test]
    async fn test_daemon_deregister_nonexistent_returns_none() {
        let temp = tempfile::tempdir().unwrap();
        let julie_home = temp.path().join("julie-home");
        std::fs::create_dir_all(&julie_home).unwrap();

        let ds = test_daemon_state_with_home(julie_home);

        let result = DaemonState::deregister_project(&ds, "nonexistent_workspace_id")
            .await
            .expect("deregister of unknown ID should not error");

        assert!(
            result.is_none(),
            "Deregistering an unknown workspace_id should return None"
        );
    }

    // =========================================================================
    // Handler helper tests — error paths
    // =========================================================================

    #[tokio::test]
    async fn test_get_database_for_workspace_missing_workspace() {
        let temp = tempfile::tempdir().unwrap();
        let julie_home = temp.path().join("julie-home");
        std::fs::create_dir_all(&julie_home).unwrap();

        let ds = test_daemon_state_with_home(julie_home);
        let handler = daemon_handler(ds);

        let result = handler.get_database_for_workspace("nonexistent_ws").await;
        let err = match result {
            Err(e) => e.to_string(),
            Ok(_) => panic!("Should error for unknown workspace_id"),
        };
        assert!(
            err.contains("not found"),
            "Error should mention 'not found'. Got: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_get_database_for_workspace_not_ready() {
        // A workspace that exists but has no DB (status: Registered, empty_shell)
        let temp = tempfile::tempdir().unwrap();
        let julie_home = temp.path().join("julie-home");
        std::fs::create_dir_all(&julie_home).unwrap();

        let ds = test_daemon_state_with_home(julie_home);

        // Insert a workspace with no DB
        {
            let mut state = ds.write().await;
            state.workspaces.insert(
                "ws_no_db".to_string(),
                LoadedWorkspace {
                    workspace: JulieWorkspace::empty_shell(PathBuf::from("/tmp/ws_no_db")),
                    status: WorkspaceLoadStatus::Registered,
                    path: PathBuf::from("/tmp/ws_no_db"),
                },
            );
        }

        let handler = daemon_handler(ds);
        let result = handler.get_database_for_workspace("ws_no_db").await;
        let err = match result {
            Err(e) => e.to_string(),
            Ok(_) => panic!("Should error when workspace DB is None (not ready)"),
        };
        assert!(
            err.contains("not ready"),
            "Error should mention 'not ready'. Got: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_get_search_index_for_workspace_missing_workspace() {
        let temp = tempfile::tempdir().unwrap();
        let julie_home = temp.path().join("julie-home");
        std::fs::create_dir_all(&julie_home).unwrap();

        let ds = test_daemon_state_with_home(julie_home);
        let handler = daemon_handler(ds);

        let result = handler
            .get_search_index_for_workspace("nonexistent_ws")
            .await;
        let err = match result {
            Err(e) => e.to_string(),
            Ok(_) => panic!("Should error for unknown workspace_id"),
        };
        assert!(
            err.contains("not found"),
            "Error should mention 'not found'. Got: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_get_search_index_for_workspace_no_index() {
        // Workspace exists but has no search index (empty_shell)
        let temp = tempfile::tempdir().unwrap();
        let julie_home = temp.path().join("julie-home");
        std::fs::create_dir_all(&julie_home).unwrap();

        let ds = test_daemon_state_with_home(julie_home);

        {
            let mut state = ds.write().await;
            state.workspaces.insert(
                "ws_no_idx".to_string(),
                LoadedWorkspace {
                    workspace: JulieWorkspace::empty_shell(PathBuf::from("/tmp/ws_no_idx")),
                    status: WorkspaceLoadStatus::Registered,
                    path: PathBuf::from("/tmp/ws_no_idx"),
                },
            );
        }

        let handler = daemon_handler(ds);
        let result = handler
            .get_search_index_for_workspace("ws_no_idx")
            .await
            .expect("Should succeed even without index");
        assert!(
            result.is_none(),
            "Should return None when search_index is None"
        );
    }

    #[tokio::test]
    async fn test_get_workspace_root_for_target_missing_workspace() {
        let temp = tempfile::tempdir().unwrap();
        let julie_home = temp.path().join("julie-home");
        std::fs::create_dir_all(&julie_home).unwrap();

        let ds = test_daemon_state_with_home(julie_home);
        let handler = daemon_handler(ds);

        let result = handler
            .get_workspace_root_for_target("nonexistent_ws")
            .await;
        assert!(result.is_err(), "Should error for unknown workspace_id");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("not found"),
            "Error should mention 'not found'. Got: {}",
            err
        );
    }

    // =========================================================================
    // Handler helper tests — happy paths (daemon mode)
    // =========================================================================

    #[tokio::test]
    async fn test_get_database_for_workspace_daemon_mode() {
        use crate::database::SymbolDatabase;
        use std::sync::Mutex;

        let temp = tempfile::tempdir().unwrap();
        let julie_home = temp.path().join("julie-home");
        std::fs::create_dir_all(&julie_home).unwrap();

        // Create a real database
        let db_path = temp.path().join("symbols.db");
        let db = SymbolDatabase::new(&db_path).expect("should create test DB");

        // Build a workspace with a real DB
        let mut workspace = JulieWorkspace::empty_shell(temp.path().to_path_buf());
        workspace.db = Some(Arc::new(Mutex::new(db)));

        let ds = test_daemon_state_with_home(julie_home);
        {
            let mut state = ds.write().await;
            state.workspaces.insert(
                "ws_with_db".to_string(),
                LoadedWorkspace {
                    workspace,
                    status: WorkspaceLoadStatus::Ready,
                    path: temp.path().to_path_buf(),
                },
            );
        }

        let handler = daemon_handler(ds);
        let db_arc = handler
            .get_database_for_workspace("ws_with_db")
            .await
            .expect("Should return the workspace's DB");

        // Verify we got a usable DB connection
        let db_guard = db_arc.lock().unwrap();
        // The DB should be functional — check it doesn't panic
        drop(db_guard);
    }

    #[tokio::test]
    async fn test_get_workspace_root_for_target_daemon_mode() {
        let temp = tempfile::tempdir().unwrap();
        let julie_home = temp.path().join("julie-home");
        std::fs::create_dir_all(&julie_home).unwrap();

        let project_path = temp.path().join("my-project");
        std::fs::create_dir_all(&project_path).unwrap();

        let ds = test_daemon_state_with_home(julie_home);
        {
            let mut state = ds.write().await;
            state.workspaces.insert(
                "ws_root_test".to_string(),
                LoadedWorkspace {
                    workspace: JulieWorkspace::empty_shell(project_path.clone()),
                    status: WorkspaceLoadStatus::Ready,
                    path: project_path.clone(),
                },
            );
        }

        let handler = daemon_handler(ds);
        let root = handler
            .get_workspace_root_for_target("ws_root_test")
            .await
            .expect("Should return the workspace root");
        assert_eq!(
            root, project_path,
            "Should return the path from LoadedWorkspace"
        );
    }

    #[tokio::test]
    async fn test_get_search_index_for_workspace_daemon_mode() {
        use crate::search::index::SearchIndex;
        use std::sync::Mutex;

        let temp = tempfile::tempdir().unwrap();
        let julie_home = temp.path().join("julie-home");
        std::fs::create_dir_all(&julie_home).unwrap();

        // Create a real Tantivy index
        let tantivy_path = temp.path().join("tantivy");
        std::fs::create_dir_all(&tantivy_path).unwrap();
        let search_index =
            SearchIndex::create(&tantivy_path).expect("should create test search index");

        let mut workspace = JulieWorkspace::empty_shell(temp.path().to_path_buf());
        workspace.search_index = Some(Arc::new(Mutex::new(search_index)));

        let ds = test_daemon_state_with_home(julie_home);
        {
            let mut state = ds.write().await;
            state.workspaces.insert(
                "ws_with_idx".to_string(),
                LoadedWorkspace {
                    workspace,
                    status: WorkspaceLoadStatus::Ready,
                    path: temp.path().to_path_buf(),
                },
            );
        }

        let handler = daemon_handler(ds);
        let idx_opt = handler
            .get_search_index_for_workspace("ws_with_idx")
            .await
            .expect("Should return the workspace's search index");

        assert!(
            idx_opt.is_some(),
            "Should return Some when search_index is present"
        );
    }

    // =========================================================================
    // Registry persistence failure tests
    // =========================================================================

    #[cfg(unix)]
    #[tokio::test]
    async fn test_register_project_fails_when_registry_persist_fails() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let julie_home = temp.path().join("julie-home");
        std::fs::create_dir_all(&julie_home).unwrap();

        let project_dir = temp.path().join("persist-fail-project");
        std::fs::create_dir_all(&project_dir).unwrap();

        let ds = test_daemon_state_with_home(julie_home.clone());

        // Make julie_home read-only so registry.save() fails
        std::fs::set_permissions(&julie_home, std::fs::Permissions::from_mode(0o555)).unwrap();

        let result = DaemonState::register_project(&ds, &project_dir).await;

        // Restore permissions for cleanup
        std::fs::set_permissions(&julie_home, std::fs::Permissions::from_mode(0o755)).unwrap();

        assert!(
            result.is_err(),
            "register_project should fail when registry persist fails"
        );
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("registry") && err_msg.contains("write failed"),
            "Error should indicate registry write failure. Got: {}",
            err_msg
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_deregister_project_fails_when_registry_persist_fails() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let julie_home = temp.path().join("julie-home");
        std::fs::create_dir_all(&julie_home).unwrap();

        let project_dir = temp.path().join("dereg-persist-fail");
        std::fs::create_dir_all(&project_dir).unwrap();

        let ds = test_daemon_state_with_home(julie_home.clone());

        // Register the project first (this should succeed with writable julie_home)
        let reg = DaemonState::register_project(&ds, &project_dir)
            .await
            .expect("register should succeed");

        // Now make julie_home read-only so the deregister's persist fails
        std::fs::set_permissions(&julie_home, std::fs::Permissions::from_mode(0o555)).unwrap();

        let result = DaemonState::deregister_project(&ds, &reg.workspace_id).await;

        // Restore permissions for cleanup
        std::fs::set_permissions(&julie_home, std::fs::Permissions::from_mode(0o755)).unwrap();

        assert!(
            result.is_err(),
            "deregister_project should fail when registry persist fails"
        );
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("registry") && err_msg.contains("write failed"),
            "Error should indicate registry write failure. Got: {}",
            err_msg
        );
    }
}
