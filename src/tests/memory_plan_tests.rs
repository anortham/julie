//! Tests for plan CRUD (`src/memory/plan.rs`).
//!
//! Covers: `save_plan()`, `get_plan()`, `list_plans()`, `activate_plan()`,
//! `get_active_plan()`, `update_plan()`, `complete_plan()`, slugification,
//! YAML frontmatter format, and edge cases.

#[cfg(test)]
mod tests {
    use crate::memory::plan::{
        activate_plan, complete_plan, get_active_plan, get_plan, list_plans, save_plan,
        slugify, update_plan,
    };
    use crate::memory::{PlanInput, PlanUpdate};
    use tempfile::TempDir;

    // ========================================================================
    // slugify() — unit tests
    // ========================================================================

    #[test]
    fn test_slugify_basic() {
        assert_eq!(slugify("My Feature Plan"), "my-feature-plan");
    }

    #[test]
    fn test_slugify_strips_special_chars() {
        assert_eq!(slugify("Hello, World! (v2)"), "hello-world-v2");
    }

    #[test]
    fn test_slugify_collapses_hyphens() {
        assert_eq!(slugify("foo---bar"), "foo-bar");
    }

    #[test]
    fn test_slugify_trims_leading_trailing_hyphens() {
        assert_eq!(slugify("--hello--"), "hello");
    }

    #[test]
    fn test_slugify_already_slug() {
        assert_eq!(slugify("my-plan"), "my-plan");
    }

    #[test]
    fn test_slugify_numbers() {
        assert_eq!(slugify("Phase 3 Plan"), "phase-3-plan");
    }

    #[test]
    fn test_slugify_uppercase() {
        assert_eq!(slugify("UPPERCASE PLAN"), "uppercase-plan");
    }

    // ========================================================================
    // save_plan() — basic functionality
    // ========================================================================

    #[test]
    fn test_save_plan_creates_file() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let input = PlanInput {
            id: None,
            title: "My Feature Plan".into(),
            content: "## Steps\n\n1. Do the thing\n2. Do the other thing".into(),
            tags: Some(vec!["feature".into(), "v2".into()]),
            activate: None,
        };

        let plan = save_plan(root, input).unwrap();

        assert_eq!(plan.id, "my-feature-plan");
        assert_eq!(plan.title, "My Feature Plan");
        assert_eq!(plan.status, "active");
        assert!(!plan.created.is_empty());
        assert!(!plan.updated.is_empty());
        assert_eq!(plan.tags, vec!["feature".to_string(), "v2".to_string()]);

        // Verify the file exists on disk
        let plan_file = root.join(".memories/plans/my-feature-plan.md");
        assert!(plan_file.exists(), "Plan file should exist at {:?}", plan_file);
    }

    #[test]
    fn test_save_plan_with_explicit_id() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let input = PlanInput {
            id: Some("custom-id".into()),
            title: "My Feature Plan".into(),
            content: "Content".into(),
            tags: None,
            activate: None,
        };

        let plan = save_plan(root, input).unwrap();

        assert_eq!(plan.id, "custom-id");

        let plan_file = root.join(".memories/plans/custom-id.md");
        assert!(plan_file.exists());
    }

    #[test]
    fn test_save_plan_auto_id_from_title() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let input = PlanInput {
            id: None,
            title: "Refactor Database Layer".into(),
            content: "Details here".into(),
            tags: None,
            activate: None,
        };

        let plan = save_plan(root, input).unwrap();
        assert_eq!(plan.id, "refactor-database-layer");
    }

    #[test]
    fn test_save_plan_and_activate() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let input = PlanInput {
            id: None,
            title: "Activated Plan".into(),
            content: "Will be active".into(),
            tags: None,
            activate: Some(true),
        };

        let plan = save_plan(root, input).unwrap();

        // Verify .active-plan was written
        let active_path = root.join(".memories/.active-plan");
        assert!(active_path.exists(), ".active-plan should exist");
        let active_id = std::fs::read_to_string(&active_path).unwrap();
        assert_eq!(active_id.trim(), plan.id);
    }

    #[test]
    fn test_save_plan_without_activate() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let input = PlanInput {
            id: None,
            title: "Not Activated".into(),
            content: "Content".into(),
            tags: None,
            activate: None,
        };

        save_plan(root, input).unwrap();

        // .active-plan should NOT exist
        let active_path = root.join(".memories/.active-plan");
        assert!(!active_path.exists(), ".active-plan should not exist");
    }

    #[test]
    fn test_save_plan_no_tags() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let input = PlanInput {
            id: None,
            title: "No Tags Plan".into(),
            content: "Content".into(),
            tags: None,
            activate: None,
        };

        let plan = save_plan(root, input).unwrap();
        assert!(plan.tags.is_empty(), "Tags should be empty vec");
    }

    #[test]
    fn test_save_plan_file_format() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let input = PlanInput {
            id: None,
            title: "Format Test".into(),
            content: "Body content here".into(),
            tags: Some(vec!["tag1".into()]),
            activate: None,
        };

        save_plan(root, input).unwrap();

        let content = std::fs::read_to_string(
            root.join(".memories/plans/format-test.md"),
        )
        .unwrap();

        // Must have YAML frontmatter
        assert!(content.starts_with("---\n"), "Must start with YAML frontmatter");
        assert!(content.contains("\n---\n"), "Must have closing frontmatter delimiter");

        // Check key fields in frontmatter
        assert!(content.contains("id: format-test"), "Frontmatter must contain id");
        assert!(content.contains("title: Format Test"), "Frontmatter must contain title");
        assert!(content.contains("status: active"), "Frontmatter must contain status");
        assert!(content.contains("created:"), "Frontmatter must contain created");
        assert!(content.contains("updated:"), "Frontmatter must contain updated");

        // Body should be after frontmatter
        assert!(content.contains("Body content here"), "Body should be present");
    }

    // ========================================================================
    // get_plan() — read a single plan
    // ========================================================================

    #[test]
    fn test_get_plan_exists() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let input = PlanInput {
            id: Some("test-plan".into()),
            title: "Test Plan".into(),
            content: "Test content".into(),
            tags: Some(vec!["test".into()]),
            activate: None,
        };
        save_plan(root, input).unwrap();

        let plan = get_plan(root, "test-plan").unwrap();
        assert!(plan.is_some(), "Plan should exist");

        let plan = plan.unwrap();
        assert_eq!(plan.id, "test-plan");
        assert_eq!(plan.title, "Test Plan");
        assert_eq!(plan.content, "Test content");
        assert_eq!(plan.tags, vec!["test".to_string()]);
    }

    #[test]
    fn test_get_plan_not_found() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let plan = get_plan(root, "nonexistent").unwrap();
        assert!(plan.is_none(), "Should return None for nonexistent plan");
    }

    #[test]
    fn test_get_plan_round_trip() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let input = PlanInput {
            id: Some("round-trip".into()),
            title: "Round Trip Test".into(),
            content: "## Steps\n\n1. First\n2. Second".into(),
            tags: Some(vec!["alpha".into(), "beta".into()]),
            activate: None,
        };

        let saved = save_plan(root, input).unwrap();
        let loaded = get_plan(root, "round-trip").unwrap().unwrap();

        assert_eq!(saved.id, loaded.id);
        assert_eq!(saved.title, loaded.title);
        assert_eq!(saved.content, loaded.content);
        assert_eq!(saved.status, loaded.status);
        assert_eq!(saved.created, loaded.created);
        assert_eq!(saved.updated, loaded.updated);
        assert_eq!(saved.tags, loaded.tags);
    }

    // ========================================================================
    // list_plans() — list all plans
    // ========================================================================

    #[test]
    fn test_list_plans_empty() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let plans = list_plans(root, None).unwrap();
        assert!(plans.is_empty(), "Should return empty vec when no plans exist");
    }

    #[test]
    fn test_list_plans_returns_all() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        save_plan(
            root,
            PlanInput {
                id: Some("plan-a".into()),
                title: "Plan A".into(),
                content: "Content A".into(),
                tags: None,
                activate: None,
            },
        )
        .unwrap();

        save_plan(
            root,
            PlanInput {
                id: Some("plan-b".into()),
                title: "Plan B".into(),
                content: "Content B".into(),
                tags: None,
                activate: None,
            },
        )
        .unwrap();

        let plans = list_plans(root, None).unwrap();
        assert_eq!(plans.len(), 2, "Should return 2 plans");

        let ids: Vec<&str> = plans.iter().map(|p| p.id.as_str()).collect();
        assert!(ids.contains(&"plan-a"));
        assert!(ids.contains(&"plan-b"));
    }

    #[test]
    fn test_list_plans_filter_by_status() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        save_plan(
            root,
            PlanInput {
                id: Some("active-plan".into()),
                title: "Active Plan".into(),
                content: "Content".into(),
                tags: None,
                activate: None,
            },
        )
        .unwrap();

        save_plan(
            root,
            PlanInput {
                id: Some("completed-plan".into()),
                title: "Completed Plan".into(),
                content: "Content".into(),
                tags: None,
                activate: None,
            },
        )
        .unwrap();

        // Complete the second plan
        complete_plan(root, "completed-plan").unwrap();

        let active_plans = list_plans(root, Some("active")).unwrap();
        assert_eq!(active_plans.len(), 1);
        assert_eq!(active_plans[0].id, "active-plan");

        let completed_plans = list_plans(root, Some("completed")).unwrap();
        assert_eq!(completed_plans.len(), 1);
        assert_eq!(completed_plans[0].id, "completed-plan");
    }

    // ========================================================================
    // activate_plan() — set active plan
    // ========================================================================

    #[test]
    fn test_activate_plan_writes_active_file() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        save_plan(
            root,
            PlanInput {
                id: Some("my-plan".into()),
                title: "My Plan".into(),
                content: "Content".into(),
                tags: None,
                activate: None,
            },
        )
        .unwrap();

        activate_plan(root, "my-plan").unwrap();

        let active_path = root.join(".memories/.active-plan");
        assert!(active_path.exists());
        let content = std::fs::read_to_string(&active_path).unwrap();
        assert_eq!(content.trim(), "my-plan");
    }

    #[test]
    fn test_activate_plan_nonexistent_fails() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let result = activate_plan(root, "nonexistent");
        assert!(result.is_err(), "Activating nonexistent plan should fail");
    }

    #[test]
    fn test_activate_plan_replaces_previous() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // Create two plans
        for id in &["plan-a", "plan-b"] {
            save_plan(
                root,
                PlanInput {
                    id: Some(id.to_string()),
                    title: format!("Plan {}", id),
                    content: "Content".into(),
                    tags: None,
                    activate: None,
                },
            )
            .unwrap();
        }

        activate_plan(root, "plan-a").unwrap();
        activate_plan(root, "plan-b").unwrap();

        let active_path = root.join(".memories/.active-plan");
        let content = std::fs::read_to_string(&active_path).unwrap();
        assert_eq!(content.trim(), "plan-b", "Should have replaced plan-a with plan-b");
    }

    // ========================================================================
    // get_active_plan() — read the active plan
    // ========================================================================

    #[test]
    fn test_get_active_plan_none_when_no_file() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let result = get_active_plan(root).unwrap();
        assert!(result.is_none(), "Should return None when no .active-plan file");
    }

    #[test]
    fn test_get_active_plan_returns_plan() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        save_plan(
            root,
            PlanInput {
                id: Some("my-active-plan".into()),
                title: "My Active Plan".into(),
                content: "Active content".into(),
                tags: None,
                activate: Some(true),
            },
        )
        .unwrap();

        let plan = get_active_plan(root).unwrap();
        assert!(plan.is_some(), "Should return the active plan");
        let plan = plan.unwrap();
        assert_eq!(plan.id, "my-active-plan");
        assert_eq!(plan.title, "My Active Plan");
    }

    #[test]
    fn test_get_active_plan_returns_none_when_plan_deleted() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // Write an .active-plan that points to a nonexistent plan
        let memories_dir = root.join(".memories");
        std::fs::create_dir_all(&memories_dir).unwrap();
        std::fs::write(memories_dir.join(".active-plan"), "deleted-plan").unwrap();

        let result = get_active_plan(root).unwrap();
        assert!(result.is_none(), "Should return None when plan file doesn't exist");
    }

    // ========================================================================
    // update_plan() — modify existing plan
    // ========================================================================

    #[test]
    fn test_update_plan_title() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        save_plan(
            root,
            PlanInput {
                id: Some("update-me".into()),
                title: "Original Title".into(),
                content: "Original content".into(),
                tags: None,
                activate: None,
            },
        )
        .unwrap();

        let updated = update_plan(
            root,
            "update-me",
            PlanUpdate {
                title: Some("New Title".into()),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(updated.title, "New Title");
        assert_eq!(updated.content, "Original content", "Content should be unchanged");
    }

    #[test]
    fn test_update_plan_content() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        save_plan(
            root,
            PlanInput {
                id: Some("update-content".into()),
                title: "Title".into(),
                content: "Old content".into(),
                tags: None,
                activate: None,
            },
        )
        .unwrap();

        let updated = update_plan(
            root,
            "update-content",
            PlanUpdate {
                content: Some("New content".into()),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(updated.content, "New content");
        assert_eq!(updated.title, "Title", "Title should be unchanged");
    }

    #[test]
    fn test_update_plan_status() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        save_plan(
            root,
            PlanInput {
                id: Some("update-status".into()),
                title: "Title".into(),
                content: "Content".into(),
                tags: None,
                activate: None,
            },
        )
        .unwrap();

        let updated = update_plan(
            root,
            "update-status",
            PlanUpdate {
                status: Some("archived".into()),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(updated.status, "archived");
    }

    #[test]
    fn test_update_plan_tags() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        save_plan(
            root,
            PlanInput {
                id: Some("update-tags".into()),
                title: "Title".into(),
                content: "Content".into(),
                tags: Some(vec!["old-tag".into()]),
                activate: None,
            },
        )
        .unwrap();

        let updated = update_plan(
            root,
            "update-tags",
            PlanUpdate {
                tags: Some(vec!["new-tag1".into(), "new-tag2".into()]),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(updated.tags, vec!["new-tag1".to_string(), "new-tag2".to_string()]);
    }

    #[test]
    fn test_update_plan_updates_timestamp() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let saved = save_plan(
            root,
            PlanInput {
                id: Some("update-ts".into()),
                title: "Title".into(),
                content: "Content".into(),
                tags: None,
                activate: None,
            },
        )
        .unwrap();

        // Small sleep to ensure timestamps differ
        std::thread::sleep(std::time::Duration::from_millis(10));

        let updated = update_plan(
            root,
            "update-ts",
            PlanUpdate {
                title: Some("New Title".into()),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(updated.created, saved.created, "Created timestamp must not change");
        assert_ne!(
            updated.updated, saved.updated,
            "Updated timestamp should change"
        );
    }

    #[test]
    fn test_update_plan_nonexistent_fails() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let result = update_plan(
            root,
            "nonexistent",
            PlanUpdate {
                title: Some("New".into()),
                ..Default::default()
            },
        );

        assert!(result.is_err(), "Updating nonexistent plan should fail");
    }

    #[test]
    fn test_update_plan_persists_to_disk() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        save_plan(
            root,
            PlanInput {
                id: Some("persist-test".into()),
                title: "Original".into(),
                content: "Original content".into(),
                tags: None,
                activate: None,
            },
        )
        .unwrap();

        update_plan(
            root,
            "persist-test",
            PlanUpdate {
                title: Some("Updated".into()),
                content: Some("Updated content".into()),
                ..Default::default()
            },
        )
        .unwrap();

        // Read it back from disk
        let loaded = get_plan(root, "persist-test").unwrap().unwrap();
        assert_eq!(loaded.title, "Updated");
        assert_eq!(loaded.content, "Updated content");
    }

    // ========================================================================
    // complete_plan() — mark plan as completed
    // ========================================================================

    #[test]
    fn test_complete_plan_sets_status() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        save_plan(
            root,
            PlanInput {
                id: Some("complete-me".into()),
                title: "Completable Plan".into(),
                content: "Content".into(),
                tags: None,
                activate: None,
            },
        )
        .unwrap();

        let completed = complete_plan(root, "complete-me").unwrap();
        assert_eq!(completed.status, "completed");

        // Verify persistence
        let loaded = get_plan(root, "complete-me").unwrap().unwrap();
        assert_eq!(loaded.status, "completed");
    }

    #[test]
    fn test_complete_plan_nonexistent_fails() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let result = complete_plan(root, "nonexistent");
        assert!(result.is_err(), "Completing nonexistent plan should fail");
    }

    #[test]
    fn test_complete_plan_preserves_other_fields() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        save_plan(
            root,
            PlanInput {
                id: Some("preserve-fields".into()),
                title: "Preserved Title".into(),
                content: "Preserved content".into(),
                tags: Some(vec!["important".into()]),
                activate: None,
            },
        )
        .unwrap();

        let completed = complete_plan(root, "preserve-fields").unwrap();
        assert_eq!(completed.title, "Preserved Title");
        assert_eq!(completed.content, "Preserved content");
        assert_eq!(completed.tags, vec!["important".to_string()]);
    }

    // ========================================================================
    // Edge cases
    // ========================================================================

    #[test]
    fn test_save_plan_creates_plans_directory() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // .memories/plans/ doesn't exist yet
        assert!(!root.join(".memories/plans").exists());

        save_plan(
            root,
            PlanInput {
                id: Some("first-plan".into()),
                title: "First".into(),
                content: "Content".into(),
                tags: None,
                activate: None,
            },
        )
        .unwrap();

        assert!(root.join(".memories/plans").exists());
    }

    #[test]
    fn test_list_plans_ignores_active_plan_file() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        save_plan(
            root,
            PlanInput {
                id: Some("only-plan".into()),
                title: "Only Plan".into(),
                content: "Content".into(),
                tags: None,
                activate: Some(true),
            },
        )
        .unwrap();

        // .active-plan is in the plans dir but should NOT be listed
        let plans = list_plans(root, None).unwrap();
        assert_eq!(plans.len(), 1, "Should only list .md files, not .active-plan");
        assert_eq!(plans[0].id, "only-plan");
    }

    #[test]
    fn test_plan_content_with_yaml_like_strings() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // Content that looks like YAML — should not confuse the parser
        let tricky_content = "---\nThis looks like frontmatter but it's body\n---";

        save_plan(
            root,
            PlanInput {
                id: Some("tricky".into()),
                title: "Tricky Content".into(),
                content: tricky_content.into(),
                tags: None,
                activate: None,
            },
        )
        .unwrap();

        let loaded = get_plan(root, "tricky").unwrap().unwrap();
        assert_eq!(loaded.title, "Tricky Content");
        // The body content may have the tricky content in some form
        assert!(loaded.content.contains("This looks like frontmatter"));
    }

    #[test]
    fn test_save_plan_empty_tags_vec() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let input = PlanInput {
            id: Some("empty-tags".into()),
            title: "Empty Tags".into(),
            content: "Content".into(),
            tags: Some(vec![]),
            activate: None,
        };

        let plan = save_plan(root, input).unwrap();
        assert!(plan.tags.is_empty());
    }

    #[test]
    fn test_multiple_plan_operations_workflow() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // Create 3 plans
        for i in 1..=3 {
            save_plan(
                root,
                PlanInput {
                    id: Some(format!("plan-{}", i)),
                    title: format!("Plan {}", i),
                    content: format!("Content for plan {}", i),
                    tags: None,
                    activate: None,
                },
            )
            .unwrap();
        }

        // List all plans
        let all = list_plans(root, None).unwrap();
        assert_eq!(all.len(), 3);

        // Activate plan-2
        activate_plan(root, "plan-2").unwrap();
        let active = get_active_plan(root).unwrap().unwrap();
        assert_eq!(active.id, "plan-2");

        // Complete plan-1
        complete_plan(root, "plan-1").unwrap();

        // List active plans
        let active_plans = list_plans(root, Some("active")).unwrap();
        assert_eq!(active_plans.len(), 2);

        // List completed plans
        let completed_plans = list_plans(root, Some("completed")).unwrap();
        assert_eq!(completed_plans.len(), 1);
        assert_eq!(completed_plans[0].id, "plan-1");

        // Update plan-3
        let updated = update_plan(
            root,
            "plan-3",
            PlanUpdate {
                title: Some("Updated Plan 3".into()),
                tags: Some(vec!["updated".into()]),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(updated.title, "Updated Plan 3");
        assert_eq!(updated.tags, vec!["updated".to_string()]);
    }
}
