// Tests for the shared walker builder (src/utils/walk.rs)
//
// Verifies that build_walker correctly handles:
// - .gitignore parsing (including nested)
// - .julieignore support
// - BLACKLISTED_DIRECTORIES filtering
// - .git exclusion (always, even with hidden(false))
// - Dotfile inclusion (e.g., .editorconfig)
// - Non-git workspaces

use crate::utils::walk::{WalkConfig, build_walker};
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
