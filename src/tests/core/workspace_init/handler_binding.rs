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
async fn test_loaded_workspace_swap_intent_invalidates_loaded_and_current_state_before_async_gap() {
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

    let second_root = second_workspace.path().canonicalize().unwrap();
    let _second_id =
        crate::workspace::registry::generate_workspace_id(&second_root.to_string_lossy())
            .expect("Should generate second workspace id");

    handler.publish_loaded_workspace_swap_intent_for_test();

    assert_eq!(
        handler.loaded_workspace_id(),
        None,
        "loaded workspace id should be cleared before the async swap gap"
    );
    assert_eq!(
        handler.current_workspace_id(),
        None,
        "current primary binding should be cleared during the async swap gap"
    );
    assert_eq!(
        handler.current_workspace_root().canonicalize().unwrap(),
        handler
            .workspace_startup_hint()
            .path
            .canonicalize()
            .unwrap(),
        "current workspace root should fall back to startup hint during the async swap gap"
    );
}

#[tokio::test]
#[serial]
async fn test_neutral_gap_primary_workspace_storage_anchor_fails_without_primary_identity() {
    use crate::handler::JulieServerHandler;

    let workspace = setup_test_workspace();
    fs::create_dir_all(workspace.path().join(".git")).unwrap();

    let handler = JulieServerHandler::new_for_test()
        .await
        .expect("Failed to create handler");
    handler
        .initialize_workspace_with_force(Some(workspace.path().to_string_lossy().to_string()), true)
        .await
        .expect("Failed to initialize workspace");

    handler.publish_loaded_workspace_swap_intent_for_test();

    let err = handler
        .workspace_storage_anchor()
        .await
        .expect_err("neutral gap should not expose stale loaded workspace storage");

    assert!(
        err.to_string()
            .contains("Primary workspace identity unavailable during swap"),
        "unexpected error: {err:#}"
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

#[tokio::test]
#[serial]
async fn test_root_swap_failure_restores_previous_loaded_workspace_state() {
    use crate::handler::JulieServerHandler;

    unsafe {
        env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }

    let first_workspace = setup_test_workspace();
    fs::create_dir_all(first_workspace.path().join(".git")).unwrap();

    let failing_target_parent = TempDir::new().unwrap();
    let failing_target = failing_target_parent.path().join("not-a-directory");
    fs::write(&failing_target, "swap should fail here").unwrap();

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

    let err = handler
        .initialize_workspace_with_force(Some(failing_target.to_string_lossy().to_string()), true)
        .await
        .expect_err("root swap to file path should fail after swap intent");

    assert!(
        !handler.is_primary_workspace_swap_in_progress(),
        "failed swap should clear swap-in-progress state"
    );
    assert_eq!(
        handler.loaded_workspace_id(),
        Some(first_id.clone()),
        "failed swap should restore the previous loaded workspace id"
    );
    assert_eq!(
        handler.current_workspace_id(),
        Some(first_id.clone()),
        "failed swap should restore the previous current workspace id"
    );
    assert_eq!(
        handler.current_workspace_root().canonicalize().unwrap(),
        first_root,
        "failed swap should restore the previous current workspace root"
    );

    let (anchor_root, anchor_override) = handler
        .workspace_storage_anchor()
        .await
        .expect("session should remain usable after failed swap");
    assert_eq!(
        anchor_root.canonicalize().unwrap(),
        first_workspace.path().canonicalize().unwrap()
    );
    assert_eq!(anchor_override, None);
    assert!(
        handler
            .loaded_workspace_file_watcher_running_for_test()
            .await,
        "failed swap should restore file watching for the restored stdio workspace"
    );

    assert!(
        err.to_string().contains("Not a directory") || err.to_string().contains("not a directory"),
        "unexpected swap failure: {err:#}"
    );
}

#[tokio::test]
#[serial]
async fn test_non_force_root_swap_tears_down_old_search_index() {
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

    let old_workspace = handler
        .get_workspace()
        .await
        .expect("workspace lookup should succeed")
        .expect("first workspace should be loaded");
    let old_search_index = old_workspace
        .search_index
        .as_ref()
        .expect("first workspace search index should exist")
        .clone();

    handler
        .initialize_workspace_with_force(
            Some(second_workspace.path().to_string_lossy().to_string()),
            false,
        )
        .await
        .expect("Failed to initialize second workspace");

    let current_workspace = handler
        .get_workspace()
        .await
        .expect("workspace lookup should succeed")
        .expect("second workspace should be loaded");
    assert_eq!(
        current_workspace.root.canonicalize().unwrap(),
        second_workspace.path().canonicalize().unwrap(),
        "non-force root swap should replace the loaded workspace"
    );

    let old_search_index = old_search_index.lock().unwrap();
    assert!(
        old_search_index.is_shutdown(),
        "non-force root swap should shut down the old search index before replacement"
    );
}
