//! Tests for recall — filesystem mode (`src/memory/recall.rs`).
//!
//! Covers: `recall()` — last N checkpoints, date filtering (since/days/from/to),
//! limit, full flag (git stripping), planId filtering, active plan inclusion,
//! and `parse_since()` helper.

#[cfg(test)]
mod tests {
    use crate::memory::recall::{parse_since, recall};
    use crate::memory::storage::{format_checkpoint, generate_checkpoint_id};
    use crate::memory::{Checkpoint, GitContext, RecallOptions};
    use chrono::{Duration, Utc};
    use std::path::Path;
    use tempfile::TempDir;

    // ========================================================================
    // Helper: create a checkpoint file in the .memories tree
    // ========================================================================

    /// Write a checkpoint file at .memories/{date}/{HHMMSS}_{hash}.md
    fn write_checkpoint(root: &Path, checkpoint: &Checkpoint) {
        let date = &checkpoint.timestamp[..10]; // YYYY-MM-DD
        let date_dir = root.join(".memories").join(date);
        std::fs::create_dir_all(&date_dir).unwrap();

        let id = &checkpoint.id;
        let time_part = &checkpoint.timestamp[11..19]; // HH:MM:SS
        let hhmmss = time_part.replace(':', "");
        let hash4 = id
            .strip_prefix("checkpoint_")
            .unwrap_or(id)
            .get(..4)
            .unwrap_or("0000");
        let filename = format!("{}_{}.md", hhmmss, hash4);

        let content = format_checkpoint(checkpoint);
        std::fs::write(date_dir.join(&filename), &content).unwrap();
    }

    /// Create a checkpoint struct with a given timestamp and description.
    fn make_checkpoint(timestamp: &str, description: &str) -> Checkpoint {
        let id = generate_checkpoint_id(timestamp, description);
        Checkpoint {
            id,
            timestamp: timestamp.to_string(),
            description: description.to_string(),
            checkpoint_type: None,
            context: None,
            decision: None,
            alternatives: None,
            impact: None,
            evidence: None,
            symbols: None,
            next: None,
            confidence: None,
            unknowns: None,
            tags: None,
            git: None,
            summary: Some(description.lines().next().unwrap_or("").to_string()),
            plan_id: None,
        }
    }

    /// Create a checkpoint with git context.
    fn make_checkpoint_with_git(timestamp: &str, description: &str) -> Checkpoint {
        let mut cp = make_checkpoint(timestamp, description);
        cp.git = Some(GitContext {
            branch: Some("main".to_string()),
            commit: Some("abc1234".to_string()),
            files: Some(vec!["src/main.rs".to_string()]),
        });
        cp
    }

    /// Create a checkpoint with a plan ID.
    fn make_checkpoint_with_plan(
        timestamp: &str,
        description: &str,
        plan_id: &str,
    ) -> Checkpoint {
        let mut cp = make_checkpoint(timestamp, description);
        cp.plan_id = Some(plan_id.to_string());
        cp
    }

    /// Write an active plan file.
    fn write_active_plan(root: &Path, plan_id: &str, title: &str, content: &str) {
        let plans_dir = root.join(".memories").join("plans");
        std::fs::create_dir_all(&plans_dir).unwrap();

        let plan_content = format!(
            "---\nid: {}\ntitle: {}\nstatus: active\ncreated: '2026-03-01T00:00:00.000Z'\nupdated: '2026-03-01T00:00:00.000Z'\n---\n\n{}\n",
            plan_id, title, content
        );
        std::fs::write(plans_dir.join(format!("{}.md", plan_id)), &plan_content).unwrap();

        // Write .active-plan marker
        std::fs::write(root.join(".memories").join(".active-plan"), plan_id).unwrap();
    }

    // ========================================================================
    // parse_since() — unit tests
    // ========================================================================

    #[test]
    fn test_parse_since_hours() {
        let now = Utc::now();
        let result = parse_since("2h").unwrap();
        let expected = now - Duration::hours(2);
        // Allow 1 second of drift
        assert!((result - expected).num_seconds().abs() < 2);
    }

    #[test]
    fn test_parse_since_minutes() {
        let now = Utc::now();
        let result = parse_since("30m").unwrap();
        let expected = now - Duration::minutes(30);
        assert!((result - expected).num_seconds().abs() < 2);
    }

    #[test]
    fn test_parse_since_days() {
        let now = Utc::now();
        let result = parse_since("3d").unwrap();
        let expected = now - Duration::days(3);
        assert!((result - expected).num_seconds().abs() < 2);
    }

    #[test]
    fn test_parse_since_weeks() {
        let now = Utc::now();
        let result = parse_since("1w").unwrap();
        let expected = now - Duration::weeks(1);
        assert!((result - expected).num_seconds().abs() < 2);
    }

    #[test]
    fn test_parse_since_iso_timestamp() {
        let result = parse_since("2026-03-07T10:00:00.000Z").unwrap();
        assert_eq!(
            result.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            "2026-03-07T10:00:00.000Z"
        );
    }

    #[test]
    fn test_parse_since_invalid() {
        assert!(parse_since("xyz").is_none());
        assert!(parse_since("").is_none());
        assert!(parse_since("5x").is_none());
    }

    // ========================================================================
    // recall() — basic: returns last N checkpoints sorted newest-first
    // ========================================================================

    #[test]
    fn test_recall_returns_last_n_checkpoints_newest_first() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // Create 3 checkpoints across 2 days
        let cp1 = make_checkpoint("2026-03-06T10:00:00.000Z", "First checkpoint");
        let cp2 = make_checkpoint("2026-03-06T14:00:00.000Z", "Second checkpoint");
        let cp3 = make_checkpoint("2026-03-07T09:00:00.000Z", "Third checkpoint");

        write_checkpoint(root, &cp1);
        write_checkpoint(root, &cp2);
        write_checkpoint(root, &cp3);

        let result = recall(root, RecallOptions::default()).unwrap();

        assert_eq!(result.checkpoints.len(), 3);
        // Newest first
        assert_eq!(result.checkpoints[0].id, cp3.id);
        assert_eq!(result.checkpoints[1].id, cp2.id);
        assert_eq!(result.checkpoints[2].id, cp1.id);
    }

    #[test]
    fn test_recall_default_limit_is_5() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // Create 7 checkpoints
        for i in 0..7 {
            let ts = format!("2026-03-07T{:02}:00:00.000Z", 10 + i);
            let cp = make_checkpoint(&ts, &format!("Checkpoint {}", i));
            write_checkpoint(root, &cp);
        }

        let result = recall(root, RecallOptions::default()).unwrap();

        assert_eq!(result.checkpoints.len(), 5, "Default limit should be 5");
        // Should be the 5 newest
        assert!(result.checkpoints[0]
            .description
            .contains("Checkpoint 6"));
        assert!(result.checkpoints[4]
            .description
            .contains("Checkpoint 2"));
    }

    #[test]
    fn test_recall_custom_limit() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        for i in 0..5 {
            let ts = format!("2026-03-07T{:02}:00:00.000Z", 10 + i);
            let cp = make_checkpoint(&ts, &format!("Checkpoint {}", i));
            write_checkpoint(root, &cp);
        }

        let result = recall(
            root,
            RecallOptions {
                limit: Some(2),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(result.checkpoints.len(), 2);
    }

    #[test]
    fn test_recall_limit_zero_returns_plan_only() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let cp = make_checkpoint("2026-03-07T10:00:00.000Z", "Some checkpoint");
        write_checkpoint(root, &cp);
        write_active_plan(root, "my-plan", "My Plan", "Plan content");

        let result = recall(
            root,
            RecallOptions {
                limit: Some(0),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(result.checkpoints.len(), 0, "limit=0 should return no checkpoints");
        assert!(result.active_plan.is_some(), "Should still include active plan");
        assert_eq!(result.active_plan.unwrap().id, "my-plan");
    }

    // ========================================================================
    // recall() — active plan included
    // ========================================================================

    #[test]
    fn test_recall_includes_active_plan() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        write_active_plan(root, "test-plan", "Test Plan", "Plan description");

        let result = recall(root, RecallOptions::default()).unwrap();

        assert!(result.active_plan.is_some());
        let plan = result.active_plan.unwrap();
        assert_eq!(plan.id, "test-plan");
        assert_eq!(plan.title, "Test Plan");
    }

    #[test]
    fn test_recall_no_active_plan() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let result = recall(root, RecallOptions::default()).unwrap();

        assert!(result.active_plan.is_none());
    }

    // ========================================================================
    // recall() — full flag (git metadata stripping)
    // ========================================================================

    #[test]
    fn test_recall_full_false_strips_git_context() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let cp = make_checkpoint_with_git("2026-03-07T10:00:00.000Z", "Checkpoint with git");
        write_checkpoint(root, &cp);

        // Default (full=None, which means false)
        let result = recall(root, RecallOptions::default()).unwrap();
        assert_eq!(result.checkpoints.len(), 1);
        assert!(
            result.checkpoints[0].git.is_none(),
            "full=false should strip git context"
        );
    }

    #[test]
    fn test_recall_full_true_preserves_git_context() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let cp = make_checkpoint_with_git("2026-03-07T10:00:00.000Z", "Checkpoint with git");
        write_checkpoint(root, &cp);

        let result = recall(
            root,
            RecallOptions {
                full: Some(true),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(result.checkpoints.len(), 1);
        assert!(
            result.checkpoints[0].git.is_some(),
            "full=true should preserve git context"
        );
        let git = result.checkpoints[0].git.as_ref().unwrap();
        assert_eq!(git.branch, Some("main".to_string()));
    }

    // ========================================================================
    // recall() — planId filtering
    // ========================================================================

    #[test]
    fn test_recall_plan_id_filter() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let cp1 = make_checkpoint_with_plan("2026-03-07T10:00:00.000Z", "Under plan A", "plan-a");
        let cp2 = make_checkpoint("2026-03-07T11:00:00.000Z", "No plan");
        let cp3 = make_checkpoint_with_plan("2026-03-07T12:00:00.000Z", "Under plan B", "plan-b");
        let cp4 = make_checkpoint_with_plan("2026-03-07T13:00:00.000Z", "Also plan A", "plan-a");

        write_checkpoint(root, &cp1);
        write_checkpoint(root, &cp2);
        write_checkpoint(root, &cp3);
        write_checkpoint(root, &cp4);

        let result = recall(
            root,
            RecallOptions {
                plan_id: Some("plan-a".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(result.checkpoints.len(), 2);
        assert!(result.checkpoints.iter().all(|c| c.plan_id == Some("plan-a".to_string())));
        // Newest first
        assert_eq!(result.checkpoints[0].id, cp4.id);
        assert_eq!(result.checkpoints[1].id, cp1.id);
    }

    // ========================================================================
    // recall() — date filtering: since
    // ========================================================================

    #[test]
    fn test_recall_since_duration() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let now = Utc::now();
        let ts_old = (now - Duration::hours(5)).to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let ts_recent = (now - Duration::hours(1)).to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

        let cp_old = make_checkpoint(&ts_old, "Old checkpoint");
        let cp_recent = make_checkpoint(&ts_recent, "Recent checkpoint");

        write_checkpoint(root, &cp_old);
        write_checkpoint(root, &cp_recent);

        let result = recall(
            root,
            RecallOptions {
                since: Some("2h".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(result.checkpoints.len(), 1);
        assert!(result.checkpoints[0].description.contains("Recent"));
    }

    #[test]
    fn test_recall_since_iso_timestamp() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let cp1 = make_checkpoint("2026-03-06T10:00:00.000Z", "Before cutoff");
        let cp2 = make_checkpoint("2026-03-07T14:00:00.000Z", "After cutoff");

        write_checkpoint(root, &cp1);
        write_checkpoint(root, &cp2);

        let result = recall(
            root,
            RecallOptions {
                since: Some("2026-03-07T00:00:00.000Z".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(result.checkpoints.len(), 1);
        assert!(result.checkpoints[0].description.contains("After cutoff"));
    }

    // ========================================================================
    // recall() — date filtering: days
    // ========================================================================

    #[test]
    fn test_recall_days_filter() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let now = Utc::now();
        let ts_old = (now - Duration::days(10)).to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let ts_recent = (now - Duration::hours(12)).to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

        let cp_old = make_checkpoint(&ts_old, "Old checkpoint");
        let cp_recent = make_checkpoint(&ts_recent, "Recent checkpoint");

        write_checkpoint(root, &cp_old);
        write_checkpoint(root, &cp_recent);

        let result = recall(
            root,
            RecallOptions {
                days: Some(3),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(result.checkpoints.len(), 1);
        assert!(result.checkpoints[0].description.contains("Recent"));
    }

    // ========================================================================
    // recall() — date filtering: from/to range
    // ========================================================================

    #[test]
    fn test_recall_from_to_range() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let cp1 = make_checkpoint("2026-03-05T10:00:00.000Z", "Before range");
        let cp2 = make_checkpoint("2026-03-06T12:00:00.000Z", "In range");
        let cp3 = make_checkpoint("2026-03-07T09:00:00.000Z", "Also in range");
        let cp4 = make_checkpoint("2026-03-08T10:00:00.000Z", "After range");

        write_checkpoint(root, &cp1);
        write_checkpoint(root, &cp2);
        write_checkpoint(root, &cp3);
        write_checkpoint(root, &cp4);

        let result = recall(
            root,
            RecallOptions {
                from: Some("2026-03-06".to_string()),
                to: Some("2026-03-07".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(result.checkpoints.len(), 2);
        assert!(result.checkpoints[0].description.contains("Also in range"));
        assert!(result.checkpoints[1].description.contains("In range"));
    }

    #[test]
    fn test_recall_from_only() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let cp1 = make_checkpoint("2026-03-05T10:00:00.000Z", "Before");
        let cp2 = make_checkpoint("2026-03-07T12:00:00.000Z", "After");

        write_checkpoint(root, &cp1);
        write_checkpoint(root, &cp2);

        let result = recall(
            root,
            RecallOptions {
                from: Some("2026-03-06".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(result.checkpoints.len(), 1);
        assert!(result.checkpoints[0].description.contains("After"));
    }

    #[test]
    fn test_recall_to_only() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let cp1 = make_checkpoint("2026-03-05T10:00:00.000Z", "Before cutoff");
        let cp2 = make_checkpoint("2026-03-07T12:00:00.000Z", "After cutoff");

        write_checkpoint(root, &cp1);
        write_checkpoint(root, &cp2);

        let result = recall(
            root,
            RecallOptions {
                to: Some("2026-03-06".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(result.checkpoints.len(), 1);
        assert!(result.checkpoints[0].description.contains("Before cutoff"));
    }

    // ========================================================================
    // recall() — search returns early (placeholder for Task 7)
    // ========================================================================

    #[test]
    fn test_recall_search_returns_empty_for_now() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let cp = make_checkpoint("2026-03-07T10:00:00.000Z", "Some checkpoint");
        write_checkpoint(root, &cp);

        let result = recall(
            root,
            RecallOptions {
                search: Some("something".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        // Task 7 will handle search — for now, return empty result with plan
        assert_eq!(result.checkpoints.len(), 0);
    }

    // ========================================================================
    // recall() — empty .memories directory
    // ========================================================================

    #[test]
    fn test_recall_no_memories_directory() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let result = recall(root, RecallOptions::default()).unwrap();

        assert_eq!(result.checkpoints.len(), 0);
        assert!(result.active_plan.is_none());
    }

    #[test]
    fn test_recall_empty_memories_directory() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        std::fs::create_dir_all(root.join(".memories")).unwrap();

        let result = recall(root, RecallOptions::default()).unwrap();

        assert_eq!(result.checkpoints.len(), 0);
    }

    // ========================================================================
    // recall() — skips non-date directories (e.g., "plans/")
    // ========================================================================

    #[test]
    fn test_recall_ignores_non_date_directories() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // Write a checkpoint in a valid date dir
        let cp = make_checkpoint("2026-03-07T10:00:00.000Z", "Valid checkpoint");
        write_checkpoint(root, &cp);

        // Create a non-date directory that should be ignored
        let plans_dir = root.join(".memories").join("plans");
        std::fs::create_dir_all(&plans_dir).unwrap();
        std::fs::write(plans_dir.join("something.md"), "not a checkpoint").unwrap();

        // Create another non-date directory
        let junk_dir = root.join(".memories").join("not-a-date");
        std::fs::create_dir_all(&junk_dir).unwrap();

        let result = recall(root, RecallOptions::default()).unwrap();

        assert_eq!(result.checkpoints.len(), 1);
        assert!(result.checkpoints[0].description.contains("Valid checkpoint"));
    }

    // ========================================================================
    // recall() — combined filters
    // ========================================================================

    #[test]
    fn test_recall_plan_id_plus_limit() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        for i in 0..5 {
            let ts = format!("2026-03-07T{:02}:00:00.000Z", 10 + i);
            let cp = make_checkpoint_with_plan(&ts, &format!("Under plan {}", i), "my-plan");
            write_checkpoint(root, &cp);
        }

        let result = recall(
            root,
            RecallOptions {
                plan_id: Some("my-plan".to_string()),
                limit: Some(2),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(result.checkpoints.len(), 2);
        // Should be the 2 newest
        assert!(result.checkpoints[0].description.contains("Under plan 4"));
        assert!(result.checkpoints[1].description.contains("Under plan 3"));
    }

    // ========================================================================
    // recall() — malformed checkpoint files are skipped
    // ========================================================================

    #[test]
    fn test_recall_skips_malformed_files() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // Write a valid checkpoint
        let cp = make_checkpoint("2026-03-07T10:00:00.000Z", "Valid checkpoint");
        write_checkpoint(root, &cp);

        // Write a malformed file in the same date dir
        let date_dir = root.join(".memories").join("2026-03-07");
        std::fs::write(date_dir.join("120000_bad1.md"), "not valid yaml frontmatter").unwrap();

        let result = recall(root, RecallOptions::default()).unwrap();

        // Should only get the valid checkpoint
        assert_eq!(result.checkpoints.len(), 1);
        assert!(result.checkpoints[0].description.contains("Valid checkpoint"));
    }
}
