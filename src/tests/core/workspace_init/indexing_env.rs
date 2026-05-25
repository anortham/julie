use super::*;

/// Test: Incremental indexing respects JULIE_WORKSPACE env var
///
/// Regression test for: Incremental indexing (via ManageWorkspaceTool) should respect
/// JULIE_WORKSPACE just like startup indexing does.
///
/// This test ensures that when a user manually triggers indexing or when incremental
/// updates occur, the JULIE_WORKSPACE environment variable is respected even when the
/// current working directory is different.
#[tokio::test(flavor = "multi_thread")]
#[serial]
async fn test_incremental_indexing_respects_env_var() {
    use crate::handler::JulieServerHandler;
    use crate::tools::workspace::ManageWorkspaceTool;

    let target_workspace = setup_test_workspace();
    let different_cwd = setup_test_workspace();

    // Save original state
    let original_env = env::var("JULIE_WORKSPACE").ok();
    let original_cwd = env::current_dir().expect("Failed to get cwd");

    // Set up scenario: JULIE_WORKSPACE points to one dir, cwd is different
    unsafe {
        env::set_var("JULIE_WORKSPACE", target_workspace.path());
    }
    env::set_current_dir(different_cwd.path()).expect("Failed to change cwd");

    // Initialize handler and workspace
    let handler = JulieServerHandler::new_for_test()
        .await
        .expect("Failed to create handler");
    handler
        .initialize_workspace_with_force(
            Some(target_workspace.path().to_string_lossy().to_string()),
            true,
        )
        .await
        .expect("Failed to initialize workspace");

    // Create ManageWorkspaceTool with path=None (should use JULIE_WORKSPACE)
    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: None, // This should use JULIE_WORKSPACE, not current_dir!
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };

    // Call the tool - this should index target_workspace, not different_cwd
    let result = index_tool.call_tool(&handler).await;

    // The operation should succeed (indexing the correct workspace)
    assert!(
        result.is_ok(),
        "Incremental indexing should succeed with JULIE_WORKSPACE: {:?}",
        result.err()
    );

    // Cleanup - change to system temp BEFORE dropping test temp dirs
    let system_temp = std::env::temp_dir();
    env::set_current_dir(&system_temp).expect("Failed to change to system temp");

    drop(target_workspace);
    drop(different_cwd);

    // Restore original state
    let _ = env::set_current_dir(&original_cwd);
    unsafe {
        if let Some(val) = original_env {
            env::set_var("JULIE_WORKSPACE", val);
        } else {
            env::remove_var("JULIE_WORKSPACE");
        }
    }
}

/// Test: resolve_workspace_path respects JULIE_WORKSPACE
///
/// Unit test for: The resolve_workspace_path function should check JULIE_WORKSPACE
/// environment variable before falling back to current_dir().
#[test]
#[serial]
fn test_resolve_workspace_path_respects_env_var() {
    use crate::tools::workspace::ManageWorkspaceTool;

    let target_workspace = setup_test_workspace();
    let different_cwd = setup_test_workspace();

    // Create workspace markers so find_workspace_root doesn't walk up past them
    // Use .git as it's the first marker checked in find_workspace_root
    fs::create_dir_all(target_workspace.path().join(".git")).expect("Failed to create .git");
    fs::create_dir_all(different_cwd.path().join(".git")).expect("Failed to create .git");

    // Save original state
    let original_env = env::var("JULIE_WORKSPACE").ok();
    let original_cwd = env::current_dir().expect("Failed to get cwd");

    // Set up scenario: JULIE_WORKSPACE points to one dir, cwd is different
    unsafe {
        env::set_var("JULIE_WORKSPACE", target_workspace.path());
    }
    env::set_current_dir(different_cwd.path()).expect("Failed to change cwd");

    // Create tool and resolve path with None
    let tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: None,
        force: None,
        name: None,
        workspace_id: None,
        detailed: None,
    };

    // Without handler_root, env var should be used (third priority)
    let resolved = tool.resolve_workspace_path(None, None);
    assert!(
        resolved.is_ok(),
        "resolve_workspace_path should succeed: {:?}",
        resolved.err()
    );

    let resolved_path = resolved.unwrap();
    let expected = target_workspace
        .path()
        .canonicalize()
        .expect("Failed to canonicalize");

    // The resolved path should be the target workspace, not current_dir
    assert_eq!(
        resolved_path, expected,
        "resolve_workspace_path should use JULIE_WORKSPACE when no handler_root, not current_dir"
    );

    // With handler_root, it should take priority over env var
    let handler_root_dir = setup_test_workspace();
    fs::create_dir_all(handler_root_dir.path().join(".git")).expect("Failed to create .git");
    let resolved_with_root = tool.resolve_workspace_path(None, Some(handler_root_dir.path()));
    assert!(resolved_with_root.is_ok());
    assert_eq!(
        resolved_with_root.unwrap(),
        handler_root_dir.path().to_path_buf(),
        "resolve_workspace_path should prefer handler_root over JULIE_WORKSPACE"
    );

    // Cleanup
    let system_temp = std::env::temp_dir();
    env::set_current_dir(&system_temp).expect("Failed to change to system temp");

    drop(target_workspace);
    drop(different_cwd);

    let _ = env::set_current_dir(&original_cwd);
    unsafe {
        if let Some(val) = original_env {
            env::set_var("JULIE_WORKSPACE", val);
        } else {
            env::remove_var("JULIE_WORKSPACE");
        }
    }
}
