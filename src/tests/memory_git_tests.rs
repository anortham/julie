//! Tests for git context capture (`src/memory/git.rs`).
//!
//! Covers: `get_git_context()` — branch, commit, changed files, untracked files,
//! graceful failure when not in a git repo, and handling of edge cases.

#[cfg(test)]
mod tests {
    use crate::memory::git::get_git_context;
    use std::path::Path;
    use tempfile::TempDir;

    // ========================================================================
    // Helper: create a temp git repo with an initial commit
    // ========================================================================

    async fn create_temp_git_repo() -> TempDir {
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path();

        // Initialize git repo
        run_git(path, &["init"]).await;
        run_git(path, &["config", "user.email", "test@test.com"]).await;
        run_git(path, &["config", "user.name", "Test"]).await;

        // Create initial commit so HEAD exists
        let file = path.join("README.md");
        std::fs::write(&file, "# Test\n").unwrap();
        run_git(path, &["add", "README.md"]).await;
        run_git(path, &["commit", "-m", "initial commit"]).await;

        dir
    }

    async fn run_git(dir: &Path, args: &[&str]) {
        let output = tokio::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .await
            .expect("failed to run git");
        assert!(
            output.status.success(),
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // ========================================================================
    // Happy path tests
    // ========================================================================

    #[tokio::test]
    async fn test_returns_branch_name() {
        let repo = create_temp_git_repo().await;
        let ctx = get_git_context(repo.path()).await;
        let ctx = ctx.expect("should return Some for a git repo");
        // Default branch is either "main" or "master" depending on git config
        let branch = ctx.branch.expect("branch should be Some");
        assert!(
            branch == "main" || branch == "master",
            "unexpected branch: {branch}"
        );
    }

    #[tokio::test]
    async fn test_returns_short_commit_hash() {
        let repo = create_temp_git_repo().await;
        let ctx = get_git_context(repo.path()).await.unwrap();
        let commit = ctx.commit.expect("commit should be Some");
        // Short hash is typically 7 chars
        assert!(
            commit.len() >= 7 && commit.len() <= 12,
            "commit hash has unexpected length: {commit} (len={})",
            commit.len()
        );
        // Should be hex characters
        assert!(
            commit.chars().all(|c| c.is_ascii_hexdigit()),
            "commit hash contains non-hex chars: {commit}"
        );
    }

    #[tokio::test]
    async fn test_returns_changed_files() {
        let repo = create_temp_git_repo().await;
        let path = repo.path();

        // Modify an existing tracked file
        std::fs::write(path.join("README.md"), "# Modified\n").unwrap();

        let ctx = get_git_context(path).await.unwrap();
        let files = ctx.files.expect("files should be Some");
        assert!(
            files.contains(&"README.md".to_string()),
            "changed files should contain README.md, got: {files:?}"
        );
    }

    #[tokio::test]
    async fn test_returns_untracked_files() {
        let repo = create_temp_git_repo().await;
        let path = repo.path();

        // Create a new untracked file
        std::fs::write(path.join("new_file.txt"), "hello\n").unwrap();

        let ctx = get_git_context(path).await.unwrap();
        let files = ctx.files.expect("files should be Some");
        assert!(
            files.contains(&"new_file.txt".to_string()),
            "files should contain untracked new_file.txt, got: {files:?}"
        );
    }

    #[tokio::test]
    async fn test_includes_both_changed_and_untracked() {
        let repo = create_temp_git_repo().await;
        let path = repo.path();

        // Modify tracked file
        std::fs::write(path.join("README.md"), "# Modified\n").unwrap();
        // Add untracked file
        std::fs::write(path.join("new_file.txt"), "hello\n").unwrap();

        let ctx = get_git_context(path).await.unwrap();
        let files = ctx.files.expect("files should be Some");
        assert!(files.contains(&"README.md".to_string()));
        assert!(files.contains(&"new_file.txt".to_string()));
    }

    #[tokio::test]
    async fn test_no_changed_files_returns_none_or_empty() {
        let repo = create_temp_git_repo().await;
        let ctx = get_git_context(repo.path()).await.unwrap();
        // When nothing is changed, files should be None or an empty vec
        match &ctx.files {
            None => {} // acceptable
            Some(files) => assert!(files.is_empty(), "expected empty files, got: {files:?}"),
        }
    }

    #[tokio::test]
    async fn test_staged_files_included() {
        let repo = create_temp_git_repo().await;
        let path = repo.path();

        // Create and stage a new file (but don't commit)
        std::fs::write(path.join("staged.rs"), "fn main() {}\n").unwrap();
        run_git(path, &["add", "staged.rs"]).await;

        let ctx = get_git_context(path).await.unwrap();
        let files = ctx.files.expect("files should be Some");
        assert!(
            files.contains(&"staged.rs".to_string()),
            "should include staged files, got: {files:?}"
        );
    }

    #[tokio::test]
    async fn test_custom_branch_name() {
        let repo = create_temp_git_repo().await;
        let path = repo.path();

        run_git(path, &["checkout", "-b", "feature/cool-thing"]).await;

        let ctx = get_git_context(path).await.unwrap();
        assert_eq!(ctx.branch.as_deref(), Some("feature/cool-thing"));
    }

    // ========================================================================
    // Graceful failure tests
    // ========================================================================

    #[tokio::test]
    async fn test_returns_none_when_not_git_repo() {
        let dir = TempDir::new().expect("failed to create temp dir");
        // dir is NOT a git repo
        let ctx = get_git_context(dir.path()).await;
        assert!(
            ctx.is_none(),
            "should return None for non-git directory"
        );
    }

    #[tokio::test]
    async fn test_returns_none_for_nonexistent_path() {
        let ctx = get_git_context(Path::new("/tmp/definitely_does_not_exist_julie_test")).await;
        assert!(ctx.is_none(), "should return None for nonexistent path");
    }

    // ========================================================================
    // Deduplication test
    // ========================================================================

    #[tokio::test]
    async fn test_no_duplicate_files() {
        let repo = create_temp_git_repo().await;
        let path = repo.path();

        // Create a file, stage it, then also modify it (appears in both
        // staged and unstaged diffs, and also as a "new" file before commit)
        std::fs::write(path.join("dupcheck.txt"), "v1\n").unwrap();
        run_git(path, &["add", "dupcheck.txt"]).await;
        // Modify after staging — now it's in both staged and unstaged diff
        std::fs::write(path.join("dupcheck.txt"), "v2\n").unwrap();

        let ctx = get_git_context(path).await.unwrap();
        let files = ctx.files.unwrap_or_default();
        let count = files.iter().filter(|f| *f == "dupcheck.txt").count();
        assert_eq!(
            count, 1,
            "file should appear exactly once, but appeared {count} times in: {files:?}"
        );
    }
}
