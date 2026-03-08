//! Tests for checkpoint save (`src/memory/checkpoint.rs`).
//!
//! Covers: `save_checkpoint()` — directory creation, file writing, git context
//! capture, active plan ID attachment, deterministic ID generation, and
//! summary extraction from description.

#[cfg(test)]
mod tests {
    use crate::memory::checkpoint::{extract_summary, read_active_plan, save_checkpoint};
    use crate::memory::storage::{generate_checkpoint_id, parse_checkpoint};
    use crate::memory::{CheckpointInput, CheckpointType};
    use std::path::Path;
    use tempfile::TempDir;

    // ========================================================================
    // Helper: create a temp git repo with an initial commit
    // ========================================================================

    async fn create_temp_git_repo() -> TempDir {
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path();

        run_git(path, &["init"]).await;
        run_git(path, &["config", "user.email", "test@test.com"]).await;
        run_git(path, &["config", "user.name", "Test"]).await;

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
    // save_checkpoint() — basic functionality
    // ========================================================================

    #[tokio::test]
    async fn test_save_checkpoint_creates_date_dir_and_file() {
        let repo = create_temp_git_repo().await;
        let root = repo.path();

        let input = CheckpointInput {
            description: "## Added user authentication\n\nImplemented JWT-based auth.".into(),
            ..Default::default()
        };

        let checkpoint = save_checkpoint(root, input).await.unwrap();

        // Verify checkpoint has the expected fields
        assert!(checkpoint.id.starts_with("checkpoint_"));
        assert!(!checkpoint.timestamp.is_empty());
        assert!(checkpoint.description.contains("JWT-based auth"));

        // Verify the file was actually written to disk
        let date = &checkpoint.timestamp[..10]; // YYYY-MM-DD
        let memories_dir = root.join(".memories").join(date);
        assert!(memories_dir.exists(), "Date directory should exist");

        // Find the written file
        let entries: Vec<_> = std::fs::read_dir(&memories_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(entries.len(), 1, "Should have exactly one checkpoint file");

        let filename = entries[0].file_name().to_string_lossy().to_string();
        assert!(filename.ends_with(".md"), "File should be markdown");
        assert!(filename.contains('_'), "Filename should be HHMMSS_hash.md");
    }

    #[tokio::test]
    async fn test_save_checkpoint_file_content_matches_goldfish_format() {
        let repo = create_temp_git_repo().await;
        let root = repo.path();

        let input = CheckpointInput {
            description: "## Test checkpoint\n\nBody content here.".into(),
            tags: Some(vec!["testing".into(), "ci".into()]),
            checkpoint_type: Some(CheckpointType::Decision),
            ..Default::default()
        };

        let checkpoint = save_checkpoint(root, input).await.unwrap();

        // Read back the file content
        let date = &checkpoint.timestamp[..10];
        let memories_dir = root.join(".memories").join(date);
        let entry = std::fs::read_dir(&memories_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .next()
            .unwrap();

        let content = std::fs::read_to_string(entry.path()).unwrap();

        // Must start with YAML frontmatter
        assert!(content.starts_with("---\n"), "Must start with YAML frontmatter delimiter");
        assert!(content.contains("\n---\n"), "Must have closing YAML frontmatter delimiter");

        // Parse it back to verify round-trip
        let parsed = parse_checkpoint(&content).unwrap();
        assert_eq!(parsed.id, checkpoint.id);
        assert_eq!(parsed.timestamp, checkpoint.timestamp);
        assert_eq!(parsed.tags, Some(vec!["testing".into(), "ci".into()]));
        assert_eq!(parsed.checkpoint_type, Some(CheckpointType::Decision));
        assert!(parsed.description.contains("Body content here."));
    }

    #[tokio::test]
    async fn test_save_checkpoint_captures_git_context() {
        let repo = create_temp_git_repo().await;
        let root = repo.path();

        // Create a dirty file so git context has changed files
        std::fs::write(root.join("new_file.rs"), "fn main() {}").unwrap();

        let input = CheckpointInput {
            description: "Added new file".into(),
            ..Default::default()
        };

        let checkpoint = save_checkpoint(root, input).await.unwrap();

        // Git context should be present
        let git = checkpoint.git.as_ref().expect("git context should be present");
        assert!(git.branch.is_some(), "branch should be captured");
        assert!(git.commit.is_some(), "commit hash should be captured");
        // The new file should appear in changed files
        let files = git.files.as_ref().expect("changed files should be present");
        assert!(
            files.iter().any(|f| f.contains("new_file.rs")),
            "new_file.rs should be in changed files, got: {:?}",
            files
        );
    }

    #[tokio::test]
    async fn test_save_checkpoint_attaches_active_plan_id() {
        let repo = create_temp_git_repo().await;
        let root = repo.path();

        // Create .memories/.active-plan
        let memories_dir = root.join(".memories");
        std::fs::create_dir_all(&memories_dir).unwrap();
        std::fs::write(memories_dir.join(".active-plan"), "my-awesome-plan").unwrap();

        let input = CheckpointInput {
            description: "Work under a plan".into(),
            ..Default::default()
        };

        let checkpoint = save_checkpoint(root, input).await.unwrap();

        assert_eq!(
            checkpoint.plan_id,
            Some("my-awesome-plan".into()),
            "Active plan ID should be attached"
        );
    }

    #[tokio::test]
    async fn test_save_checkpoint_no_active_plan() {
        let repo = create_temp_git_repo().await;
        let root = repo.path();

        // No .active-plan file exists

        let input = CheckpointInput {
            description: "Work without a plan".into(),
            ..Default::default()
        };

        let checkpoint = save_checkpoint(root, input).await.unwrap();

        assert_eq!(
            checkpoint.plan_id, None,
            "plan_id should be None when no .active-plan file exists"
        );
    }

    #[tokio::test]
    async fn test_save_checkpoint_deterministic_id() {
        let repo = create_temp_git_repo().await;
        let root = repo.path();

        let input = CheckpointInput {
            description: "Deterministic test".into(),
            ..Default::default()
        };

        let checkpoint = save_checkpoint(root, input).await.unwrap();

        // The ID should match generate_checkpoint_id with the same timestamp + description
        let expected_id =
            generate_checkpoint_id(&checkpoint.timestamp, &checkpoint.description);
        assert_eq!(checkpoint.id, expected_id);
    }

    // ========================================================================
    // save_checkpoint() — summary extraction
    // ========================================================================

    #[tokio::test]
    async fn test_save_checkpoint_summary_from_heading() {
        let repo = create_temp_git_repo().await;
        let root = repo.path();

        let input = CheckpointInput {
            description: "## Refactored auth module\n\nMoved JWT logic to separate crate.".into(),
            ..Default::default()
        };

        let checkpoint = save_checkpoint(root, input).await.unwrap();

        assert_eq!(
            checkpoint.summary,
            Some("Refactored auth module".into()),
            "Summary should be extracted from first ## heading"
        );
    }

    #[tokio::test]
    async fn test_save_checkpoint_summary_from_first_line() {
        let repo = create_temp_git_repo().await;
        let root = repo.path();

        let input = CheckpointInput {
            description: "Fixed the flaky CI test\n\nThe timeout was too low.".into(),
            ..Default::default()
        };

        let checkpoint = save_checkpoint(root, input).await.unwrap();

        assert_eq!(
            checkpoint.summary,
            Some("Fixed the flaky CI test".into()),
            "Summary should be first non-empty line when no heading"
        );
    }

    #[tokio::test]
    async fn test_save_checkpoint_summary_skips_blank_lines() {
        let repo = create_temp_git_repo().await;
        let root = repo.path();

        let input = CheckpointInput {
            description: "\n\n\nActual content starts here\n\nMore stuff.".into(),
            ..Default::default()
        };

        let checkpoint = save_checkpoint(root, input).await.unwrap();

        assert_eq!(
            checkpoint.summary,
            Some("Actual content starts here".into()),
            "Summary should skip leading blank lines"
        );
    }

    // ========================================================================
    // save_checkpoint() — all optional fields
    // ========================================================================

    #[tokio::test]
    async fn test_save_checkpoint_all_optional_fields() {
        let repo = create_temp_git_repo().await;
        let root = repo.path();

        let input = CheckpointInput {
            description: "## Full checkpoint\n\nWith all fields populated.".into(),
            checkpoint_type: Some(CheckpointType::Decision),
            tags: Some(vec!["architecture".into(), "auth".into()]),
            symbols: Some(vec!["AuthService".into(), "JwtToken".into()]),
            decision: Some("Use JWT over session cookies".into()),
            alternatives: Some(vec![
                "Session cookies - simpler but not stateless".into(),
                "OAuth only - too complex for MVP".into(),
            ]),
            impact: Some("Enables stateless auth across microservices".into()),
            context: Some("Need auth for new API gateway".into()),
            evidence: Some(vec!["All 42 auth tests pass".into()]),
            unknowns: Some(vec!["Token rotation strategy TBD".into()]),
            next: Some("Implement refresh token flow".into()),
            confidence: Some(4),
        };

        let checkpoint = save_checkpoint(root, input).await.unwrap();

        // Verify all fields made it through
        assert_eq!(checkpoint.checkpoint_type, Some(CheckpointType::Decision));
        assert_eq!(checkpoint.tags, Some(vec!["architecture".into(), "auth".into()]));
        assert_eq!(checkpoint.symbols, Some(vec!["AuthService".into(), "JwtToken".into()]));
        assert_eq!(checkpoint.decision, Some("Use JWT over session cookies".into()));
        assert_eq!(checkpoint.alternatives, Some(vec![
            "Session cookies - simpler but not stateless".into(),
            "OAuth only - too complex for MVP".into(),
        ]));
        assert_eq!(checkpoint.impact, Some("Enables stateless auth across microservices".into()));
        assert_eq!(checkpoint.context, Some("Need auth for new API gateway".into()));
        assert_eq!(checkpoint.evidence, Some(vec!["All 42 auth tests pass".into()]));
        assert_eq!(checkpoint.unknowns, Some(vec!["Token rotation strategy TBD".into()]));
        assert_eq!(checkpoint.next, Some("Implement refresh token flow".into()));
        assert_eq!(checkpoint.confidence, Some(4));

        // Verify round-trip through file
        let date = &checkpoint.timestamp[..10];
        let memories_dir = root.join(".memories").join(date);
        let entry = std::fs::read_dir(&memories_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .next()
            .unwrap();
        let content = std::fs::read_to_string(entry.path()).unwrap();
        let parsed = parse_checkpoint(&content).unwrap();

        assert_eq!(parsed.decision, Some("Use JWT over session cookies".into()));
        assert_eq!(parsed.confidence, Some(4));
        assert_eq!(parsed.unknowns, Some(vec!["Token rotation strategy TBD".into()]));
    }

    // ========================================================================
    // save_checkpoint() — edge cases
    // ========================================================================

    #[tokio::test]
    async fn test_save_checkpoint_non_git_directory() {
        // A temp dir that is NOT a git repo
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let input = CheckpointInput {
            description: "Checkpoint outside git".into(),
            ..Default::default()
        };

        let checkpoint = save_checkpoint(root, input).await.unwrap();

        // Should still save successfully, just without git context
        assert!(checkpoint.git.is_none(), "git context should be None for non-git dir");
        assert!(checkpoint.id.starts_with("checkpoint_"));

        // Verify file exists
        let date = &checkpoint.timestamp[..10];
        let memories_dir = root.join(".memories").join(date);
        assert!(memories_dir.exists());
    }

    #[tokio::test]
    async fn test_save_checkpoint_multiple_in_same_date() {
        let repo = create_temp_git_repo().await;
        let root = repo.path();

        let cp1 = save_checkpoint(
            root,
            CheckpointInput {
                description: "First checkpoint".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let cp2 = save_checkpoint(
            root,
            CheckpointInput {
                description: "Second checkpoint".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        // Both should have different IDs
        assert_ne!(cp1.id, cp2.id);

        // Both should be in the same date directory
        let date = &cp1.timestamp[..10];
        let memories_dir = root.join(".memories").join(date);
        let entries: Vec<_> = std::fs::read_dir(&memories_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(entries.len(), 2, "Should have two checkpoint files");
    }

    #[tokio::test]
    async fn test_save_checkpoint_active_plan_with_whitespace() {
        let repo = create_temp_git_repo().await;
        let root = repo.path();

        // .active-plan with trailing newline (common with echo)
        let memories_dir = root.join(".memories");
        std::fs::create_dir_all(&memories_dir).unwrap();
        std::fs::write(memories_dir.join(".active-plan"), "  my-plan\n  ").unwrap();

        let input = CheckpointInput {
            description: "Whitespace handling".into(),
            ..Default::default()
        };

        let checkpoint = save_checkpoint(root, input).await.unwrap();

        assert_eq!(
            checkpoint.plan_id,
            Some("my-plan".into()),
            "Active plan ID should be trimmed"
        );
    }

    #[tokio::test]
    async fn test_save_checkpoint_empty_active_plan_file() {
        let repo = create_temp_git_repo().await;
        let root = repo.path();

        // Empty .active-plan file
        let memories_dir = root.join(".memories");
        std::fs::create_dir_all(&memories_dir).unwrap();
        std::fs::write(memories_dir.join(".active-plan"), "").unwrap();

        let input = CheckpointInput {
            description: "Empty plan file".into(),
            ..Default::default()
        };

        let checkpoint = save_checkpoint(root, input).await.unwrap();

        assert_eq!(
            checkpoint.plan_id, None,
            "Empty .active-plan should result in None plan_id"
        );
    }

    #[tokio::test]
    async fn test_save_checkpoint_filename_format() {
        let repo = create_temp_git_repo().await;
        let root = repo.path();

        let input = CheckpointInput {
            description: "Filename test".into(),
            ..Default::default()
        };

        let checkpoint = save_checkpoint(root, input).await.unwrap();

        // Get the actual filename
        let date = &checkpoint.timestamp[..10];
        let memories_dir = root.join(".memories").join(date);
        let entry = std::fs::read_dir(&memories_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .next()
            .unwrap();
        let filename = entry.file_name().to_string_lossy().to_string();

        // Verify format: HHMMSS_xxxx.md
        let parts: Vec<&str> = filename.trim_end_matches(".md").split('_').collect();
        assert_eq!(parts.len(), 2, "Filename should be HHMMSS_hash.md, got: {}", filename);
        assert_eq!(parts[0].len(), 6, "HHMMSS should be 6 digits, got: {}", parts[0]);
        assert_eq!(parts[1].len(), 4, "Hash should be 4 chars, got: {}", parts[1]);
        assert!(parts[0].chars().all(|c| c.is_ascii_digit()), "HHMMSS should be all digits");
        assert!(parts[1].chars().all(|c| c.is_ascii_hexdigit()), "Hash should be hex");
    }

    // ========================================================================
    // CheckpointInput — defaults
    // ========================================================================

    // ========================================================================
    // extract_summary() — unit tests
    // ========================================================================

    #[test]
    fn test_extract_summary_heading() {
        assert_eq!(
            extract_summary("## My heading\n\nBody text"),
            Some("My heading".into())
        );
    }

    #[test]
    fn test_extract_summary_no_heading() {
        assert_eq!(
            extract_summary("Just a plain line\nSecond line"),
            Some("Just a plain line".into())
        );
    }

    #[test]
    fn test_extract_summary_blank_then_content() {
        assert_eq!(
            extract_summary("\n\n\nContent after blanks"),
            Some("Content after blanks".into())
        );
    }

    #[test]
    fn test_extract_summary_empty() {
        assert_eq!(extract_summary(""), None);
        assert_eq!(extract_summary("   \n  \n  "), None);
    }

    // ========================================================================
    // read_active_plan() — unit tests
    // ========================================================================

    #[test]
    fn test_read_active_plan_nonexistent() {
        let dir = TempDir::new().unwrap();
        assert_eq!(read_active_plan(dir.path()), None);
    }

    #[test]
    fn test_read_active_plan_exists() {
        let dir = TempDir::new().unwrap();
        let memories = dir.path().join(".memories");
        std::fs::create_dir_all(&memories).unwrap();
        std::fs::write(memories.join(".active-plan"), "test-plan").unwrap();
        assert_eq!(read_active_plan(dir.path()), Some("test-plan".into()));
    }

    #[test]
    fn test_read_active_plan_whitespace() {
        let dir = TempDir::new().unwrap();
        let memories = dir.path().join(".memories");
        std::fs::create_dir_all(&memories).unwrap();
        std::fs::write(memories.join(".active-plan"), "  my-plan\n  ").unwrap();
        assert_eq!(read_active_plan(dir.path()), Some("my-plan".into()));
    }

    #[test]
    fn test_read_active_plan_empty() {
        let dir = TempDir::new().unwrap();
        let memories = dir.path().join(".memories");
        std::fs::create_dir_all(&memories).unwrap();
        std::fs::write(memories.join(".active-plan"), "").unwrap();
        assert_eq!(read_active_plan(dir.path()), None);
    }

    // ========================================================================
    // CheckpointInput — defaults
    // ========================================================================

    #[test]
    fn test_checkpoint_input_default() {
        let input = CheckpointInput::default();
        assert!(input.description.is_empty());
        assert!(input.checkpoint_type.is_none());
        assert!(input.tags.is_none());
        assert!(input.symbols.is_none());
        assert!(input.decision.is_none());
        assert!(input.alternatives.is_none());
        assert!(input.impact.is_none());
        assert!(input.context.is_none());
        assert!(input.evidence.is_none());
        assert!(input.unknowns.is_none());
        assert!(input.next.is_none());
        assert!(input.confidence.is_none());
    }
}
