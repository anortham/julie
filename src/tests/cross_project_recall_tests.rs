//! Tests for cross-project recall functionality.
//!
//! Validates that recall can aggregate memories across multiple project directories
//! and that the user registry + recall integration works correctly.

#[cfg(test)]
mod tests {
    use crate::tools::memory::{recall_memories, search_memories, Memory, RecallOptions};
    use crate::user_registry::{list_projects_at, register_project_at};
    use serde_json::json;
    use std::fs;
    use tempfile::TempDir;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Create a fake project with memories in a temp directory.
    fn create_project_with_memories(
        parent: &std::path::Path,
        name: &str,
        memories: Vec<(i64, &str, &str, Vec<&str>)>, // (timestamp, type, description, tags)
    ) -> std::path::PathBuf {
        let project_dir = parent.join(name);
        let memories_dir = project_dir.join(".memories").join("2026-02-09");
        fs::create_dir_all(&memories_dir).unwrap();

        for (ts, mem_type, desc, tags) in memories {
            let memory = Memory::new(
                format!("{}_{}", mem_type, ts),
                ts,
                mem_type.to_string(),
            )
            .with_extra(json!({
                "description": desc,
                "tags": tags,
            }));

            let filename = format!("{}_{:04x}.json", ts, ts as u16);
            let file_path = memories_dir.join(filename);
            let json = serde_json::to_string_pretty(&memory).unwrap();
            fs::write(&file_path, json).unwrap();
        }

        project_dir
    }

    // -----------------------------------------------------------------------
    // Multi-project recall_memories
    // -----------------------------------------------------------------------

    #[test]
    fn test_recall_from_multiple_projects_and_merge() {
        let dir = TempDir::new().unwrap();

        let project_a = create_project_with_memories(
            dir.path(),
            "project-a",
            vec![
                (1000, "checkpoint", "First checkpoint in A", vec!["auth"]),
                (3000, "decision", "Architecture decision in A", vec!["arch"]),
            ],
        );

        let project_b = create_project_with_memories(
            dir.path(),
            "project-b",
            vec![(2000, "checkpoint", "Bugfix in B", vec!["bugfix"])],
        );

        let options = RecallOptions::default();

        // Recall from each project independently
        let memories_a = recall_memories(&project_a, options.clone()).unwrap();
        let memories_b = recall_memories(&project_b, options.clone()).unwrap();

        assert_eq!(memories_a.len(), 2);
        assert_eq!(memories_b.len(), 1);

        // Merge and sort chronologically (simulating what recall_global does)
        let mut merged: Vec<(Memory, String)> = Vec::new();
        for m in memories_a {
            merged.push((m, "project-a".to_string()));
        }
        for m in memories_b {
            merged.push((m, "project-b".to_string()));
        }
        merged.sort_by_key(|(m, _)| m.timestamp);

        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].0.timestamp, 1000); // A first
        assert_eq!(merged[0].1, "project-a");
        assert_eq!(merged[1].0.timestamp, 2000); // B second
        assert_eq!(merged[1].1, "project-b");
        assert_eq!(merged[2].0.timestamp, 3000); // A third
        assert_eq!(merged[2].1, "project-a");
    }

    #[test]
    fn test_limit_applied_after_merge() {
        let dir = TempDir::new().unwrap();

        let project_a = create_project_with_memories(
            dir.path(),
            "project-a",
            vec![
                (1000, "checkpoint", "Old checkpoint in A", vec![]),
                (4000, "checkpoint", "New checkpoint in A", vec![]),
            ],
        );

        let project_b = create_project_with_memories(
            dir.path(),
            "project-b",
            vec![
                (2000, "checkpoint", "Old checkpoint in B", vec![]),
                (3000, "checkpoint", "New checkpoint in B", vec![]),
            ],
        );

        // Recall ALL from both (no limit on individual calls)
        let options = RecallOptions { limit: None, ..Default::default() };
        let mut all: Vec<Memory> = Vec::new();
        all.extend(recall_memories(&project_a, options.clone()).unwrap());
        all.extend(recall_memories(&project_b, options).unwrap());

        // Sort chronologically, then apply limit=2 (should keep 2 newest)
        all.sort_by_key(|m| m.timestamp);
        all.reverse(); // Newest first
        all.truncate(2);
        all.reverse(); // Back to chronological

        assert_eq!(all.len(), 2);
        assert_eq!(all[0].timestamp, 3000, "Should keep 2nd newest");
        assert_eq!(all[1].timestamp, 4000, "Should keep newest");
    }

    #[test]
    fn test_missing_project_path_skipped_gracefully() {
        let dir = TempDir::new().unwrap();

        let real_project = create_project_with_memories(
            dir.path(),
            "real-project",
            vec![(1000, "checkpoint", "Real memory", vec![])],
        );

        let fake_path = dir.path().join("nonexistent-project");

        // Real project works
        let memories = recall_memories(&real_project, RecallOptions::default()).unwrap();
        assert_eq!(memories.len(), 1);

        // Missing project returns empty (no error)
        let memories = recall_memories(&fake_path, RecallOptions::default()).unwrap();
        assert_eq!(memories.len(), 0);
    }

    #[test]
    fn test_date_filter_works_across_projects() {
        let dir = TempDir::new().unwrap();

        let project_a = create_project_with_memories(
            dir.path(),
            "project-a",
            vec![
                (1000, "checkpoint", "Old A", vec![]),
                (5000, "checkpoint", "New A", vec![]),
            ],
        );

        let project_b = create_project_with_memories(
            dir.path(),
            "project-b",
            vec![
                (2000, "checkpoint", "Old B", vec![]),
                (6000, "checkpoint", "New B", vec![]),
            ],
        );

        // Filter: since=3000 should only return memories with ts >= 3000
        let options = RecallOptions {
            since: Some(3000),
            ..Default::default()
        };

        let memories_a = recall_memories(&project_a, options.clone()).unwrap();
        let memories_b = recall_memories(&project_b, options).unwrap();

        assert_eq!(memories_a.len(), 1, "Only new A should match");
        assert_eq!(memories_a[0].timestamp, 5000);
        assert_eq!(memories_b.len(), 1, "Only new B should match");
        assert_eq!(memories_b[0].timestamp, 6000);
    }

    #[test]
    fn test_type_filter_works_across_projects() {
        let dir = TempDir::new().unwrap();

        let project = create_project_with_memories(
            dir.path(),
            "mixed-types",
            vec![
                (1000, "checkpoint", "A checkpoint", vec![]),
                (2000, "decision", "A decision", vec![]),
                (3000, "learning", "A learning", vec![]),
            ],
        );

        let options = RecallOptions {
            memory_type: Some("decision".to_string()),
            ..Default::default()
        };

        let memories = recall_memories(&project, options).unwrap();
        assert_eq!(memories.len(), 1);
        assert_eq!(memories[0].memory_type, "decision");
    }

    // -----------------------------------------------------------------------
    // Search across projects
    // -----------------------------------------------------------------------

    #[test]
    fn test_search_across_projects() {
        let dir = TempDir::new().unwrap();

        let project_a = create_project_with_memories(
            dir.path(),
            "project-a",
            vec![(1000, "checkpoint", "Implemented JWT authentication", vec!["auth"])],
        );

        let project_b = create_project_with_memories(
            dir.path(),
            "project-b",
            vec![(2000, "checkpoint", "Fixed search ranking bug", vec!["search"])],
        );

        let options = RecallOptions::default();

        // Search for "authentication" should find project A's memory
        let results_a = search_memories(&project_a, "authentication", options.clone()).unwrap();
        assert!(!results_a.is_empty(), "Should find auth memory in project A");

        // Search for "search ranking" should find project B's memory
        let results_b = search_memories(&project_b, "search ranking", options).unwrap();
        assert!(!results_b.is_empty(), "Should find search memory in project B");
    }

    // -----------------------------------------------------------------------
    // Registry + recall integration
    // -----------------------------------------------------------------------

    #[test]
    fn test_registry_and_recall_integration() {
        let dir = TempDir::new().unwrap();
        let registry_path = dir.path().join("registry.json");

        // Create projects with memories
        let project_a = create_project_with_memories(
            dir.path(),
            "alpha",
            vec![(1000, "checkpoint", "Alpha work", vec!["feature"])],
        );
        let project_b = create_project_with_memories(
            dir.path(),
            "beta",
            vec![(2000, "checkpoint", "Beta work", vec!["bugfix"])],
        );

        // Register both projects
        register_project_at(&project_a, &registry_path).unwrap();
        register_project_at(&project_b, &registry_path).unwrap();

        // List projects from registry
        let projects = list_projects_at(&registry_path).unwrap();
        assert_eq!(projects.len(), 2);

        // Recall from all registered projects (simulating recall_global flow)
        let mut all_memories: Vec<(Memory, String)> = Vec::new();
        for project in &projects {
            let project_path = std::path::Path::new(&project.path);
            if project_path.exists() {
                let memories = recall_memories(project_path, RecallOptions::default()).unwrap();
                for m in memories {
                    all_memories.push((m, project.name.clone()));
                }
            }
        }

        all_memories.sort_by_key(|(m, _)| m.timestamp);

        assert_eq!(all_memories.len(), 2);
        // Both projects should be represented
        let project_names: Vec<&str> = all_memories.iter().map(|(_, n)| n.as_str()).collect();
        assert!(project_names.contains(&"alpha"));
        assert!(project_names.contains(&"beta"));
    }

    #[test]
    fn test_empty_registry_returns_no_memories() {
        let dir = TempDir::new().unwrap();
        let registry_path = dir.path().join("empty_registry.json");

        let projects = list_projects_at(&registry_path).unwrap();
        assert!(projects.is_empty());
    }

    // -----------------------------------------------------------------------
    // Fixture-based tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_recall_from_fixture_projects() {
        let fixtures_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join("cross_project_recall");

        let project_a = fixtures_dir.join("project_a");
        let project_b = fixtures_dir.join("project_b");

        assert!(project_a.exists(), "Fixture project_a should exist");
        assert!(project_b.exists(), "Fixture project_b should exist");

        let memories_a = recall_memories(&project_a, RecallOptions::default()).unwrap();
        let memories_b = recall_memories(&project_b, RecallOptions::default()).unwrap();

        assert_eq!(memories_a.len(), 2, "project_a should have 2 memories");
        assert_eq!(memories_b.len(), 1, "project_b should have 1 memory");

        // Verify fixture content was parsed correctly
        let descriptions: Vec<&str> = memories_a
            .iter()
            .filter_map(|m| m.extra.get("description").and_then(|v| v.as_str()))
            .collect();
        assert!(descriptions.iter().any(|d| d.contains("JWT")));
        assert!(descriptions.iter().any(|d| d.contains("PostgreSQL")));
    }
}
