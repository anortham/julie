use std::path::Path;
use std::process::Command;

use crate::workspace::git_identity::git_common_dir;

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(dir)
        .status()
        .expect("git must be installed");
    assert!(status.success(), "git {args:?} failed in {}", dir.display());
}

#[test]
fn git_common_dir_matches_between_main_checkout_and_linked_worktree() {
    let temp = tempfile::tempdir().unwrap();
    let main = temp.path().join("main");
    std::fs::create_dir_all(&main).unwrap();
    git(&main, &["init", "-q"]);
    git(
        &main,
        &[
            "-c",
            "user.name=seed",
            "-c",
            "user.email=seed@example.com",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "init",
        ],
    );
    let worktree = temp.path().join("wt");
    git(
        &main,
        &["worktree", "add", "-q", worktree.to_str().unwrap(), "HEAD"],
    );

    let expected = main.join(".git").canonicalize().unwrap();
    assert_eq!(git_common_dir(&main), Some(expected.clone()));
    assert_eq!(git_common_dir(&worktree), Some(expected));
}

#[test]
fn git_common_dir_is_none_outside_a_repository() {
    let temp = tempfile::tempdir().unwrap();
    assert_eq!(git_common_dir(temp.path()), None);
}
