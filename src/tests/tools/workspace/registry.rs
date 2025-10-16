//! Inline tests extracted from src/workspace/registry.rs
//!
//! These tests verify core registry functionality:
//! - Workspace ID generation with consistent hashing
//! - Name sanitization for filesystem compatibility
//! - Expiration logic for primary vs reference workspaces

use crate::workspace::registry::*;

#[test]
fn test_generate_workspace_id() {
    let path = "/Users/test/project-a";
    let id = generate_workspace_id(path).unwrap();

    // Should be format: name_hash8
    assert!(id.contains('_'));
    let parts: Vec<&str> = id.split('_').collect();
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[1].len(), 8); // Hash should be 8 chars
}

#[test]
fn test_sanitize_name() {
    assert_eq!(sanitize_name("project-a"), "project-a");
    assert_eq!(sanitize_name("Project A"), "project_a");
    assert_eq!(sanitize_name("my:project"), "my_project");
    assert_eq!(sanitize_name(""), "ws_");
}

#[test]
fn test_workspace_entry_expiration() {
    let config = RegistryConfig::default();

    // Primary workspace should never expire
    let primary =
        WorkspaceEntry::new("/test/primary".to_string(), WorkspaceType::Primary, &config)
            .unwrap();
    assert!(!primary.is_expired());
    assert!(primary.expires_at.is_none());

    // Reference workspace should have expiration
    let reference = WorkspaceEntry::new(
        "/test/reference".to_string(),
        WorkspaceType::Reference,
        &config,
    )
    .unwrap();
    assert!(!reference.is_expired()); // Should not be expired immediately
    assert!(reference.expires_at.is_some());
}
