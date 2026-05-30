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

/// Test (Function #2, discovery walk): the upward `.julie` discovery walk in
/// `JulieWorkspace::find_workspace_root` must stop at ANY VCS repository root
/// (here `.hg`), not just `.git`, so it cannot climb past a non-git project
/// root into a stray ancestor `.julie/` left in a parent/temp directory.
///
/// RED before fix: only `.git` was treated as a boundary, so a `.hg`-rooted
/// project with a stray ancestor `.julie/` would wrongly capture that ancestor.
#[test]
fn test_workspace_root_stops_at_hg_boundary_not_stray_ancestor_julie() {
    use crate::workspace::JulieWorkspace;

    // Stray ancestor .julie ABOVE a non-git (Mercurial) project root.
    let stray_root = TempDir::new().expect("Failed to create stray root");
    fs::create_dir_all(stray_root.path().join(".julie").join("indexes"))
        .expect("Failed to create stray .julie");

    // Mercurial project root with NO .julie of its own.
    let repo = stray_root.path().join("repo");
    fs::create_dir_all(repo.join(".hg")).expect("Failed to create .hg root");
    let start = repo.join("src");
    fs::create_dir_all(&start).expect("Failed to create start subdir");

    let result =
        JulieWorkspace::find_workspace_root(&start).expect("find_workspace_root should not error");

    assert_eq!(
        result, None,
        "discovery walk must stop at the .hg VCS-root boundary and return None (so the caller \
         creates repo/.julie), not climb to the stray ancestor .julie; got: {:?}",
        result
    );
}

/// Test (Function #2, monorepo regression guard — must pass BEFORE and AFTER the
/// fix): a Cargo workspace member crate with its OWN `Cargo.toml` and an ancestor
/// workspace-root `.julie/` must still resolve to the workspace-root `.julie/`.
/// This guards against the wrong fix of adding nesting build manifests
/// (`Cargo.toml`/`package.json`) as boundaries, which would halt the walk inside
/// the member crate and split the index.
#[test]
fn test_workspace_root_member_crate_resolves_to_workspace_julie_not_own_cargo_toml() {
    use crate::workspace::JulieWorkspace;

    // Cargo workspace root: real .julie + .git VCS root + workspace manifest.
    let ws = TempDir::new().expect("Failed to create workspace root");
    fs::create_dir_all(ws.path().join(".julie").join("indexes"))
        .expect("Failed to create workspace .julie");
    fs::create_dir_all(ws.path().join(".git")).expect("Failed to create workspace .git");
    fs::write(
        ws.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/foo\"]\n",
    )
    .expect("Failed to write workspace Cargo.toml");

    // Member crate with its OWN Cargo.toml and no .julie of its own.
    let member = ws.path().join("crates").join("foo");
    fs::create_dir_all(member.join("src")).expect("Failed to create member crate src");
    fs::write(member.join("Cargo.toml"), "[package]\nname = \"foo\"\n")
        .expect("Failed to write member Cargo.toml");

    let start = member.join("src");

    let julie_dir = JulieWorkspace::find_workspace_root(&start)
        .expect("find_workspace_root should not error")
        .expect("walk must reach the workspace-root .julie, not stop at the member crate");

    let resolved_root = julie_dir.parent().expect(".julie dir must have a parent");
    assert_eq!(
        resolved_root
            .canonicalize()
            .unwrap_or_else(|_| resolved_root.to_path_buf()),
        ws.path()
            .canonicalize()
            .unwrap_or_else(|_| ws.path().to_path_buf()),
        "a member crate's own Cargo.toml must NOT be a boundary; the discovery walk must climb \
         to the workspace-root .julie"
    );
}

/// Test (Function #2, home-guard regression guard — must pass BEFORE and AFTER):
/// the `~/.julie` global-home guard must still fire under the new multi-VCS
/// boundary logic. A working dir under a fake home with no VCS root and no real
/// `.julie/` must NOT capture the global home `.julie/`.
#[test]
#[serial]
fn test_workspace_root_still_rejects_home_julie_under_new_vcs_markers() {
    use crate::workspace::JulieWorkspace;

    let fake_home = TempDir::new().expect("Failed to create fake home");
    let global_julie = fake_home.path().join(".julie");
    fs::create_dir_all(global_julie.join("logs")).expect("Failed to create .julie/logs");
    fs::write(global_julie.join("registry.toml"), "# global registry")
        .expect("Failed to create registry.toml");

    let working_dir = fake_home.path().join("projects").join("myapp");
    fs::create_dir_all(&working_dir).expect("Failed to create working dir");

    let original_home = env::var("HOME").ok();
    #[cfg(windows)]
    let original_userprofile = env::var("USERPROFILE").ok();
    unsafe {
        env::set_var("HOME", fake_home.path());
        #[cfg(windows)]
        env::set_var("USERPROFILE", fake_home.path());
    }

    let result = JulieWorkspace::find_workspace_root(&working_dir)
        .expect("find_workspace_root should not error");

    // Restore env BEFORE asserting so a panic can't leak the override.
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

    assert_eq!(
        result, None,
        "the ~/.julie home guard must still skip the global home .julie under the new VCS-marker \
         boundary logic; with no VCS root and no non-home .julie the walk must return None, got: {:?}",
        result
    );
}

/// Test (Function #1, explicit-path resolver — cross-VCS breadth, RED before fix):
/// `ManageWorkspaceTool::find_workspace_root` must recognize a `.hg` (and other
/// VCS-root) directory as a workspace-root marker, not just `.git`. Before the
/// fix the marker set only had `.git` among VCS roots, so a Mercurial-only project
/// fell through to returning the start_path.
#[test]
fn test_paths_find_workspace_root_recognizes_hg_root() {
    use crate::tools::workspace::ManageWorkspaceTool;

    let hg_root = TempDir::new().expect("Failed to create hg root");
    fs::create_dir_all(hg_root.path().join(".hg")).expect("Failed to create .hg");
    let sub = hg_root.path().join("sub");
    fs::create_dir_all(&sub).expect("Failed to create sub");

    let tool = ManageWorkspaceTool {
        operation: "test".to_string(),
        path: None,
        force: None,
        name: None,
        workspace_id: None,
        detailed: None,
    };

    let result = tool
        .find_workspace_root(&sub)
        .expect("find_workspace_root should not error");

    assert_eq!(
        result.canonicalize().unwrap_or_else(|_| result.clone()),
        hg_root
            .path()
            .canonicalize()
            .unwrap_or_else(|_| hg_root.path().to_path_buf()),
        "Function #1 must recognize a .hg directory as a VCS workspace-root marker, \
         stopping at the Mercurial root instead of falling through to the start_path"
    );
}
