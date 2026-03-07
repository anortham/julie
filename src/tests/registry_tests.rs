//! Tests for the global project registry (CRUD, persistence, atomic writes).

use crate::registry::{GlobalRegistry, ProjectStatus, RegisterResult};

// ============================================================================
// BASIC CRUD
// ============================================================================

#[test]
fn test_new_registry_is_empty() {
    let registry = GlobalRegistry::new();
    assert_eq!(registry.version, "1");
    assert!(registry.projects.is_empty());
    assert!(registry.list_projects().is_empty());
}

#[test]
fn test_register_project() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().join("my-project");
    std::fs::create_dir_all(&project_dir).unwrap();

    let mut registry = GlobalRegistry::new();
    let result = registry.register_project(&project_dir).unwrap();

    assert!(matches!(&result, RegisterResult::Created(_)));
    let id = result.workspace_id().to_string();
    assert!(!id.is_empty());
    assert!(id.contains("my-project"), "workspace ID should contain project name, got: {}", id);
    assert_eq!(registry.projects.len(), 1);

    let entry = registry.get_project(&id).unwrap();
    assert_eq!(entry.name, "my-project");
    assert_eq!(entry.workspace_id, id);
    assert_eq!(entry.status, ProjectStatus::Registered);
    assert!(entry.last_indexed.is_none());
    assert!(entry.symbol_count.is_none());
    assert!(entry.file_count.is_none());
}

#[test]
fn test_register_duplicate_returns_already_exists() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().join("my-project");
    std::fs::create_dir_all(&project_dir).unwrap();

    let mut registry = GlobalRegistry::new();
    let result1 = registry.register_project(&project_dir).unwrap();
    let result2 = registry.register_project(&project_dir).unwrap();

    assert!(matches!(&result1, RegisterResult::Created(_)));
    assert!(matches!(&result2, RegisterResult::AlreadyExists(_)));
    assert_eq!(result1.workspace_id(), result2.workspace_id());
    assert_eq!(registry.projects.len(), 1);
}

#[test]
fn test_remove_project() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().join("my-project");
    std::fs::create_dir_all(&project_dir).unwrap();

    let mut registry = GlobalRegistry::new();
    let id = registry.register_project(&project_dir).unwrap().workspace_id().to_string();
    assert_eq!(registry.projects.len(), 1);

    let removed = registry.remove_project(&id);
    assert!(removed);
    assert!(registry.projects.is_empty());
}

#[test]
fn test_remove_nonexistent_returns_false() {
    let mut registry = GlobalRegistry::new();
    assert!(!registry.remove_project("nonexistent_abcd1234"));
}

#[test]
fn test_list_projects_sorted_by_name() {
    let temp = tempfile::tempdir().unwrap();

    let dir_c = temp.path().join("charlie");
    let dir_a = temp.path().join("alpha");
    let dir_b = temp.path().join("bravo");
    std::fs::create_dir_all(&dir_c).unwrap();
    std::fs::create_dir_all(&dir_a).unwrap();
    std::fs::create_dir_all(&dir_b).unwrap();

    let mut registry = GlobalRegistry::new();
    registry.register_project(&dir_c).unwrap();
    registry.register_project(&dir_a).unwrap();
    registry.register_project(&dir_b).unwrap();

    let projects = registry.list_projects();
    assert_eq!(projects.len(), 3);
    assert_eq!(projects[0].name, "alpha");
    assert_eq!(projects[1].name, "bravo");
    assert_eq!(projects[2].name, "charlie");
}

// ============================================================================
// STATUS TRANSITIONS
// ============================================================================

#[test]
fn test_mark_indexing() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().join("my-project");
    std::fs::create_dir_all(&project_dir).unwrap();

    let mut registry = GlobalRegistry::new();
    let id = registry.register_project(&project_dir).unwrap().workspace_id().to_string();

    assert!(registry.mark_indexing(&id));
    assert_eq!(registry.get_project(&id).unwrap().status, ProjectStatus::Indexing);
}

#[test]
fn test_mark_ready_with_stats() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().join("my-project");
    std::fs::create_dir_all(&project_dir).unwrap();

    let mut registry = GlobalRegistry::new();
    let id = registry.register_project(&project_dir).unwrap().workspace_id().to_string();

    assert!(registry.mark_ready(&id, 1500, 42));

    let entry = registry.get_project(&id).unwrap();
    assert_eq!(entry.status, ProjectStatus::Ready);
    assert_eq!(entry.symbol_count, Some(1500));
    assert_eq!(entry.file_count, Some(42));
    assert!(entry.last_indexed.is_some());
}

#[test]
fn test_mark_error() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().join("my-project");
    std::fs::create_dir_all(&project_dir).unwrap();

    let mut registry = GlobalRegistry::new();
    let id = registry.register_project(&project_dir).unwrap().workspace_id().to_string();

    assert!(registry.mark_error(&id, "parse failure".to_string()));
    assert_eq!(
        registry.get_project(&id).unwrap().status,
        ProjectStatus::Error("parse failure".to_string())
    );
}

#[test]
fn test_mark_nonexistent_returns_false() {
    let mut registry = GlobalRegistry::new();
    assert!(!registry.mark_indexing("nonexistent_abcd1234"));
    assert!(!registry.mark_ready("nonexistent_abcd1234", 0, 0));
    assert!(!registry.mark_error("nonexistent_abcd1234", "fail".to_string()));
}

// ============================================================================
// PERSISTENCE (SAVE / LOAD)
// ============================================================================

#[test]
fn test_save_and_load_roundtrip() {
    let temp = tempfile::tempdir().unwrap();
    let julie_home = temp.path().join("julie-home");
    std::fs::create_dir_all(&julie_home).unwrap();

    let project_dir = temp.path().join("my-project");
    std::fs::create_dir_all(&project_dir).unwrap();

    // Create and save a registry
    let mut registry = GlobalRegistry::new();
    let id = registry.register_project(&project_dir).unwrap().workspace_id().to_string();
    registry.mark_ready(&id, 500, 20);
    registry.save(&julie_home).unwrap();

    // Load it back
    let loaded = GlobalRegistry::load(&julie_home).unwrap();
    assert_eq!(loaded.version, "1");
    assert_eq!(loaded.projects.len(), 1);

    let entry = loaded.get_project(&id).unwrap();
    assert_eq!(entry.name, "my-project");
    assert_eq!(entry.status, ProjectStatus::Ready);
    assert_eq!(entry.symbol_count, Some(500));
    assert_eq!(entry.file_count, Some(20));
    assert!(entry.last_indexed.is_some());
}

#[test]
fn test_load_creates_empty_when_missing() {
    let temp = tempfile::tempdir().unwrap();
    let julie_home = temp.path().join("nonexistent-home");
    // Don't create the directory — load should handle this gracefully

    let registry = GlobalRegistry::load(&julie_home).unwrap();
    assert!(registry.projects.is_empty());
}

#[test]
fn test_load_handles_empty_file() {
    let temp = tempfile::tempdir().unwrap();
    let julie_home = temp.path();
    std::fs::write(julie_home.join("registry.toml"), "").unwrap();

    let registry = GlobalRegistry::load(julie_home).unwrap();
    assert!(registry.projects.is_empty());
}

#[test]
fn test_save_creates_directory_if_needed() {
    let temp = tempfile::tempdir().unwrap();
    let julie_home = temp.path().join("deep").join("nested").join("dir");

    let registry = GlobalRegistry::new();
    registry.save(&julie_home).unwrap();

    assert!(julie_home.join("registry.toml").exists());
}

#[test]
fn test_atomic_write_no_temp_file_left_behind() {
    let temp = tempfile::tempdir().unwrap();
    let julie_home = temp.path();

    let registry = GlobalRegistry::new();
    registry.save(julie_home).unwrap();

    // The temp file should have been renamed, not left behind
    let tmp_file = julie_home.join("registry.toml.tmp");
    assert!(!tmp_file.exists(), "Temp file should be cleaned up after atomic rename");
    assert!(julie_home.join("registry.toml").exists());
}

#[test]
fn test_toml_is_human_readable() {
    let temp = tempfile::tempdir().unwrap();
    let julie_home = temp.path();
    let project_dir = temp.path().join("my-project");
    std::fs::create_dir_all(&project_dir).unwrap();

    let mut registry = GlobalRegistry::new();
    registry.register_project(&project_dir).unwrap();
    registry.save(julie_home).unwrap();

    let content = std::fs::read_to_string(julie_home.join("registry.toml")).unwrap();
    assert!(content.contains("version = \"1\""), "TOML should contain version field");
    assert!(content.contains("my-project"), "TOML should contain project name");
    assert!(content.contains("[projects."), "TOML should have projects section");
}

// ============================================================================
// WORKSPACE ID CONSISTENCY
// ============================================================================

#[test]
fn test_workspace_id_matches_generate_workspace_id() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().join("test-project");
    std::fs::create_dir_all(&project_dir).unwrap();

    let mut registry = GlobalRegistry::new();
    let id_from_registry = registry.register_project(&project_dir).unwrap().workspace_id().to_string();

    // Should match the standalone function
    let canonical = project_dir.canonicalize().unwrap();
    let id_from_fn = crate::workspace::registry::generate_workspace_id(
        &canonical.to_string_lossy(),
    )
    .unwrap();

    assert_eq!(id_from_registry, id_from_fn);
}
