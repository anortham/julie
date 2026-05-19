use tempfile::TempDir;

use crate::external_extract::{
    normalize_deleted_external_file, normalize_existing_external_file, normalize_external_root,
};
use crate::indexing_core::discovery::{discover_external_files, is_external_file_indexable};

fn write_file(path: &std::path::Path, content: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parent dir");
    }
    std::fs::write(path, content).expect("write fixture file");
}

#[test]
fn extract_paths_absolute_and_relative_files_share_db_key() {
    let temp_dir = TempDir::new().expect("temp dir");
    let root = normalize_external_root(temp_dir.path()).expect("normalize root");
    let absolute = root.join("src/lib.rs");
    write_file(&absolute, "pub fn lib() {}\n");

    let from_absolute =
        normalize_existing_external_file(&root, &absolute).expect("absolute file normalizes");
    let from_relative = normalize_existing_external_file(&root, std::path::Path::new("src/lib.rs"))
        .expect("relative file normalizes");

    assert_eq!(from_absolute.relative, "src/lib.rs");
    assert_eq!(from_relative.relative, "src/lib.rs");
    assert_eq!(from_absolute.absolute, from_relative.absolute);
}

#[test]
fn extract_paths_reject_files_outside_root() {
    let root_dir = TempDir::new().expect("root dir");
    let outside_dir = TempDir::new().expect("outside dir");
    let root = normalize_external_root(root_dir.path()).expect("normalize root");
    let outside = outside_dir.path().join("other.rs");
    write_file(&outside, "pub fn outside() {}\n");

    let error = normalize_existing_external_file(&root, &outside)
        .expect_err("outside existing file must be rejected");

    assert!(
        error.to_string().contains("outside external extract root"),
        "unexpected outside-root error: {error}"
    );
}

#[test]
fn extract_paths_reject_symlink_outside_root() {
    let root_dir = TempDir::new().expect("root dir");
    let outside_dir = TempDir::new().expect("outside dir");
    let root = normalize_external_root(root_dir.path()).expect("normalize root");
    let outside = outside_dir.path().join("target.rs");
    write_file(&outside, "pub fn outside() {}\n");
    let link = root.join("src/link.rs");
    std::fs::create_dir_all(link.parent().unwrap()).expect("create link parent");

    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, &link).expect("create symlink");
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(&outside, &link).expect("create symlink");

    let error = normalize_existing_external_file(&root, &link)
        .expect_err("symlink escaping root must be rejected");

    assert!(
        error.to_string().contains("outside external extract root"),
        "unexpected symlink error: {error}"
    );
}

#[test]
fn extract_deleted_missing_paths_normalize_safely() {
    let temp_dir = TempDir::new().expect("temp dir");
    let root = normalize_external_root(temp_dir.path()).expect("normalize root");

    let deleted = normalize_deleted_external_file(&root, std::path::Path::new("src/deleted.rs"))
        .expect("missing relative delete normalizes");
    assert_eq!(deleted.relative, "src/deleted.rs");
    assert_eq!(deleted.absolute, root.join("src/deleted.rs"));

    let traversal = normalize_deleted_external_file(&root, std::path::Path::new("../escape.rs"))
        .expect_err("delete traversal outside root is rejected");
    assert!(
        traversal
            .to_string()
            .contains("outside external extract root"),
        "unexpected traversal error: {traversal}"
    );
}

#[test]
fn extract_ignore_file_excludes_matching_files() {
    let temp_dir = TempDir::new().expect("temp dir");
    let root = normalize_external_root(temp_dir.path()).expect("normalize root");
    write_file(&root.join("src/keep.rs"), "pub fn keep() {}\n");
    write_file(&root.join("generated/out.rs"), "pub fn generated() {}\n");
    write_file(&root.join("external.ignore"), "generated/\n");

    let files = discover_external_files(&root, &[root.join("external.ignore")])
        .expect("external discovery succeeds");

    assert_eq!(files, vec![root.join("src/keep.rs")]);
    assert!(
        !is_external_file_indexable(
            &root,
            &root.join("generated/out.rs"),
            &[root.join("external.ignore")]
        )
        .expect("indexable check succeeds")
    );
}

#[test]
fn extract_discovery_respects_gitignore_and_julieignore_without_creating_julieignore() {
    let temp_dir = TempDir::new().expect("temp dir");
    let root = normalize_external_root(temp_dir.path()).expect("normalize root");
    write_file(&root.join(".gitignore"), "gitignored/\n");
    write_file(&root.join(".julieignore"), "julieignored/\n");
    write_file(&root.join("gitignored/a.rs"), "pub fn a() {}\n");
    write_file(&root.join("julieignored/b.rs"), "pub fn b() {}\n");
    write_file(&root.join("src/c.rs"), "pub fn c() {}\n");

    let files = discover_external_files(&root, &[]).expect("external discovery succeeds");

    assert_eq!(files, vec![root.join("src/c.rs")]);
    assert!(
        !is_external_file_indexable(&root, &root.join("gitignored/a.rs"), &[])
            .expect("gitignored file indexability check succeeds")
    );
    assert!(
        !is_external_file_indexable(&root, &root.join("julieignored/b.rs"), &[])
            .expect("julieignored file indexability check succeeds")
    );
    assert!(
        is_external_file_indexable(&root, &root.join("src/c.rs"), &[])
            .expect("kept file indexability check succeeds")
    );
}

#[test]
fn extract_ignore_file_cannot_override_hard_blacklist() {
    let temp_dir = TempDir::new().expect("temp dir");
    let root = normalize_external_root(temp_dir.path()).expect("normalize root");
    write_file(&root.join("src/keep.rs"), "pub fn keep() {}\n");
    write_file(&root.join("node_modules/pkg/index.rs"), "pub fn pkg() {}\n");
    write_file(&root.join("external.ignore"), "!node_modules/\n");

    let files = discover_external_files(&root, &[root.join("external.ignore")])
        .expect("external discovery succeeds");

    assert_eq!(files, vec![root.join("src/keep.rs")]);
    assert!(
        !is_external_file_indexable(
            &root,
            &root.join("node_modules/pkg/index.rs"),
            &[root.join("external.ignore")]
        )
        .expect("blacklisted file indexability check succeeds")
    );
}
