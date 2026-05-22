// Tests for the shared walker builder (src/utils/walk.rs)
//
// Verifies that build_walker correctly handles:
// - .gitignore parsing (including nested)
// - .julieignore support
// - BLACKLISTED_DIRECTORIES filtering
// - .git exclusion (always, even with hidden(false))
// - Dotfile inclusion (e.g., .editorconfig)
// - Non-git workspaces

use crate::utils::walk::{WalkConfig, build_walker, try_build_single_path_walker};
use std::fs;
use tempfile::TempDir;

fn collect_walked_files(root: &std::path::Path, config: &WalkConfig) -> Vec<String> {
    build_walker(root, config)
        .filter_map(|r| r.ok())
        .filter(|e| e.file_type().map_or(false, |ft| ft.is_file()))
        .map(|e| {
            e.path()
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect()
}

fn single_path_walker_includes(
    root: &std::path::Path,
    path: &std::path::Path,
    config: &WalkConfig,
) -> bool {
    try_build_single_path_walker(root, path, config)
        .unwrap()
        .filter_map(|r| r.ok())
        .any(|entry| entry.path() == path)
}

#[test]
fn test_walk_respects_gitignore() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join(".git")).unwrap();
    fs::write(root.join(".gitignore"), "ignored_dir/\n").unwrap();
    fs::create_dir_all(root.join("ignored_dir")).unwrap();
    fs::write(root.join("ignored_dir/file.rs"), "// ignored").unwrap();
    fs::write(root.join("kept.rs"), "// kept").unwrap();

    let files = collect_walked_files(root, &WalkConfig::full_index());
    assert!(files.iter().any(|f| f.contains("kept.rs")));
    assert!(
        !files.iter().any(|f| f.contains("ignored_dir")),
        "gitignored directory should be excluded"
    );
}

#[test]
fn test_walk_respects_julieignore() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join(".git")).unwrap();
    fs::write(root.join(".julieignore"), "vendor/\n").unwrap();
    fs::create_dir_all(root.join("vendor")).unwrap();
    fs::write(root.join("vendor/lib.js"), "// vendor").unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/app.rs"), "fn main() {}").unwrap();

    let files = collect_walked_files(root, &WalkConfig::full_index());
    assert!(
        !files.iter().any(|f| f.contains("vendor")),
        "julieignored directory should be excluded"
    );
    assert!(files.iter().any(|f| f.contains("app.rs")));
}

#[test]
fn test_walk_vendor_scan_skips_gitignored_but_not_blacklisted() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join(".git")).unwrap();
    fs::write(root.join(".gitignore"), "build_output/\n").unwrap();
    // node_modules is in BLACKLISTED_DIRECTORIES but vendor_scan has blacklist OFF
    fs::create_dir_all(root.join("node_modules")).unwrap();
    fs::write(root.join("node_modules/pkg.js"), "// pkg").unwrap();
    fs::create_dir_all(root.join("build_output")).unwrap();
    fs::write(root.join("build_output/out.js"), "// out").unwrap();

    let files = collect_walked_files(root, &WalkConfig::vendor_scan());
    // node_modules NOT blacklisted in vendor_scan → included
    assert!(
        files.iter().any(|f| f.contains("node_modules")),
        "vendor_scan should include node_modules (blacklist OFF)"
    );
    // build_output IS gitignored → excluded
    assert!(
        !files.iter().any(|f| f.contains("build_output")),
        "vendor_scan should still respect .gitignore"
    );
}

#[test]
fn test_walk_always_excludes_dot_git() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join(".git/objects")).unwrap();
    fs::write(root.join(".git/config"), "[core]").unwrap();
    fs::write(root.join("src.rs"), "fn main() {}").unwrap();

    // Even vendor_scan (blacklisted dirs OFF) should skip .git
    let files = collect_walked_files(root, &WalkConfig::vendor_scan());
    assert!(
        !files.iter().any(|f| f.contains(".git")),
        ".git should always be excluded"
    );
}

#[test]
fn test_walk_always_excludes_dot_julie() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join(".git")).unwrap();
    fs::create_dir_all(root.join(".julie/logs")).unwrap();
    fs::write(root.join(".julie/logs/julie.log"), "mutating log").unwrap();
    fs::write(root.join(".julie/config.toml"), "[julie]").unwrap();
    fs::write(root.join("src.rs"), "fn main() {}").unwrap();

    let files = collect_walked_files(root, &WalkConfig::vendor_scan());
    assert!(
        !files.iter().any(|f| f.contains(".julie")),
        ".julie should always be excluded"
    );
    assert!(files.iter().any(|f| f.contains("src.rs")));
}

#[test]
fn test_walk_includes_dotfiles_like_editorconfig() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join(".git")).unwrap();
    fs::write(root.join(".editorconfig"), "root = true").unwrap();
    fs::write(root.join(".gitignore"), "").unwrap();

    let files = collect_walked_files(root, &WalkConfig::full_index());
    assert!(
        files.iter().any(|f| f.contains(".editorconfig")),
        "dotfiles like .editorconfig should be included"
    );
}

#[test]
fn test_walk_excludes_blacklisted_directories_when_enabled() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join(".git")).unwrap();
    fs::create_dir_all(root.join("node_modules")).unwrap();
    fs::write(root.join("node_modules/pkg.js"), "// pkg").unwrap();
    fs::create_dir_all(root.join("target/debug")).unwrap();
    fs::write(root.join("target/debug/out.rs"), "// out").unwrap();
    fs::write(root.join("src.rs"), "fn main() {}").unwrap();

    let files = collect_walked_files(root, &WalkConfig::full_index());
    assert!(files.iter().any(|f| f.contains("src.rs")));
    assert!(
        !files.iter().any(|f| f.contains("node_modules")),
        "blacklisted node_modules should be excluded in full_index"
    );
    assert!(
        !files.iter().any(|f| f.contains("target")),
        "blacklisted target should be excluded in full_index"
    );
}

#[test]
fn test_walk_nested_gitignore() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join(".git")).unwrap();
    fs::write(root.join(".gitignore"), "*.log\n").unwrap();
    fs::create_dir_all(root.join("subdir")).unwrap();
    fs::write(root.join("subdir/.gitignore"), "local_only/\n").unwrap();
    fs::create_dir_all(root.join("subdir/local_only")).unwrap();
    fs::write(root.join("subdir/local_only/data.txt"), "secret").unwrap();
    fs::write(root.join("subdir/code.rs"), "fn main() {}").unwrap();
    fs::write(root.join("app.log"), "log line").unwrap();

    let files = collect_walked_files(root, &WalkConfig::full_index());
    assert!(files.iter().any(|f| f.contains("code.rs")));
    assert!(
        !files.iter().any(|f| f.contains("local_only")),
        "nested .gitignore should exclude local_only/"
    );
    assert!(
        !files.iter().any(|f| f.contains(".log")),
        "root .gitignore should exclude *.log"
    );
}

#[test]
fn test_walk_works_without_git_repo() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    // NO .git directory
    fs::write(root.join("file.rs"), "fn main() {}").unwrap();

    let files = collect_walked_files(root, &WalkConfig::full_index());
    assert!(
        files.iter().any(|f| f.contains("file.rs")),
        "walker should work without a .git directory"
    );
}

/// Regression test: an ancestor `.julieignore` outside the walked workspace
/// must NOT influence the walk. A previous bug auto-generated `.julieignore`
/// at `~/` (when julie was misused to index home), and those anchored
/// patterns then silently dropped legitimate files from sibling projects on
/// every subsequent run.
#[test]
fn test_walk_ignores_ancestor_julieignore() {
    let outer = TempDir::new().unwrap();
    let outer_root = outer.path();
    let workspace = outer_root.join("workspace");
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir_all(workspace.join(".git")).unwrap();
    fs::create_dir_all(workspace.join("plugins")).unwrap();
    fs::write(workspace.join("plugins/source.rs"), "// real source").unwrap();

    // Sibling ancestor `.julieignore` outside the workspace — must be ignored.
    fs::write(outer_root.join(".julieignore"), "workspace/plugins/\n").unwrap();

    let files = collect_walked_files(&workspace, &WalkConfig::full_index());
    assert!(
        files.iter().any(|f| f.contains("plugins/source.rs")),
        "ancestor .julieignore must not exclude workspace files; got: {files:?}"
    );
}

/// Regression test: an ancestor `.gitignore` outside the walked workspace
/// must also be ignored. Workspace `.gitignore` is the authority; ancestor
/// patterns can silently drop legitimate files.
#[test]
fn test_walk_ignores_ancestor_gitignore() {
    let outer = TempDir::new().unwrap();
    let outer_root = outer.path();
    let workspace = outer_root.join("workspace");
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir_all(workspace.join(".git")).unwrap();
    fs::write(workspace.join("kept.rs"), "fn main() {}").unwrap();

    // Sibling ancestor `.gitignore` — must be ignored.
    fs::write(outer_root.join(".gitignore"), "*.rs\n").unwrap();

    let files = collect_walked_files(&workspace, &WalkConfig::full_index());
    assert!(
        files.iter().any(|f| f.contains("kept.rs")),
        "ancestor .gitignore must not exclude workspace files; got: {files:?}"
    );
}

/// Regression test: a nested workspace should inherit `.gitignore` rules from
/// ancestor directories up to the git root.
///
/// Reproduces: `/repo/.gitignore` has a pattern, and we index `/repo/packages/foo`.
/// With `parents(false)`, the git-root `.gitignore` was not inherited for non-git
/// mechanisms. The `ignore` crate's `git_ignore(true)` correctly reads `.gitignore`
/// up to the git root even with `parents(false)`.
///
/// Uses `private_data/` — a name NOT in BLACKLISTED_DIRECTORIES — to test that
/// exclusion comes from git root `.gitignore`, not the hardcoded blacklist.
#[test]
fn test_walk_inherits_git_root_gitignore_in_nested_workspace() {
    let outer = TempDir::new().unwrap();
    let outer_root = outer.path();

    // Structure:
    //   outer_root/repo/               ← git root (has .git/ and .gitignore)
    //   outer_root/repo/packages/foo/  ← nested workspace being indexed
    let repo = outer_root.join("repo");
    let workspace = repo.join("packages").join("foo");
    fs::create_dir_all(workspace.join("private_data")).unwrap();
    fs::create_dir_all(repo.join(".git")).unwrap();

    // .gitignore at git root: unanchored `private_data/` excludes any dir of that name.
    // NOTE: private_data is intentionally NOT in BLACKLISTED_DIRECTORIES so the
    // exclusion can only come from gitignore inheritance.
    fs::write(repo.join(".gitignore"), "private_data/\n").unwrap();

    fs::write(workspace.join("source.rs"), "fn main() {}").unwrap();
    fs::write(
        workspace.join("private_data").join("secret.rs"),
        "// should be excluded",
    )
    .unwrap();

    let files = collect_walked_files(&workspace, &WalkConfig::full_index());

    assert!(
        files.iter().any(|f| f.contains("source.rs")),
        "source.rs must be included in nested workspace walk; got: {files:?}"
    );
    assert!(
        !files.iter().any(|f| f.contains("secret.rs")),
        "private_data/secret.rs must be excluded by git-root .gitignore; got: {files:?}"
    );
}

#[test]
fn test_single_path_walk_respects_parent_gitignore() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::write(root.join(".gitignore"), "ignored.rs\n").unwrap();
    fs::write(root.join("ignored.rs"), "fn ignored() {}").unwrap();
    fs::write(root.join("kept.rs"), "fn kept() {}").unwrap();

    assert!(!single_path_walker_includes(
        root,
        &root.join("ignored.rs"),
        &WalkConfig::full_index()
    ));
    assert!(single_path_walker_includes(
        root,
        &root.join("kept.rs"),
        &WalkConfig::full_index()
    ));
}
