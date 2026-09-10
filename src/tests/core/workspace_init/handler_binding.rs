use super::*;

/// Test: Handler uses provided workspace_root, not current_dir
///
/// Verifies the P1 fix: JulieServerHandler::new(workspace_root) stores the
/// provided path in session startup state and uses it as the fallback in
/// initialize_workspace_with_force, instead of calling current_dir().
///
/// This prevents the bug where `julie-server --workspace /repo` launched from
/// a different directory would index the wrong path.
#[tokio::test]
#[serial]
async fn test_handler_uses_provided_workspace_root() {
    use crate::handler::JulieServerHandler;

    let intended_workspace = setup_test_workspace();
    let different_cwd = setup_test_workspace();

    // Create .git boundaries so find_workspace_root doesn't walk up into ~/.julie
    fs::create_dir_all(intended_workspace.path().join(".git")).unwrap();
    fs::create_dir_all(different_cwd.path().join(".git")).unwrap();

    // Save original cwd
    let original_cwd = env::current_dir().expect("Failed to get cwd");

    // Set cwd to a DIFFERENT directory than the workspace root we'll pass
    env::set_current_dir(different_cwd.path()).expect("Failed to change cwd");

    // Create handler with explicit workspace root (NOT current_dir)
    let handler = JulieServerHandler::new(intended_workspace.path().to_path_buf())
        .await
        .expect("Failed to create handler");

    // Verify the session startup root is the explicit workspace root.
    // Use canonicalize() on both sides to handle Windows 8.3 short path names
    // (e.g., tempfile may return CHS300~1 instead of CHS300372)
    assert_eq!(
        handler.current_workspace_root().canonicalize().unwrap(),
        intended_workspace.path().canonicalize().unwrap(),
        "Handler should seed the session root from new(), not current_dir"
    );

    // Verify current_dir is different (precondition check)
    let cwd = env::current_dir().expect("Failed to get cwd");
    assert_ne!(
        cwd.canonicalize().unwrap(),
        intended_workspace.path().canonicalize().unwrap(),
        "Test precondition: cwd should differ from intended workspace"
    );

    // Initialize workspace with None path - should use workspace_root as fallback
    let result = handler.initialize_workspace(None).await;
    assert!(
        result.is_ok(),
        "initialize_workspace(None) should succeed using workspace_root: {:?}",
        result.err()
    );

    // Verify the workspace was initialized at the intended path, not cwd
    let workspace = handler
        .get_workspace()
        .await
        .expect("Failed to get workspace")
        .expect("Workspace should be initialized");
    assert_eq!(
        workspace.root.canonicalize().unwrap(),
        intended_workspace.path().canonicalize().unwrap(),
        "Workspace should be initialized at the intended root, not cwd"
    );

    // Cleanup
    let system_temp = std::env::temp_dir();
    env::set_current_dir(&system_temp).expect("Failed to change to system temp");
    drop(intended_workspace);
    drop(different_cwd);
    let _ = env::set_current_dir(&original_cwd);
}

#[tokio::test]
#[serial]
async fn test_handler_uses_provided_workspace_root_updates_session_current_primary() {
    use crate::handler::JulieServerHandler;

    let intended_workspace = setup_test_workspace();
    fs::create_dir_all(intended_workspace.path().join(".git")).unwrap();

    let handler = JulieServerHandler::new_for_test()
        .await
        .expect("Failed to create handler");

    handler
        .initialize_workspace_with_force(
            Some(intended_workspace.path().to_string_lossy().to_string()),
            true,
        )
        .await
        .expect("Failed to initialize workspace with explicit path");

    let expected_root = intended_workspace.path().canonicalize().unwrap();
    let expected_id =
        crate::workspace::registry::generate_workspace_id(&expected_root.to_string_lossy())
            .expect("Should generate workspace id");

    assert_eq!(
        handler.current_workspace_root().canonicalize().unwrap(),
        expected_root,
        "explicit stdio initialization should update session current root"
    );
    assert_eq!(
        handler.current_workspace_id(),
        Some(expected_id),
        "explicit stdio initialization should seed session current workspace id"
    );
}

#[tokio::test]
#[serial]
async fn test_loaded_workspace_id_tracks_rebound_workspace() {
    use crate::handler::JulieServerHandler;

    let first_workspace = setup_test_workspace();
    let second_workspace = setup_test_workspace();
    fs::create_dir_all(first_workspace.path().join(".git")).unwrap();
    fs::create_dir_all(second_workspace.path().join(".git")).unwrap();

    let handler = JulieServerHandler::new_for_test()
        .await
        .expect("Failed to create handler");

    handler
        .initialize_workspace_with_force(
            Some(first_workspace.path().to_string_lossy().to_string()),
            true,
        )
        .await
        .expect("Failed to initialize first workspace");

    let first_root = first_workspace.path().canonicalize().unwrap();
    let first_id = crate::workspace::registry::generate_workspace_id(&first_root.to_string_lossy())
        .expect("Should generate first workspace id");
    assert_eq!(handler.loaded_workspace_id(), Some(first_id));

    handler
        .initialize_workspace_with_force(
            Some(second_workspace.path().to_string_lossy().to_string()),
            false,
        )
        .await
        .expect("Failed to rebind to second workspace");

    let second_root = second_workspace.path().canonicalize().unwrap();
    let second_id =
        crate::workspace::registry::generate_workspace_id(&second_root.to_string_lossy())
            .expect("Should generate second workspace id");

    assert_eq!(
        handler.loaded_workspace_id(),
        Some(second_id.clone()),
        "loaded workspace id should track the workspace stored in handler.workspace"
    );
    assert_eq!(
        handler.current_workspace_id(),
        Some(second_id),
        "current workspace id should still track session current-primary state"
    );
}
