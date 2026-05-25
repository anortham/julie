use super::*;

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

#[test]
#[serial]
fn test_find_workspace_root_does_not_let_parent_julie_capture_unmarked_explicit_dir() {
    use crate::tools::workspace::ManageWorkspaceTool;

    let parent_workspace = TempDir::new().expect("Failed to create parent workspace");
    fs::create_dir_all(parent_workspace.path().join(".julie").join("indexes"))
        .expect("Failed to create parent .julie");

    let explicit_workspace = parent_workspace
        .path()
        .join("julie-dogfood-rewrite.fixture");
    fs::create_dir_all(explicit_workspace.join("src"))
        .expect("Failed to create explicit workspace");

    let tool = ManageWorkspaceTool {
        operation: "test".to_string(),
        path: None,
        force: None,
        name: None,
        workspace_id: None,
        detailed: None,
    };

    let result = tool
        .find_workspace_root(&explicit_workspace)
        .expect("find_workspace_root should not error");

    assert_eq!(
        result.canonicalize().unwrap_or_else(|_| result.clone()),
        explicit_workspace
            .canonicalize()
            .unwrap_or_else(|_| explicit_workspace.clone()),
        "a parent .julie directory must not capture an explicit unmarked child workspace"
    );
}
