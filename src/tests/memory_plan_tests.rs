// Tests for plan system (mutable development plans - Phase 1.5)
// Following TDD: Write tests first, then implement

use anyhow::Result;
use std::fs;
use tempfile::TempDir;

use crate::tools::memory::GitContext;
use crate::tools::memory::PlanAction;
use crate::tools::memory::plan::*;

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Create a test workspace with .memories/plans/ directory
fn create_test_workspace() -> Result<TempDir> {
    let temp = TempDir::new()?;
    let plans_dir = temp.path().join(".memories").join("plans");
    fs::create_dir_all(&plans_dir)?;
    Ok(temp)
}

// ============================================================================
// CREATE PLAN TESTS
// ============================================================================

#[test]
fn test_create_plan_basic() -> Result<()> {
    // Setup
    let temp = create_test_workspace()?;
    let workspace_root = temp.path();

    // Create a plan
    let plan = create_plan(
        workspace_root,
        "Add Search Feature".to_string(),
        Some("## Tasks\n- [ ] Design API".to_string()),
        None,
    )?;

    // Verify plan structure
    assert_eq!(plan.title, "Add Search Feature");
    assert_eq!(plan.memory_type, "plan");
    assert_eq!(plan.status, PlanStatus::Active); // New plans default to Active
    assert_eq!(plan.content, Some("## Tasks\n- [ ] Design API".to_string()));
    assert!(plan.timestamp > 0);

    // Verify ID format
    assert!(plan.id.starts_with("plan_"));
    assert_eq!(plan.id, "plan_add-search-feature");

    Ok(())
}

#[test]
fn test_create_plan_saves_to_disk() -> Result<()> {
    // Setup
    let temp = create_test_workspace()?;
    let workspace_root = temp.path();

    // Create plan
    let plan = create_plan(workspace_root, "Test Plan".to_string(), None, None)?;

    // Verify file exists
    let plan_path = workspace_root
        .join(".memories")
        .join("plans")
        .join("plan_test-plan.json");

    assert!(plan_path.exists(), "Plan file should exist on disk");

    // Verify file content is valid JSON
    let content = fs::read_to_string(&plan_path)?;
    let parsed: serde_json::Value = serde_json::from_str(&content)?;

    assert_eq!(parsed["title"], "Test Plan");
    assert_eq!(parsed["type"], "plan");
    assert_eq!(parsed["status"], "active");

    Ok(())
}

#[test]
fn test_create_plan_with_git_context() -> Result<()> {
    // Setup
    let temp = create_test_workspace()?;
    let workspace_root = temp.path();

    let git_context = GitContext {
        branch: "feature/plans".to_string(),
        commit: "abc123".to_string(),
        dirty: false,
        files_changed: Some(vec!["src/main.rs".to_string()]),
    };

    // Create plan with git context
    let plan = create_plan(
        workspace_root,
        "Plan with Git".to_string(),
        None,
        Some(git_context.clone()),
    )?;

    // Verify git context is saved
    assert!(plan.git.is_some());
    let saved_git = plan.git.unwrap();
    assert_eq!(saved_git.branch, "feature/plans");
    assert_eq!(saved_git.commit, "abc123");
    assert_eq!(saved_git.dirty, false);

    Ok(())
}

// ============================================================================
// UPDATE PLAN TESTS
// ============================================================================

#[test]
fn test_update_plan_content() -> Result<()> {
    // Setup
    let temp = create_test_workspace()?;
    let workspace_root = temp.path();

    // Create initial plan
    let plan = create_plan(
        workspace_root,
        "Update Test".to_string(),
        Some("Initial content".to_string()),
        None,
    )?;

    let original_timestamp = plan.timestamp;

    // Wait a moment to ensure timestamp changes (timestamp is in seconds)
    std::thread::sleep(std::time::Duration::from_secs(1));

    // Update the plan
    let updates = PlanUpdates {
        content: Some("Updated content".to_string()),
        ..Default::default()
    };

    let updated = update_plan(workspace_root, &plan.id, updates)?;

    // Verify updates
    assert_eq!(updated.content, Some("Updated content".to_string()));
    assert_eq!(updated.title, "Update Test"); // Title unchanged
    assert_eq!(updated.status, PlanStatus::Active); // Status unchanged
    assert!(
        updated.timestamp > original_timestamp,
        "Timestamp should be updated"
    );

    Ok(())
}

#[test]
fn test_update_plan_status() -> Result<()> {
    // Setup
    let temp = create_test_workspace()?;
    let workspace_root = temp.path();

    // Create plan
    let plan = create_plan(workspace_root, "Status Test".to_string(), None, None)?;

    assert_eq!(plan.status, PlanStatus::Active);

    // Update status to completed
    let updates = PlanUpdates {
        status: Some(PlanStatus::Completed),
        ..Default::default()
    };

    let updated = update_plan(workspace_root, &plan.id, updates)?;

    assert_eq!(updated.status, PlanStatus::Completed);

    Ok(())
}

#[test]
fn test_update_plan_persists_to_disk() -> Result<()> {
    // Setup
    let temp = create_test_workspace()?;
    let workspace_root = temp.path();

    // Create and update plan
    let plan = create_plan(workspace_root, "Persist Test".to_string(), None, None)?;

    let updates = PlanUpdates {
        content: Some("New content".to_string()),
        ..Default::default()
    };

    update_plan(workspace_root, &plan.id, updates)?;

    // Read from disk and verify
    let plan_path = workspace_root
        .join(".memories")
        .join("plans")
        .join(format!("{}.json", plan.id));

    let content = fs::read_to_string(&plan_path)?;
    let parsed: serde_json::Value = serde_json::from_str(&content)?;

    assert_eq!(parsed["content"], "New content");

    Ok(())
}

#[test]
fn test_update_nonexistent_plan_fails() -> Result<()> {
    // Setup
    let temp = create_test_workspace()?;
    let workspace_root = temp.path();

    // Try to update non-existent plan
    let updates = PlanUpdates::default();
    let result = update_plan(workspace_root, "plan_nonexistent", updates);

    assert!(result.is_err(), "Updating non-existent plan should fail");

    Ok(())
}

// ============================================================================
// GET PLAN TESTS
// ============================================================================

#[test]
fn test_get_plan_retrieves_existing() -> Result<()> {
    // Setup
    let temp = create_test_workspace()?;
    let workspace_root = temp.path();

    // Create plan
    let created = create_plan(
        workspace_root,
        "Get Test".to_string(),
        Some("Test content".to_string()),
        None,
    )?;

    // Get the plan
    let retrieved = get_plan(workspace_root, &created.id)?;

    // Verify it matches
    assert_eq!(retrieved.id, created.id);
    assert_eq!(retrieved.title, created.title);
    assert_eq!(retrieved.content, created.content);
    assert_eq!(retrieved.status, created.status);

    Ok(())
}

#[test]
fn test_get_nonexistent_plan_fails() -> Result<()> {
    // Setup
    let temp = create_test_workspace()?;
    let workspace_root = temp.path();

    // Try to get non-existent plan
    let result = get_plan(workspace_root, "plan_nonexistent");

    assert!(result.is_err(), "Getting non-existent plan should fail");

    Ok(())
}

// ============================================================================
// LIST PLANS TESTS
// ============================================================================

#[test]
fn test_list_plans_empty() -> Result<()> {
    // Setup
    let temp = create_test_workspace()?;
    let workspace_root = temp.path();

    // List plans (should be empty)
    let plans = list_plans(workspace_root, None)?;

    assert_eq!(plans.len(), 0, "Should have no plans initially");

    Ok(())
}

#[test]
fn test_list_plans_returns_all() -> Result<()> {
    // Setup
    let temp = create_test_workspace()?;
    let workspace_root = temp.path();

    // Create multiple plans
    create_plan(workspace_root, "Plan 1".to_string(), None, None)?;
    create_plan(workspace_root, "Plan 2".to_string(), None, None)?;
    create_plan(workspace_root, "Plan 3".to_string(), None, None)?;

    // List all plans
    let plans = list_plans(workspace_root, None)?;

    assert_eq!(plans.len(), 3, "Should have 3 plans");

    Ok(())
}

#[test]
fn test_list_plans_filter_by_status() -> Result<()> {
    // Setup
    let temp = create_test_workspace()?;
    let workspace_root = temp.path();

    // Create plans with different statuses
    let plan1 = create_plan(workspace_root, "Active Plan".to_string(), None, None)?;
    let plan2 = create_plan(workspace_root, "Another Plan".to_string(), None, None)?;

    // Complete one plan
    let updates = PlanUpdates {
        status: Some(PlanStatus::Completed),
        ..Default::default()
    };
    update_plan(workspace_root, &plan2.id, updates)?;

    // List only active plans
    let active_plans = list_plans(workspace_root, Some(PlanStatus::Active))?;
    assert_eq!(active_plans.len(), 1, "Should have 1 active plan");
    assert_eq!(active_plans[0].id, plan1.id);

    // List only completed plans
    let completed_plans = list_plans(workspace_root, Some(PlanStatus::Completed))?;
    assert_eq!(completed_plans.len(), 1, "Should have 1 completed plan");
    assert_eq!(completed_plans[0].id, plan2.id);

    Ok(())
}

#[test]
fn test_list_plans_sorted_by_timestamp() -> Result<()> {
    // Setup
    let temp = create_test_workspace()?;
    let workspace_root = temp.path();

    // Create plans with delays to ensure different timestamps (timestamp is in seconds)
    let plan1 = create_plan(workspace_root, "First".to_string(), None, None)?;
    std::thread::sleep(std::time::Duration::from_secs(1));

    let plan2 = create_plan(workspace_root, "Second".to_string(), None, None)?;
    std::thread::sleep(std::time::Duration::from_secs(1));

    let plan3 = create_plan(workspace_root, "Third".to_string(), None, None)?;

    // List plans
    let plans = list_plans(workspace_root, None)?;

    // Verify order (most recent first)
    assert_eq!(plans[0].id, plan3.id, "Most recent should be first");
    assert_eq!(plans[1].id, plan2.id);
    assert_eq!(plans[2].id, plan1.id, "Oldest should be last");

    Ok(())
}

// ============================================================================
// ACTIVATE PLAN TESTS
// ============================================================================

#[test]
fn test_activate_plan_sets_active() -> Result<()> {
    // Setup
    let temp = create_test_workspace()?;
    let workspace_root = temp.path();

    // Create a completed plan
    let plan = create_plan(workspace_root, "To Activate".to_string(), None, None)?;
    let updates = PlanUpdates {
        status: Some(PlanStatus::Completed),
        ..Default::default()
    };
    update_plan(workspace_root, &plan.id, updates)?;

    // Activate it
    activate_plan(workspace_root, &plan.id)?;

    // Verify it's active
    let activated = get_plan(workspace_root, &plan.id)?;
    assert_eq!(activated.status, PlanStatus::Active);

    Ok(())
}

#[test]
fn test_activate_plan_deactivates_others() -> Result<()> {
    // Setup
    let temp = create_test_workspace()?;
    let workspace_root = temp.path();

    // Create multiple active plans
    let plan1 = create_plan(workspace_root, "Plan 1".to_string(), None, None)?;
    let plan2 = create_plan(workspace_root, "Plan 2".to_string(), None, None)?;
    let plan3 = create_plan(workspace_root, "Plan 3".to_string(), None, None)?;

    // All should be active initially
    assert_eq!(
        get_plan(workspace_root, &plan1.id)?.status,
        PlanStatus::Active
    );
    assert_eq!(
        get_plan(workspace_root, &plan2.id)?.status,
        PlanStatus::Active
    );
    assert_eq!(
        get_plan(workspace_root, &plan3.id)?.status,
        PlanStatus::Active
    );

    // Activate plan2
    activate_plan(workspace_root, &plan2.id)?;

    // Verify only plan2 is active
    assert_eq!(
        get_plan(workspace_root, &plan1.id)?.status,
        PlanStatus::Archived
    );
    assert_eq!(
        get_plan(workspace_root, &plan2.id)?.status,
        PlanStatus::Active
    );
    assert_eq!(
        get_plan(workspace_root, &plan3.id)?.status,
        PlanStatus::Archived
    );

    Ok(())
}

// ============================================================================
// COMPLETE PLAN TESTS
// ============================================================================

#[test]
fn test_complete_plan() -> Result<()> {
    // Setup
    let temp = create_test_workspace()?;
    let workspace_root = temp.path();

    // Create active plan
    let plan = create_plan(workspace_root, "To Complete".to_string(), None, None)?;
    assert_eq!(plan.status, PlanStatus::Active);

    // Complete it
    let completed = complete_plan(workspace_root, &plan.id)?;

    // Verify status
    assert_eq!(completed.status, PlanStatus::Completed);

    // Verify persisted
    let retrieved = get_plan(workspace_root, &plan.id)?;
    assert_eq!(retrieved.status, PlanStatus::Completed);

    Ok(())
}

// ============================================================================
// SLUG GENERATION TESTS
// ============================================================================

#[test]
fn test_slug_basic() -> Result<()> {
    // Setup
    let temp = create_test_workspace()?;
    let workspace_root = temp.path();

    // Test various titles
    let test_cases = vec![
        ("Add Search Feature", "plan_add-search-feature"),
        ("Fix: Auth Bug", "plan_fix-auth-bug"),
        ("Database Migration (v2)", "plan_database-migration-v2"),
        ("Update README", "plan_update-readme"),
        (
            "Refactor   Multiple   Spaces",
            "plan_refactor-multiple-spaces",
        ),
    ];

    for (title, expected_id) in test_cases {
        let plan = create_plan(workspace_root, title.to_string(), None, None)?;
        assert_eq!(
            plan.id, expected_id,
            "Title '{}' should generate ID '{}'",
            title, expected_id
        );
    }

    Ok(())
}

#[test]
fn test_slug_special_characters() -> Result<()> {
    // Setup
    let temp = create_test_workspace()?;
    let workspace_root = temp.path();

    // Special characters should be stripped/replaced
    let plan = create_plan(
        workspace_root,
        "Plan: Fix @mentions & #hashtags!".to_string(),
        None,
        None,
    )?;

    // Should have valid slug (only lowercase, hyphens, alphanumeric)
    assert!(plan.id.starts_with("plan_"));
    let slug = &plan.id[5..]; // Remove "plan_" prefix
    assert!(
        slug.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
        "Slug should only contain lowercase letters, digits, and hyphens: {}",
        slug
    );

    Ok(())
}

#[test]
fn test_slug_unicode() -> Result<()> {
    // Setup
    let temp = create_test_workspace()?;
    let workspace_root = temp.path();

    // Unicode should be handled gracefully (converted or stripped)
    let plan = create_plan(
        workspace_root,
        "Add café feature 🚀".to_string(),
        None,
        None,
    )?;

    // Should have valid ASCII slug
    assert!(plan.id.starts_with("plan_"));
    let slug = &plan.id[5..];
    assert!(slug.is_ascii(), "Slug should be ASCII-only: {}", slug);

    Ok(())
}

#[test]
fn test_empty_title_fails() -> Result<()> {
    // Setup
    let temp = create_test_workspace()?;
    let workspace_root = temp.path();

    // Empty title should fail
    let result = create_plan(workspace_root, "".to_string(), None, None);
    assert!(result.is_err(), "Empty title should fail");

    // Whitespace-only title should fail
    let result = create_plan(workspace_root, "   ".to_string(), None, None);
    assert!(result.is_err(), "Whitespace-only title should fail");

    Ok(())
}

// ============================================================================
// EDGE CASES
// ============================================================================

#[test]
fn test_plan_with_empty_content() -> Result<()> {
    // Setup
    let temp = create_test_workspace()?;
    let workspace_root = temp.path();

    // Create plan with None content
    let plan = create_plan(workspace_root, "No Content".to_string(), None, None)?;
    assert_eq!(plan.content, None);

    // Create plan with empty string content
    let plan2 = create_plan(
        workspace_root,
        "Empty Content".to_string(),
        Some("".to_string()),
        None,
    )?;
    assert_eq!(plan2.content, Some("".to_string()));

    Ok(())
}

#[test]
fn test_plan_extra_fields() -> Result<()> {
    // Setup
    let temp = create_test_workspace()?;
    let workspace_root = temp.path();

    // Create plan
    let mut plan = create_plan(workspace_root, "Extra Fields".to_string(), None, None)?;

    // Add extra fields
    plan.extra = serde_json::json!({
        "priority": "high",
        "tags": ["important", "urgent"],
        "estimate_hours": 8
    });

    // Update with extra fields
    let updates = PlanUpdates {
        extra: Some(plan.extra.clone()),
        ..Default::default()
    };

    let updated = update_plan(workspace_root, &plan.id, updates)?;

    // Verify extra fields preserved
    assert_eq!(updated.extra["priority"], "high");
    assert_eq!(
        updated.extra["tags"],
        serde_json::json!(["important", "urgent"])
    );
    assert_eq!(updated.extra["estimate_hours"], 8);

    Ok(())
}

// ============================================================================
// PLANACTION ENUM JSON SERIALIZATION TESTS
// ============================================================================

#[test]
fn test_plan_action_serializes_to_lowercase() -> Result<()> {
    // Test that PlanAction enum serializes to lowercase JSON
    let action = PlanAction::Save;
    let json = serde_json::to_string(&action)?;
    assert_eq!(
        json, "\"save\"",
        "PlanAction::Save should serialize to lowercase 'save'"
    );

    let action = PlanAction::Get;
    let json = serde_json::to_string(&action)?;
    assert_eq!(
        json, "\"get\"",
        "PlanAction::Get should serialize to lowercase 'get'"
    );

    let action = PlanAction::List;
    let json = serde_json::to_string(&action)?;
    assert_eq!(
        json, "\"list\"",
        "PlanAction::List should serialize to lowercase 'list'"
    );

    let action = PlanAction::Activate;
    let json = serde_json::to_string(&action)?;
    assert_eq!(
        json, "\"activate\"",
        "PlanAction::Activate should serialize to lowercase 'activate'"
    );

    let action = PlanAction::Update;
    let json = serde_json::to_string(&action)?;
    assert_eq!(
        json, "\"update\"",
        "PlanAction::Update should serialize to lowercase 'update'"
    );

    let action = PlanAction::Complete;
    let json = serde_json::to_string(&action)?;
    assert_eq!(
        json, "\"complete\"",
        "PlanAction::Complete should serialize to lowercase 'complete'"
    );

    Ok(())
}

#[test]
fn test_plan_action_deserializes_from_lowercase() -> Result<()> {
    // Test that PlanAction enum deserializes from lowercase JSON
    let action: PlanAction = serde_json::from_str("\"save\"")?;
    assert!(
        matches!(action, PlanAction::Save),
        "Should deserialize 'save' to PlanAction::Save"
    );

    let action: PlanAction = serde_json::from_str("\"get\"")?;
    assert!(
        matches!(action, PlanAction::Get),
        "Should deserialize 'get' to PlanAction::Get"
    );

    let action: PlanAction = serde_json::from_str("\"list\"")?;
    assert!(
        matches!(action, PlanAction::List),
        "Should deserialize 'list' to PlanAction::List"
    );

    let action: PlanAction = serde_json::from_str("\"activate\"")?;
    assert!(
        matches!(action, PlanAction::Activate),
        "Should deserialize 'activate' to PlanAction::Activate"
    );

    let action: PlanAction = serde_json::from_str("\"update\"")?;
    assert!(
        matches!(action, PlanAction::Update),
        "Should deserialize 'update' to PlanAction::Update"
    );

    let action: PlanAction = serde_json::from_str("\"complete\"")?;
    assert!(
        matches!(action, PlanAction::Complete),
        "Should deserialize 'complete' to PlanAction::Complete"
    );

    Ok(())
}

#[test]
fn test_plan_action_rejects_capitalized() {
    // Test that PlanAction enum REJECTS capitalized JSON (should fail with serde rename_all lowercase)
    let result = serde_json::from_str::<PlanAction>("\"Save\"");
    assert!(result.is_err(), "Should reject capitalized 'Save'");

    let result = serde_json::from_str::<PlanAction>("\"Get\"");
    assert!(result.is_err(), "Should reject capitalized 'Get'");

    let result = serde_json::from_str::<PlanAction>("\"List\"");
    assert!(result.is_err(), "Should reject capitalized 'List'");
}
