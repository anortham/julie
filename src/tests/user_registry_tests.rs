//! Tests for the user-level project registry (src/user_registry.rs).
//!
//! Uses explicit registry paths (temp dirs) to avoid touching real ~/.julie/.

#[cfg(test)]
mod tests {
    use crate::user_registry::*;
    use tempfile::TempDir;

    /// Helper: create a temp registry path.
    fn temp_registry() -> (TempDir, std::path::PathBuf) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("project_registry.json");
        (dir, path)
    }

    /// Helper: create a fake workspace directory.
    fn fake_workspace(dir: &TempDir, name: &str) -> std::path::PathBuf {
        let ws = dir.path().join(name);
        std::fs::create_dir_all(&ws).unwrap();
        ws
    }

    // -----------------------------------------------------------------------
    // Registration basics
    // -----------------------------------------------------------------------

    #[test]
    fn test_register_creates_registry_file() {
        let (dir, registry_path) = temp_registry();
        let ws = fake_workspace(&dir, "my-project");

        register_project_at(&ws, &registry_path).unwrap();

        assert!(registry_path.exists(), "Registry file should be created");
    }

    #[test]
    fn test_register_stores_correct_fields() {
        let (dir, registry_path) = temp_registry();
        let ws = fake_workspace(&dir, "cool-app");

        register_project_at(&ws, &registry_path).unwrap();

        let projects = list_projects_at(&registry_path).unwrap();
        assert_eq!(projects.len(), 1);

        let entry = &projects[0];
        assert_eq!(entry.name, "cool-app");
        assert_eq!(entry.path, ws.to_string_lossy().to_string());
        assert!(entry.registered_at > 0);
        assert_eq!(entry.registered_at, entry.last_opened);
        assert!(!entry.project_id.is_empty());
    }

    #[test]
    fn test_register_multiple_projects() {
        let (dir, registry_path) = temp_registry();
        let ws_a = fake_workspace(&dir, "project-a");
        let ws_b = fake_workspace(&dir, "project-b");
        let ws_c = fake_workspace(&dir, "project-c");

        register_project_at(&ws_a, &registry_path).unwrap();
        register_project_at(&ws_b, &registry_path).unwrap();
        register_project_at(&ws_c, &registry_path).unwrap();

        let projects = list_projects_at(&registry_path).unwrap();
        assert_eq!(projects.len(), 3);

        let names: Vec<&str> = projects.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"project-a"));
        assert!(names.contains(&"project-b"));
        assert!(names.contains(&"project-c"));
    }

    // -----------------------------------------------------------------------
    // Re-registration (idempotency)
    // -----------------------------------------------------------------------

    #[test]
    fn test_reregister_updates_last_opened_only() {
        let (dir, registry_path) = temp_registry();
        let ws = fake_workspace(&dir, "my-project");

        register_project_at(&ws, &registry_path).unwrap();

        let first = list_projects_at(&registry_path).unwrap();
        let first_registered_at = first[0].registered_at;
        let first_last_opened = first[0].last_opened;

        // Sleep briefly so timestamps differ
        std::thread::sleep(std::time::Duration::from_millis(1100));

        register_project_at(&ws, &registry_path).unwrap();

        let second = list_projects_at(&registry_path).unwrap();
        assert_eq!(second.len(), 1, "Should still be one project");
        assert_eq!(
            second[0].registered_at, first_registered_at,
            "registered_at should NOT change"
        );
        assert!(
            second[0].last_opened >= first_last_opened,
            "last_opened should be updated"
        );
    }

    // -----------------------------------------------------------------------
    // list_projects sorting
    // -----------------------------------------------------------------------

    #[test]
    fn test_list_projects_sorted_by_last_opened_desc() {
        let (dir, registry_path) = temp_registry();
        let ws_a = fake_workspace(&dir, "oldest");
        let ws_b = fake_workspace(&dir, "middle");
        let ws_c = fake_workspace(&dir, "newest");

        // Register with small delays so timestamps differ
        register_project_at(&ws_a, &registry_path).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        register_project_at(&ws_b, &registry_path).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        register_project_at(&ws_c, &registry_path).unwrap();

        let projects = list_projects_at(&registry_path).unwrap();
        assert_eq!(projects.len(), 3);
        assert_eq!(projects[0].name, "newest", "Most recently opened first");
        assert_eq!(projects[1].name, "middle");
        assert_eq!(projects[2].name, "oldest", "Least recently opened last");
    }

    // -----------------------------------------------------------------------
    // Edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn test_list_projects_empty_when_no_registry() {
        let dir = TempDir::new().unwrap();
        let registry_path = dir.path().join("nonexistent_registry.json");

        let projects = list_projects_at(&registry_path).unwrap();
        assert!(projects.is_empty());
    }

    #[test]
    fn test_register_handles_corrupt_registry() {
        let (dir, registry_path) = temp_registry();
        let ws = fake_workspace(&dir, "my-project");

        // Write corrupt JSON
        std::fs::write(&registry_path, "{ this is not valid json }").unwrap();

        // Registration should succeed (starts fresh)
        register_project_at(&ws, &registry_path).unwrap();

        let projects = list_projects_at(&registry_path).unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].name, "my-project");
    }

    #[test]
    fn test_registry_json_format() {
        let (dir, registry_path) = temp_registry();
        let ws = fake_workspace(&dir, "test-proj");

        register_project_at(&ws, &registry_path).unwrap();

        // Read raw JSON and verify structure
        let raw = std::fs::read_to_string(&registry_path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();

        assert_eq!(parsed["version"], 1);
        assert!(parsed["projects"].is_object());

        let projects = parsed["projects"].as_object().unwrap();
        assert_eq!(projects.len(), 1);

        let entry = projects.values().next().unwrap();
        assert!(entry["project_id"].is_string());
        assert!(entry["name"].is_string());
        assert!(entry["path"].is_string());
        assert!(entry["registered_at"].is_number());
        assert!(entry["last_opened"].is_number());
    }

    #[test]
    fn test_register_creates_parent_directory() {
        let dir = TempDir::new().unwrap();
        let registry_path = dir.path().join("nested").join("deep").join("registry.json");

        let ws = fake_workspace(&dir, "deep-proj");
        register_project_at(&ws, &registry_path).unwrap();

        assert!(registry_path.exists());
        let projects = list_projects_at(&registry_path).unwrap();
        assert_eq!(projects.len(), 1);
    }

    #[test]
    fn test_project_id_uses_workspace_id_format() {
        let (dir, registry_path) = temp_registry();
        let ws = fake_workspace(&dir, "my-project");

        register_project_at(&ws, &registry_path).unwrap();

        let projects = list_projects_at(&registry_path).unwrap();
        let id = &projects[0].project_id;

        // generate_workspace_id format: "name_hash8"
        assert!(
            id.contains('_'),
            "Project ID should be name_hash format, got: {}",
            id
        );
        let parts: Vec<&str> = id.rsplitn(2, '_').collect();
        assert_eq!(
            parts[0].len(),
            8,
            "Hash suffix should be 8 chars, got: {}",
            parts[0]
        );
    }
}
