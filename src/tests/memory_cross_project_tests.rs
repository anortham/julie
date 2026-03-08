//! Tests for cross-project recall (`recall_cross_project()` in `src/memory/recall.rs`).
//!
//! Covers: cross-workspace aggregation, timestamp sorting, project tagging,
//! workspace summaries, global limit, date filters, plan exclusion, full flag,
//! and planId filtering.
//!
//! Split from `memory_recall_tests.rs` to keep test files under the 1000-line limit.

#[cfg(test)]
mod tests {
    use crate::memory::recall::recall_cross_project;
    use crate::memory::storage::{format_checkpoint, generate_checkpoint_id};
    use crate::memory::{Checkpoint, GitContext, RecallOptions};
    use std::path::Path;
    use tempfile::TempDir;

    // ========================================================================
    // Helpers (shared with memory_recall_tests.rs, duplicated here for
    // module independence — these are tiny test helpers, not business logic)
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
    // recall_cross_project() — cross-project recall (daemon mode)
    // ========================================================================

    #[test]
    fn test_cross_project_aggregates_checkpoints_from_multiple_workspaces() {
        let ws1 = TempDir::new().unwrap();
        let ws2 = TempDir::new().unwrap();

        // Project A: 2 checkpoints
        let cp_a1 = make_checkpoint("2026-03-07T10:00:00.000Z", "Project A first");
        let cp_a2 = make_checkpoint("2026-03-07T12:00:00.000Z", "Project A second");
        write_checkpoint(ws1.path(), &cp_a1);
        write_checkpoint(ws1.path(), &cp_a2);

        // Project B: 1 checkpoint
        let cp_b1 = make_checkpoint("2026-03-07T11:00:00.000Z", "Project B first");
        write_checkpoint(ws2.path(), &cp_b1);

        let workspaces = vec![
            ("project-a".to_string(), ws1.path().to_path_buf()),
            ("project-b".to_string(), ws2.path().to_path_buf()),
        ];

        let result = recall_cross_project(
            workspaces,
            RecallOptions::default(),
        )
        .unwrap();

        // Should have all 3 checkpoints
        assert_eq!(result.checkpoints.len(), 3);
    }

    #[test]
    fn test_cross_project_sorted_by_timestamp_newest_first() {
        let ws1 = TempDir::new().unwrap();
        let ws2 = TempDir::new().unwrap();

        let cp_a = make_checkpoint("2026-03-07T10:00:00.000Z", "Project A old");
        let cp_b = make_checkpoint("2026-03-07T14:00:00.000Z", "Project B newest");
        let cp_a2 = make_checkpoint("2026-03-07T12:00:00.000Z", "Project A middle");

        write_checkpoint(ws1.path(), &cp_a);
        write_checkpoint(ws1.path(), &cp_a2);
        write_checkpoint(ws2.path(), &cp_b);

        let workspaces = vec![
            ("project-a".to_string(), ws1.path().to_path_buf()),
            ("project-b".to_string(), ws2.path().to_path_buf()),
        ];

        let result = recall_cross_project(
            workspaces,
            RecallOptions::default(),
        )
        .unwrap();

        assert_eq!(result.checkpoints.len(), 3);
        // Newest first across projects
        assert!(result.checkpoints[0].summary.as_deref().unwrap().contains("Project B newest"));
        assert!(result.checkpoints[1].summary.as_deref().unwrap().contains("Project A middle"));
        assert!(result.checkpoints[2].summary.as_deref().unwrap().contains("Project A old"));
    }

    #[test]
    fn test_cross_project_checkpoints_tagged_with_source_project() {
        let ws1 = TempDir::new().unwrap();
        let ws2 = TempDir::new().unwrap();

        let cp_a = make_checkpoint("2026-03-07T10:00:00.000Z", "Auth refactor");
        let cp_b = make_checkpoint("2026-03-07T11:00:00.000Z", "DB migration");

        write_checkpoint(ws1.path(), &cp_a);
        write_checkpoint(ws2.path(), &cp_b);

        let workspaces = vec![
            ("julie".to_string(), ws1.path().to_path_buf()),
            ("coa-framework".to_string(), ws2.path().to_path_buf()),
        ];

        let result = recall_cross_project(
            workspaces,
            RecallOptions::default(),
        )
        .unwrap();

        assert_eq!(result.checkpoints.len(), 2);
        // Each checkpoint summary should be prefixed with [project-name]
        let summaries: Vec<&str> = result
            .checkpoints
            .iter()
            .filter_map(|cp| cp.summary.as_deref())
            .collect();
        assert!(
            summaries.iter().any(|s: &&str| s.starts_with("[coa-framework] ")),
            "Should tag checkpoint with project name, got: {:?}",
            summaries
        );
        assert!(
            summaries.iter().any(|s: &&str| s.starts_with("[julie] ")),
            "Should tag checkpoint with project name, got: {:?}",
            summaries
        );
    }

    #[test]
    fn test_cross_project_includes_workspace_summaries() {
        let ws1 = TempDir::new().unwrap();
        let ws2 = TempDir::new().unwrap();
        let ws3 = TempDir::new().unwrap();

        // ws1: 2 checkpoints
        let cp_a1 = make_checkpoint("2026-03-06T10:00:00.000Z", "Older");
        let cp_a2 = make_checkpoint("2026-03-07T10:00:00.000Z", "Newer");
        write_checkpoint(ws1.path(), &cp_a1);
        write_checkpoint(ws1.path(), &cp_a2);

        // ws2: 1 checkpoint
        let cp_b1 = make_checkpoint("2026-03-07T14:00:00.000Z", "Only one");
        write_checkpoint(ws2.path(), &cp_b1);

        // ws3: no checkpoints (no .memories dir)

        let workspaces = vec![
            ("proj-a".to_string(), ws1.path().to_path_buf()),
            ("proj-b".to_string(), ws2.path().to_path_buf()),
            ("proj-c".to_string(), ws3.path().to_path_buf()),
        ];

        let result = recall_cross_project(
            workspaces,
            RecallOptions::default(),
        )
        .unwrap();

        let summaries = result.workspaces.expect("Should have workspace summaries");
        assert_eq!(summaries.len(), 3);

        // Find each by name
        let sum_a = summaries.iter().find(|s| s.name == "proj-a").unwrap();
        assert_eq!(sum_a.checkpoint_count, 2);
        assert_eq!(
            sum_a.last_activity.as_deref(),
            Some("2026-03-07T10:00:00.000Z")
        );

        let sum_b = summaries.iter().find(|s| s.name == "proj-b").unwrap();
        assert_eq!(sum_b.checkpoint_count, 1);
        assert_eq!(
            sum_b.last_activity.as_deref(),
            Some("2026-03-07T14:00:00.000Z")
        );

        let sum_c = summaries.iter().find(|s| s.name == "proj-c").unwrap();
        assert_eq!(sum_c.checkpoint_count, 0);
        assert!(sum_c.last_activity.is_none());
    }

    #[test]
    fn test_cross_project_applies_global_limit() {
        let ws1 = TempDir::new().unwrap();
        let ws2 = TempDir::new().unwrap();

        // 3 checkpoints in each workspace = 6 total
        for i in 0..3 {
            let ts = format!("2026-03-07T{:02}:00:00.000Z", 10 + i);
            let cp = make_checkpoint(&ts, &format!("WS1 cp {}", i));
            write_checkpoint(ws1.path(), &cp);
        }
        for i in 0..3 {
            let ts = format!("2026-03-07T{:02}:00:00.000Z", 13 + i);
            let cp = make_checkpoint(&ts, &format!("WS2 cp {}", i));
            write_checkpoint(ws2.path(), &cp);
        }

        let workspaces = vec![
            ("ws1".to_string(), ws1.path().to_path_buf()),
            ("ws2".to_string(), ws2.path().to_path_buf()),
        ];

        let result = recall_cross_project(
            workspaces,
            RecallOptions {
                limit: Some(4),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(result.checkpoints.len(), 4, "Should respect global limit");
    }

    #[test]
    fn test_cross_project_applies_date_filters() {
        let ws1 = TempDir::new().unwrap();
        let ws2 = TempDir::new().unwrap();

        let cp_old = make_checkpoint("2026-03-05T10:00:00.000Z", "Old from ws1");
        let cp_new = make_checkpoint("2026-03-07T10:00:00.000Z", "New from ws1");
        let cp_b = make_checkpoint("2026-03-04T10:00:00.000Z", "Very old from ws2");

        write_checkpoint(ws1.path(), &cp_old);
        write_checkpoint(ws1.path(), &cp_new);
        write_checkpoint(ws2.path(), &cp_b);

        let workspaces = vec![
            ("ws1".to_string(), ws1.path().to_path_buf()),
            ("ws2".to_string(), ws2.path().to_path_buf()),
        ];

        let result = recall_cross_project(
            workspaces,
            RecallOptions {
                since: Some("2026-03-06T00:00:00.000Z".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(result.checkpoints.len(), 1);
        assert!(result.checkpoints[0].summary.as_deref().unwrap().contains("New from ws1"));
    }

    #[test]
    fn test_cross_project_no_active_plan_in_result() {
        let ws1 = TempDir::new().unwrap();
        let ws2 = TempDir::new().unwrap();

        let cp = make_checkpoint("2026-03-07T10:00:00.000Z", "Something");
        write_checkpoint(ws1.path(), &cp);

        // Even if ws1 has an active plan, cross-project doesn't return one
        // (plans are per-workspace, not cross-project)
        write_active_plan(ws1.path(), "my-plan", "My Plan", "Content");

        let workspaces = vec![
            ("ws1".to_string(), ws1.path().to_path_buf()),
            ("ws2".to_string(), ws2.path().to_path_buf()),
        ];

        let result = recall_cross_project(
            workspaces,
            RecallOptions::default(),
        )
        .unwrap();

        assert!(
            result.active_plan.is_none(),
            "Cross-project recall should not include an active plan"
        );
    }

    #[test]
    fn test_cross_project_empty_workspaces_list() {
        let workspaces: Vec<(String, std::path::PathBuf)> = vec![];

        let result = recall_cross_project(
            workspaces,
            RecallOptions::default(),
        )
        .unwrap();

        assert_eq!(result.checkpoints.len(), 0);
        assert!(result.active_plan.is_none());
        let summaries = result.workspaces.expect("Should have empty workspace summaries vec");
        assert_eq!(summaries.len(), 0);
    }

    #[test]
    fn test_cross_project_strips_git_when_not_full() {
        let ws1 = TempDir::new().unwrap();

        let cp = make_checkpoint_with_git("2026-03-07T10:00:00.000Z", "With git context");
        write_checkpoint(ws1.path(), &cp);

        let workspaces = vec![
            ("ws1".to_string(), ws1.path().to_path_buf()),
        ];

        let result = recall_cross_project(
            workspaces,
            RecallOptions::default(), // full defaults to false
        )
        .unwrap();

        assert_eq!(result.checkpoints.len(), 1);
        assert!(
            result.checkpoints[0].git.is_none(),
            "full=false should strip git context in cross-project mode"
        );
    }

    #[test]
    fn test_cross_project_preserves_git_when_full() {
        let ws1 = TempDir::new().unwrap();

        let cp = make_checkpoint_with_git("2026-03-07T10:00:00.000Z", "With git context");
        write_checkpoint(ws1.path(), &cp);

        let workspaces = vec![
            ("ws1".to_string(), ws1.path().to_path_buf()),
        ];

        let result = recall_cross_project(
            workspaces,
            RecallOptions {
                full: Some(true),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(result.checkpoints.len(), 1);
        assert!(
            result.checkpoints[0].git.is_some(),
            "full=true should preserve git context in cross-project mode"
        );
    }

    #[test]
    fn test_cross_project_plan_id_filter() {
        let ws1 = TempDir::new().unwrap();
        let ws2 = TempDir::new().unwrap();

        let cp1 = make_checkpoint_with_plan("2026-03-07T10:00:00.000Z", "Plan A ws1", "plan-a");
        let cp2 = make_checkpoint("2026-03-07T11:00:00.000Z", "No plan ws1");
        let cp3 = make_checkpoint_with_plan("2026-03-07T12:00:00.000Z", "Plan A ws2", "plan-a");
        let cp4 = make_checkpoint_with_plan("2026-03-07T13:00:00.000Z", "Plan B ws2", "plan-b");

        write_checkpoint(ws1.path(), &cp1);
        write_checkpoint(ws1.path(), &cp2);
        write_checkpoint(ws2.path(), &cp3);
        write_checkpoint(ws2.path(), &cp4);

        let workspaces = vec![
            ("ws1".to_string(), ws1.path().to_path_buf()),
            ("ws2".to_string(), ws2.path().to_path_buf()),
        ];

        let result = recall_cross_project(
            workspaces,
            RecallOptions {
                plan_id: Some("plan-a".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(result.checkpoints.len(), 2);
        assert!(result.checkpoints.iter().all(|cp| cp.plan_id == Some("plan-a".to_string())));
    }
}
