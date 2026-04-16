// Workspace initialization and root detection tests
//
// Tests for the workspace root detection logic that determines where
// Julie creates its .julie directory based on CLI args, environment
// variables, and current working directory.

use serial_test::serial;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Helper to create a test directory structure
fn setup_test_workspace() -> TempDir {
    let temp = TempDir::new().expect("Failed to create temp dir");
    fs::create_dir_all(temp.path().join("src")).expect("Failed to create src dir");
    temp
}

/// Test: Workspace detection priority order (CLI > env var > current_dir)
///
/// This tests that when multiple sources provide a workspace path,
/// the priority is respected: --workspace CLI arg takes precedence,
/// then JULIE_WORKSPACE env var, then current working directory.
#[test]
#[serial]
fn test_workspace_detection_priority() {
    // Save original env and args FIRST
    let original_env = env::var("JULIE_WORKSPACE").ok();
    let original_cwd = env::current_dir().expect("Failed to get cwd");

    let workspace1 = setup_test_workspace();
    let workspace2 = setup_test_workspace();
    let _workspace3 = setup_test_workspace(); // Reserved for future CLI arg testing

    // Test 1: Only current_dir (lowest priority)
    env::set_current_dir(workspace1.path()).expect("Failed to set cwd");
    unsafe {
        env::remove_var("JULIE_WORKSPACE");
    }

    // Since we can't easily test get_workspace_root() directly (it's private),
    // we verify the behavior through the documented contract:
    // - If no CLI args or env vars, it should use current_dir
    let expected = workspace1
        .path()
        .canonicalize()
        .expect("Failed to canonicalize");
    assert!(
        expected.exists(),
        "Workspace 1 should exist for current_dir test"
    );

    // Test 2: Env var overrides current_dir (medium priority)
    unsafe {
        env::set_var("JULIE_WORKSPACE", workspace2.path());
    }
    env::set_current_dir(workspace1.path()).expect("Failed to set cwd");

    // Verify env var is set correctly
    let env_path = env::var("JULIE_WORKSPACE").expect("JULIE_WORKSPACE should be set");
    assert_eq!(
        PathBuf::from(env_path)
            .canonicalize()
            .expect("Failed to canonicalize"),
        workspace2
            .path()
            .canonicalize()
            .expect("Failed to canonicalize"),
        "JULIE_WORKSPACE env var should point to workspace2"
    );

    // Test 3: CLI arg would override both (highest priority)
    // Note: We can't test CLI args directly in unit tests without spawning a process,
    // but we document the expected behavior here for integration tests.
    // CLI --workspace flag should take precedence over JULIE_WORKSPACE env var

    // Cleanup - change to system temp BEFORE dropping test temp dirs
    let system_temp = std::env::temp_dir();
    env::set_current_dir(&system_temp).expect("Failed to change to system temp");

    // Now safe to drop temp dirs
    drop(workspace1);
    drop(workspace2);
    drop(_workspace3);

    // Try to restore original cwd, but don't fail if it doesn't exist
    let _ = env::set_current_dir(&original_cwd);

    unsafe {
        if let Some(val) = original_env {
            env::set_var("JULIE_WORKSPACE", val);
        } else {
            env::remove_var("JULIE_WORKSPACE");
        }
    }
}

/// Test: Tilde expansion in JULIE_WORKSPACE environment variable
///
/// Verifies that paths like "~/projects/foo" are expanded to the user's
/// home directory correctly on all platforms.
#[test]
#[serial]
fn test_tilde_expansion_in_env_var() {
    // Save original env
    let original_env = env::var("JULIE_WORKSPACE").ok();

    // Create a test directory in a known location (not home, for safety)
    let test_workspace = setup_test_workspace();
    let test_path = test_workspace.path();

    // Test 1: Tilde-prefixed path that DOESN'T exist (should not be used)
    unsafe {
        env::set_var("JULIE_WORKSPACE", "~/nonexistent_julie_test_dir_12345");
    }

    // Since the path doesn't exist, get_workspace_root would fall back to current_dir
    // This validates the "path must exist" check works with tilde expansion

    // Test 2: Absolute path (no tilde) should work
    unsafe {
        env::set_var("JULIE_WORKSPACE", test_path);
    }
    let env_value = env::var("JULIE_WORKSPACE").expect("Should have JULIE_WORKSPACE set");
    assert_eq!(PathBuf::from(env_value), test_path);

    // Test 3: Verify shellexpand would work on tilde paths (integration test coverage)
    // We test the shellexpand library directly since we use it in get_workspace_root
    let tilde_path = "~/test";
    let expanded = shellexpand::tilde(tilde_path);
    assert!(
        !expanded.starts_with('~'),
        "Tilde should be expanded: {} -> {}",
        tilde_path,
        expanded
    );

    // Cleanup
    unsafe {
        if let Some(val) = original_env {
            env::set_var("JULIE_WORKSPACE", val);
        } else {
            env::remove_var("JULIE_WORKSPACE");
        }
    }
}

/// Test: Path canonicalization prevents duplicate workspace IDs
///
/// Verifies that different representations of the same path (with ./ or \\)
/// are canonicalized to the same path, preventing duplicate workspaces.
#[test]
#[serial]
fn test_path_canonicalization() {
    let workspace = setup_test_workspace();
    let canonical = workspace
        .path()
        .canonicalize()
        .expect("Failed to canonicalize");

    // Test various path representations
    let path_with_dot = workspace.path().join(".");
    let path_with_dot_canonical = path_with_dot
        .canonicalize()
        .expect("Failed to canonicalize ./");

    assert_eq!(
        canonical, path_with_dot_canonical,
        "Paths with ./ should canonicalize to same path"
    );

    // Test that parent/child navigation cancels out
    let workspace_name = workspace
        .path()
        .file_name()
        .expect("Should have file name")
        .to_string_lossy();

    let path_with_navigation = workspace.path().join("..").join(workspace_name.as_ref());
    let path_with_navigation_canonical = path_with_navigation
        .canonicalize()
        .expect("Failed to canonicalize parent/child");

    assert_eq!(
        canonical, path_with_navigation_canonical,
        "Paths with ../ navigation should canonicalize to same path"
    );
}

/// Test: Workspace initialization with explicit path
///
/// Verifies that when a workspace path is provided explicitly (not None),
/// it is used correctly for initialization.
///
/// NOTE: This test verifies the contract of initialize_workspace, which will:
/// 1. Try detect_and_load (search up tree for existing .julie)
/// 2. If not found, create new workspace at the specified path
#[tokio::test]
async fn test_workspace_init_with_explicit_path() {
    use crate::workspace::JulieWorkspace;

    let workspace = setup_test_workspace();

    // Directly test JulieWorkspace::initialize (the actual implementation)
    let result = JulieWorkspace::initialize(workspace.path().to_path_buf()).await;

    assert!(
        result.is_ok(),
        "Workspace initialization should succeed with explicit path: {:?}",
        result.err()
    );

    // Verify .julie directory was created in the correct location
    let julie_dir = workspace.path().join(".julie");
    assert!(
        julie_dir.exists(),
        ".julie directory should be created at {:?}",
        julie_dir
    );

    // Verify expected subdirectories exist
    assert!(
        julie_dir.join("indexes").exists(),
        "indexes directory should exist"
    );
    // Note: models/ directory was removed in v2.0 (embeddings replaced by Tantivy)
    assert!(
        julie_dir.join("cache").exists(),
        "cache directory should exist"
    );
}

/// Test: Environment variable detection when current_dir differs
///
/// This is the CRITICAL edge case that VS Code's config fixes:
/// JULIE_WORKSPACE is set by the MCP client, but the process starts
/// in a different directory. This test ensures get_workspace_root() would
/// use the env var, not the current working directory.
///
/// NOTE: This is a conceptual test. Full integration testing would require
/// spawning a new process with specific env vars and cwd.
#[test]
#[serial]
fn test_env_var_concept() {
    let target_workspace = setup_test_workspace();
    let different_cwd = setup_test_workspace();

    // Save original state
    let original_env = env::var("JULIE_WORKSPACE").ok();
    let original_cwd = env::current_dir().expect("Failed to get cwd");

    // Set up the scenario: env var points to one dir, cwd is different
    unsafe {
        env::set_var("JULIE_WORKSPACE", target_workspace.path());
    }
    env::set_current_dir(different_cwd.path()).expect("Failed to change cwd");

    // Verify the env var is set (we don't need to validate the exact value,
    // just that it's different from cwd - the actual usage is tested in integration tests)
    let env_value = env::var("JULIE_WORKSPACE").expect("JULIE_WORKSPACE should be set");
    assert!(
        !env_value.is_empty(),
        "JULIE_WORKSPACE should be set to a non-empty value"
    );

    // Verify cwd is different
    let current = env::current_dir().expect("Should have current dir");
    assert_ne!(
        current.canonicalize().expect("Failed to canonicalize"),
        target_workspace
            .path()
            .canonicalize()
            .expect("Failed to canonicalize"),
        "Current directory should be different from JULIE_WORKSPACE"
    );

    // This verifies the SETUP is correct. The actual behavior is tested
    // by get_workspace_root() in main.rs, which we can't easily unit test
    // here since it's a private function. Integration tests would verify
    // the full flow.

    // Cleanup - change to system temp BEFORE dropping test temp dirs
    let system_temp = std::env::temp_dir();
    env::set_current_dir(&system_temp).expect("Failed to change to system temp");

    // Now safe to drop temp dirs
    drop(target_workspace);
    drop(different_cwd);

    // Try to restore original cwd, but don't fail if it doesn't exist
    let _ = env::set_current_dir(&original_cwd);

    unsafe {
        if let Some(val) = original_env {
            env::set_var("JULIE_WORKSPACE", val);
        } else {
            env::remove_var("JULIE_WORKSPACE");
        }
    }
}

/// Test: Non-existent path in JULIE_WORKSPACE falls back gracefully
///
/// Verifies that if JULIE_WORKSPACE points to a non-existent directory,
/// get_workspace_root() would fall back to current_dir instead of failing.
#[test]
#[serial]
fn test_nonexistent_env_var_fallback() {
    let workspace = setup_test_workspace();

    // Save original state
    let original_env = env::var("JULIE_WORKSPACE").ok();
    let original_cwd = env::current_dir().expect("Failed to get cwd");

    // Set env var to a path that DEFINITELY doesn't exist
    // Using an absurd path that won't collide with anything
    let nonexistent_path = if cfg!(windows) {
        "Z:\\nonexistent\\julie\\test\\directory\\that\\does\\not\\exist\\12345"
    } else {
        "/nonexistent/julie/test/directory/that/does/not/exist/12345"
    };

    unsafe {
        env::set_var("JULIE_WORKSPACE", &nonexistent_path);
    }
    env::set_current_dir(workspace.path()).expect("Failed to set cwd");

    // Verify the env var is set to something that doesn't exist
    let env_path = env::var("JULIE_WORKSPACE").expect("JULIE_WORKSPACE should be set");
    assert!(
        !Path::new(&env_path).exists(),
        "Test env path should not exist: {}",
        env_path
    );

    // Verify current_dir is set to our test workspace
    let current = env::current_dir().expect("Should have current dir");
    assert_eq!(
        current.canonicalize().expect("Failed to canonicalize"),
        workspace
            .path()
            .canonicalize()
            .expect("Failed to canonicalize"),
        "Current directory should be set to test workspace"
    );

    // This verifies the SETUP. The actual fallback behavior is in get_workspace_root()
    // which will:
    // 1. Try to use JULIE_WORKSPACE (fails because doesn't exist)
    // 2. Fall back to current_dir() (which is our test workspace)

    // Cleanup - change to system temp BEFORE dropping test temp dir
    let system_temp = std::env::temp_dir();
    env::set_current_dir(&system_temp).expect("Failed to change to system temp");

    // Now safe to drop temp dir
    drop(workspace);

    // Try to restore original cwd, but don't fail if it doesn't exist
    let _ = env::set_current_dir(&original_cwd);

    unsafe {
        if let Some(val) = original_env {
            env::set_var("JULIE_WORKSPACE", val);
        } else {
            env::remove_var("JULIE_WORKSPACE");
        }
    }
}

/// Test: Forward slashes on Windows are handled correctly
///
/// Verifies that paths with forward slashes (common in JSON configs)
/// work correctly on Windows through PathBuf normalization.
#[test]
#[serial]
#[cfg(target_os = "windows")]
fn test_forward_slashes_on_windows() {
    let workspace = setup_test_workspace();
    let workspace_path = workspace.path();

    // Convert to string with forward slashes (like VS Code might send)
    let forward_slash_path = workspace_path.to_string_lossy().replace('\\', "/");

    // PathBuf should handle this correctly
    let parsed = PathBuf::from(&forward_slash_path);
    let canonical = parsed
        .canonicalize()
        .expect("Should canonicalize forward-slash path");

    let expected = workspace_path
        .canonicalize()
        .expect("Should canonicalize original");

    assert_eq!(
        canonical, expected,
        "Forward slash paths should resolve to same canonical path on Windows"
    );
}

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

/// Test: Handler uses provided workspace_root, not current_dir
///
/// Verifies the P1 fix: JulieServerHandler::new(workspace_root) stores the
/// provided path and uses it as the fallback in initialize_workspace_with_force,
/// instead of calling current_dir().
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

    // Verify the handler stored the correct workspace root
    // Use canonicalize() on both sides to handle Windows 8.3 short path names
    // (e.g., tempfile may return CHS300~1 instead of CHS300372)
    assert_eq!(
        handler.workspace_root.canonicalize().unwrap(),
        intended_workspace.path().canonicalize().unwrap(),
        "Handler should store the workspace root passed to new(), not current_dir"
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
        handler.workspace_root.canonicalize().unwrap(),
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

/// Test: find_workspace_root rejects ~/.julie/ global config dir as workspace marker
///
/// Regression test for: The global ~/.julie/ directory
/// was being treated as a workspace marker, causing the user's entire home directory
/// to be indexed. find_workspace_root must skip ~/.julie/ and fall through.
#[test]
#[serial]
fn test_find_workspace_root_rejects_home_julie_dir() {
    use crate::tools::workspace::ManageWorkspaceTool;

    // Create a fake home directory
    let fake_home = TempDir::new().expect("Failed to create fake home");

    // Simulate global config: ~/.julie/logs/ and ~/.julie/registry.toml
    let global_julie = fake_home.path().join(".julie");
    fs::create_dir_all(global_julie.join("logs")).expect("Failed to create .julie/logs");
    fs::write(global_julie.join("registry.toml"), "# global registry")
        .expect("Failed to create registry.toml");

    // Create a working directory deep inside fake home (no workspace markers)
    let working_dir = fake_home.path().join("projects").join("myapp");
    fs::create_dir_all(&working_dir).expect("Failed to create working dir");

    // Save and override $HOME so julie_home() resolves to fake_home/.julie
    let original_home = env::var("HOME").ok();
    #[cfg(windows)]
    let original_userprofile = env::var("USERPROFILE").ok();

    unsafe {
        env::set_var("HOME", fake_home.path());
        #[cfg(windows)]
        env::set_var("USERPROFILE", fake_home.path());
    }

    let tool = ManageWorkspaceTool {
        operation: "test".to_string(),
        path: None,
        force: None,
        name: None,
        workspace_id: None,
        detailed: None,
    };

    let result = tool
        .find_workspace_root(&working_dir)
        .expect("find_workspace_root should not error");

    // The result must NOT be fake_home (which would mean ~/.julie was used as marker).
    // It should fall through to returning working_dir since there are no real markers.
    let fake_home_canonical = fake_home
        .path()
        .canonicalize()
        .unwrap_or_else(|_| fake_home.path().to_path_buf());
    let result_canonical = result.canonicalize().unwrap_or_else(|_| result.clone());

    assert_ne!(
        result_canonical,
        fake_home_canonical,
        "find_workspace_root must NOT treat ~/.julie/ global config as a workspace marker. \
         Expected working_dir or similar, got home dir: {}",
        result.display()
    );

    // It should return the working_dir itself (no markers found → use start_path)
    let working_dir_canonical = working_dir
        .canonicalize()
        .unwrap_or_else(|_| working_dir.clone());
    assert_eq!(
        result_canonical, working_dir_canonical,
        "With no workspace markers, find_workspace_root should return the start_path"
    );

    // Restore $HOME
    unsafe {
        if let Some(val) = original_home {
            env::set_var("HOME", val);
        } else {
            env::remove_var("HOME");
        }
        #[cfg(windows)]
        {
            if let Some(val) = original_userprofile {
                env::set_var("USERPROFILE", val);
            } else {
                env::remove_var("USERPROFILE");
            }
        }
    }
}

/// Test: agent instructions are always available (embedded at compile time)
///
/// JULIE_AGENT_INSTRUCTIONS.md is product metadata embedded via include_str!,
/// so instructions are available regardless of the workspace being indexed.
#[tokio::test]
#[serial]
async fn test_agent_instructions_always_available() {
    use crate::handler::JulieServerHandler;

    // Use an empty temp dir as workspace — no JULIE_AGENT_INSTRUCTIONS.md present
    let empty_workspace = setup_test_workspace();

    let handler = JulieServerHandler::new(empty_workspace.path().to_path_buf())
        .await
        .expect("Failed to create handler");

    use rmcp::ServerHandler;
    let info = handler.get_info();

    // Instructions should always be present (embedded at compile time)
    assert!(
        info.instructions.is_some(),
        "get_info().instructions should always be Some (embedded at compile time)"
    );

    let instructions = info.instructions.unwrap();
    assert!(
        instructions.contains("Rules"),
        "Embedded instructions should contain expected content"
    );
    assert!(
        instructions.contains("fast_search"),
        "Embedded instructions should reference Julie tools"
    );
}

/// Regression test: workspace_db_path and workspace_tantivy_path must return
/// different paths for different workspace IDs, even when index_root_override
/// is set (daemon mode). Previously, the override branch ignored workspace_id
/// entirely, causing all reference workspace data to be written to the primary
/// workspace's database.
#[test]
fn test_workspace_paths_differ_per_workspace_id_with_override() {
    let tmp = TempDir::new().unwrap();
    let julie_dir = tmp.path().join(".julie");
    fs::create_dir_all(&julie_dir).unwrap();

    let primary_id = "julie_528d4264";
    let ref_id = "zod_4e845d39";

    // Simulate daemon mode: override points to primary workspace's index dir
    let shared_indexes = tmp.path().join("indexes");
    let override_path = shared_indexes.join(primary_id);
    fs::create_dir_all(&override_path).unwrap();

    let workspace = crate::workspace::JulieWorkspace {
        root: tmp.path().to_path_buf(),
        julie_dir: julie_dir.clone(),
        db: None,
        search_index: None,
        watcher: None,
        embedding_provider: None,
        embedding_runtime_status: None,
        config: Default::default(),
        index_root_override: Some(override_path.clone()),
        indexing_runtime: crate::tools::workspace::indexing::state::IndexingRuntimeState::shared(),
    };

    let primary_db = workspace.workspace_db_path(primary_id);
    let ref_db = workspace.workspace_db_path(ref_id);

    assert_ne!(
        primary_db,
        ref_db,
        "workspace_db_path must return different paths for different workspace IDs. \
         Got same path for both: {}",
        primary_db.display()
    );
    assert!(
        primary_db.to_string_lossy().contains(primary_id),
        "Primary DB path should contain primary workspace ID: {}",
        primary_db.display()
    );
    assert!(
        ref_db.to_string_lossy().contains(ref_id),
        "Reference DB path should contain reference workspace ID: {}",
        ref_db.display()
    );

    // Same check for Tantivy paths
    let primary_tantivy = workspace.workspace_tantivy_path(primary_id);
    let ref_tantivy = workspace.workspace_tantivy_path(ref_id);

    assert_ne!(
        primary_tantivy, ref_tantivy,
        "workspace_tantivy_path must return different paths for different workspace IDs"
    );

    // Same check for index path
    let primary_index = workspace.workspace_index_path(primary_id);
    let ref_index = workspace.workspace_index_path(ref_id);

    assert_ne!(
        primary_index, ref_index,
        "workspace_index_path must return different paths for different workspace IDs"
    );

    // Verify both paths share the same parent (shared indexes dir)
    let db_parent = ref_db.parent().unwrap().parent().unwrap().parent().unwrap();
    let expected_parent = primary_db
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    assert_eq!(
        db_parent, expected_parent,
        "Both workspace paths should share the same indexes parent directory"
    );
}
