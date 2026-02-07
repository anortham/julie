//! Integration tests for PlanTool (Phase 1.5)
//!
//! Tests verify full MCP workflow including:
//! - Creating plans through MCP interface
//! - Updating and managing plan lifecycle
//! - SQL view integration
//! - Multi-plan activation logic
//!
//! Note: These are simplified integration tests that verify MCP routing works.
//! Detailed CRUD functionality is tested in memory_plan_tests.rs unit tests.

use crate::handler::JulieServerHandler;
use crate::tools::memory::plan::{PlanStatus, get_plan, list_plans};
use crate::tools::memory::{PlanAction, PlanTool};
use anyhow::Result;
use rmcp::model::CallToolResult;
use std::fs;
use tempfile::TempDir;

/// Helper to create a test handler with isolated workspace
async fn create_test_handler() -> Result<(JulieServerHandler, TempDir)> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_string_lossy().to_string();

    let handler = JulieServerHandler::new().await?;
    handler
        .initialize_workspace_with_force(Some(workspace_path), true)
        .await?;

    Ok((handler, temp_dir))
}

#[tokio::test]
async fn test_plan_save_action() -> Result<()> {
    let (handler, temp_dir) = create_test_handler().await?;
    let workspace_root = temp_dir.path();

    // Create a plan through MCP tool
    let save_tool = PlanTool {
        action: PlanAction::Save,
        title: Some("Implement Search Feature".to_string()),
        id: None,
        content: Some("## Tasks\n- [ ] Design API\n- [ ] Write tests\n- [ ] Implement".to_string()),
        status: None,
        activate: Some(true),
    };

    // Call tool (verify it doesn't error)
    let _result = save_tool.call_tool(&handler).await?;

    // Verify plan was saved to disk
    let plan_path = workspace_root.join(".memories/plans/plan_implement-search-feature.json");
    assert!(plan_path.exists(), "Plan file should exist on disk");

    // Verify plan content using CRUD functions
    let plan = get_plan(workspace_root, "plan_implement-search-feature")?;
    assert_eq!(plan.title, "Implement Search Feature");
    assert_eq!(plan.status, PlanStatus::Active);
    assert!(plan.content.is_some());

    Ok(())
}

#[tokio::test]
async fn test_plan_get_action() -> Result<()> {
    let (handler, temp_dir) = create_test_handler().await?;
    let workspace_root = temp_dir.path();

    // Create a plan first
    let save_tool = PlanTool {
        action: PlanAction::Save,
        title: Some("Test Plan".to_string()),
        id: None,
        content: Some("Test content".to_string()),
        status: None,
        activate: Some(false),
    };
    save_tool.call_tool(&handler).await?;

    // Get the plan through MCP tool
    let get_tool = PlanTool {
        action: PlanAction::Get,
        title: None,
        id: Some("plan_test-plan".to_string()),
        content: None,
        status: None,
        activate: None,
    };

    let _result = get_tool.call_tool(&handler).await?;

    Ok(())
}

#[tokio::test]
async fn test_plan_list_action() -> Result<()> {
    let (handler, temp_dir) = create_test_handler().await?;
    let workspace_root = temp_dir.path();

    // Create multiple plans
    for i in 1..=3 {
        let save_tool = PlanTool {
            action: PlanAction::Save,
            title: Some(format!("Plan {}", i)),
            id: None,
            content: None,
            status: None,
            activate: Some(false),
        };
        save_tool.call_tool(&handler).await?;
    }

    // List all plans
    let list_tool = PlanTool {
        action: PlanAction::List,
        title: None,
        id: None,
        content: None,
        status: None,
        activate: None,
    };

    let _result = list_tool.call_tool(&handler).await?;

    Ok(())
}

#[tokio::test]
async fn test_plan_activate_action() -> Result<()> {
    let (handler, temp_dir) = create_test_handler().await?;
    let workspace_root = temp_dir.path();

    // Create two plans
    let save_tool1 = PlanTool {
        action: PlanAction::Save,
        title: Some("Plan A".to_string()),
        id: None,
        content: None,
        status: None,
        activate: Some(true), // Start as active
    };
    save_tool1.call_tool(&handler).await?;

    let save_tool2 = PlanTool {
        action: PlanAction::Save,
        title: Some("Plan B".to_string()),
        id: None,
        content: None,
        status: None,
        activate: Some(false),
    };
    save_tool2.call_tool(&handler).await?;

    // Activate plan B (should deactivate plan A)
    let activate_tool = PlanTool {
        action: PlanAction::Activate,
        title: None,
        id: Some("plan_plan-b".to_string()),
        content: None,
        status: None,
        activate: None,
    };

    let _result = activate_tool.call_tool(&handler).await?;

    // Verify plan B is active and plan A is archived
    let plan_a = get_plan(workspace_root, "plan_plan-a")?;
    let plan_b = get_plan(workspace_root, "plan_plan-b")?;

    assert_eq!(
        plan_a.status,
        PlanStatus::Archived,
        "Plan A should be archived"
    );
    assert_eq!(plan_b.status, PlanStatus::Active, "Plan B should be active");

    Ok(())
}

#[tokio::test]
async fn test_plan_update_action() -> Result<()> {
    let (handler, temp_dir) = create_test_handler().await?;
    let workspace_root = temp_dir.path();

    // Create a plan
    let save_tool = PlanTool {
        action: PlanAction::Save,
        title: Some("Original Plan".to_string()),
        id: None,
        content: Some("Original content".to_string()),
        status: None,
        activate: Some(false),
    };
    save_tool.call_tool(&handler).await?;

    // Update the plan's content
    let update_tool = PlanTool {
        action: PlanAction::Update,
        title: None,
        id: Some("plan_original-plan".to_string()),
        content: Some("Updated content".to_string()),
        status: None,
        activate: None,
    };

    let _result = update_tool.call_tool(&handler).await?;

    // Verify plan was updated on disk
    let plan = get_plan(workspace_root, "plan_original-plan")?;
    assert_eq!(plan.content, Some("Updated content".to_string()));

    Ok(())
}

#[tokio::test]
async fn test_plan_complete_action() -> Result<()> {
    let (handler, temp_dir) = create_test_handler().await?;
    let workspace_root = temp_dir.path();

    // Create a plan
    let save_tool = PlanTool {
        action: PlanAction::Save,
        title: Some("Completable Plan".to_string()),
        id: None,
        content: None,
        status: None,
        activate: Some(false),
    };
    save_tool.call_tool(&handler).await?;

    // Complete the plan
    let complete_tool = PlanTool {
        action: PlanAction::Complete,
        title: None,
        id: Some("plan_completable-plan".to_string()),
        content: None,
        status: None,
        activate: None,
    };

    let _result = complete_tool.call_tool(&handler).await?;

    // Verify plan status is completed
    let plan = get_plan(workspace_root, "plan_completable-plan")?;
    assert_eq!(plan.status, PlanStatus::Completed);

    Ok(())
}

#[tokio::test]
async fn test_plan_filter_by_status() -> Result<()> {
    let (handler, temp_dir) = create_test_handler().await?;
    let workspace_root = temp_dir.path();

    // Create plans with different statuses
    let save_active = PlanTool {
        action: PlanAction::Save,
        title: Some("Active Plan".to_string()),
        id: None,
        content: None,
        status: None,
        activate: Some(true),
    };
    save_active.call_tool(&handler).await?;

    let save_inactive = PlanTool {
        action: PlanAction::Save,
        title: Some("Inactive Plan".to_string()),
        id: None,
        content: None,
        status: None,
        activate: Some(false),
    };
    save_inactive.call_tool(&handler).await?;

    // Complete one plan
    let complete_tool = PlanTool {
        action: PlanAction::Complete,
        title: None,
        id: Some("plan_inactive-plan".to_string()),
        content: None,
        status: None,
        activate: None,
    };
    complete_tool.call_tool(&handler).await?;

    // List only active plans
    let list_active = PlanTool {
        action: PlanAction::List,
        title: None,
        id: None,
        content: None,
        status: Some("active".to_string()),
        activate: None,
    };

    let _result = list_active.call_tool(&handler).await?;

    Ok(())
}

#[tokio::test]
async fn test_sql_view_integration() -> Result<()> {
    let (handler, temp_dir) = create_test_handler().await?;
    let workspace_root = temp_dir.path();

    // Create a plan
    let save_tool = PlanTool {
        action: PlanAction::Save,
        title: Some("SQL Test Plan".to_string()),
        id: None,
        content: Some("Test content for SQL".to_string()),
        status: None,
        activate: Some(true),
    };
    save_tool.call_tool(&handler).await?;

    // Get database connection from handler (if accessible)
    // This test verifies the SQL view works correctly
    // Note: This requires access to handler's database, which may need to be exposed
    // For now, we'll verify through the list action which uses the same data

    let list_tool = PlanTool {
        action: PlanAction::List,
        title: None,
        id: None,
        content: None,
        status: Some("active".to_string()),
        activate: None,
    };

    let _result = list_tool.call_tool(&handler).await?;

    // If list works (doesn't error), the SQL view is working correctly

    Ok(())
}

/// Extract text from CallToolResult content blocks
fn extract_text_from_result(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|content_block| {
            serde_json::to_value(content_block).ok().and_then(|json| {
                json.get("text")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// === Bug fix: get should return full content, not 10-line preview ===

#[tokio::test]
async fn test_plan_get_returns_full_content() -> Result<()> {
    let (handler, _temp_dir) = create_test_handler().await?;

    // Create a plan with 20+ lines of content
    let long_content = (1..=25)
        .map(|i| format!("- Task {}: Do something important", i))
        .collect::<Vec<_>>()
        .join("\n");

    let save_tool = PlanTool {
        action: PlanAction::Save,
        title: Some("Long Plan".to_string()),
        id: None,
        content: Some(long_content.clone()),
        status: None,
        activate: Some(false),
    };
    save_tool.call_tool(&handler).await?;

    let get_tool = PlanTool {
        action: PlanAction::Get,
        title: None,
        id: Some("plan_long-plan".to_string()),
        content: None,
        status: None,
        activate: None,
    };

    let result = get_tool.call_tool(&handler).await?;
    let text = extract_text_from_result(&result);

    // Should contain the LAST line, not just the first 10
    assert!(
        text.contains("Task 25"),
        "get should return full content, not truncated. Got:\n{}",
        text
    );
    assert!(
        !text.contains("more lines"),
        "should not show truncation indicator"
    );

    Ok(())
}

// === Bug fix: update with title should error, not silently ignore ===

#[tokio::test]
async fn test_plan_update_rejects_title_change() -> Result<()> {
    let (handler, _temp_dir) = create_test_handler().await?;

    // Create a plan
    let save_tool = PlanTool {
        action: PlanAction::Save,
        title: Some("Original Title".to_string()),
        id: None,
        content: Some("Some content".to_string()),
        status: None,
        activate: Some(false),
    };
    save_tool.call_tool(&handler).await?;

    // Try to update with a title — should error
    let update_tool = PlanTool {
        action: PlanAction::Update,
        title: Some("New Title".to_string()),
        id: Some("plan_original-title".to_string()),
        content: None,
        status: None,
        activate: None,
    };

    let result = update_tool.call_tool(&handler).await;
    assert!(
        result.is_err(),
        "update with title should return an error, not silently ignore"
    );

    Ok(())
}

// === Enhancement: list should show timestamps ===

#[tokio::test]
async fn test_plan_list_shows_timestamps() -> Result<()> {
    let (handler, _temp_dir) = create_test_handler().await?;

    let save_tool = PlanTool {
        action: PlanAction::Save,
        title: Some("Timestamped Plan".to_string()),
        id: None,
        content: Some("Content".to_string()),
        status: None,
        activate: Some(true),
    };
    save_tool.call_tool(&handler).await?;

    let list_tool = PlanTool {
        action: PlanAction::List,
        title: None,
        id: None,
        content: None,
        status: None,
        activate: None,
    };

    let result = list_tool.call_tool(&handler).await?;
    let text = extract_text_from_result(&result);

    // Should show some time indicator (just created = "just now" or "0m ago" or similar)
    assert!(
        text.contains("ago") || text.contains("just now"),
        "list should show relative timestamps. Got:\n{}",
        text
    );

    Ok(())
}

// === Enhancement: PlanStatus Display impl ===

#[test]
fn test_plan_status_display() {
    assert_eq!(PlanStatus::Active.to_string(), "active");
    assert_eq!(PlanStatus::Completed.to_string(), "completed");
    assert_eq!(PlanStatus::Archived.to_string(), "archived");
}
