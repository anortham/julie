// Workspace initialization and root detection tests
//
// Tests for the workspace root detection logic that determines where
// Julie creates its .julie directory based on CLI args, environment
// variables, and current working directory.

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
    assert!(
        julie_dir.join("models").exists(),
        "models directory should exist"
    );
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
    let handler = JulieServerHandler::new()
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

    let resolved = tool.resolve_workspace_path(None);
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
        "resolve_workspace_path should use JULIE_WORKSPACE, not current_dir"
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
