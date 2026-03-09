//! Tests for the agent dispatch backend.
//!
//! Tests cover:
//! - `AgentBackend` trait and `ClaudeBackend` implementation
//! - `is_available()` detection logic
//! - `assemble_context()` prompt assembly from search + memories
//! - `DispatchManager` lifecycle (create, update, list, history)
//! - Broadcast channel mechanism for streaming output
//! - `save_result_as_checkpoint()` persistence

use crate::agent::backend::{AgentBackend, BackendInfo};
use crate::agent::claude_backend::ClaudeBackend;
use crate::agent::context_assembly::{assemble_context, ContextHints};
use crate::agent::dispatch::{
    save_result_as_checkpoint, DispatchManager, DispatchSnapshot, DispatchStatus,
};

// ============================================================================
// AgentBackend trait + ClaudeBackend
// ============================================================================

#[test]
fn test_claude_backend_name() {
    let backend = ClaudeBackend::new();
    assert_eq!(backend.name(), "claude");
}

#[test]
fn test_claude_backend_is_available_checks_which() {
    // is_available() should return a bool without panicking.
    // We can't guarantee `claude` is installed in CI, but the method should work.
    let backend = ClaudeBackend::new();
    let _available = backend.is_available();
    // Just verify it doesn't panic — the result depends on the environment
}

#[test]
fn test_detect_backends_returns_claude() {
    use crate::agent::backend::detect_backends;
    let backends = detect_backends();
    assert!(!backends.is_empty(), "should detect at least the Claude backend");
    assert_eq!(backends[0].name, "claude");
    // `available` depends on whether `claude` CLI is installed
}

#[test]
fn test_backend_info_fields() {
    let info = BackendInfo {
        name: "test-backend".to_string(),
        available: true,
        version: Some("1.0.0".to_string()),
    };
    assert_eq!(info.name, "test-backend");
    assert!(info.available);
    assert_eq!(info.version.as_deref(), Some("1.0.0"));
}

#[test]
fn test_backend_info_serializable() {
    let info = BackendInfo {
        name: "claude".to_string(),
        available: true,
        version: Some("1.2.3".to_string()),
    };
    let json = serde_json::to_value(&info).expect("BackendInfo should serialize");
    assert_eq!(json["name"], "claude");
    assert_eq!(json["available"], true);
    assert_eq!(json["version"], "1.2.3");
}

// ============================================================================
// DispatchManager
// ============================================================================

#[tokio::test]
async fn test_dispatch_manager_create() {
    let manager = DispatchManager::new();
    assert!(manager.list_dispatches().is_empty());
    assert!(manager.backends().is_empty());
}

#[tokio::test]
async fn test_dispatch_manager_with_backends() {
    use crate::agent::backend::detect_backends;
    let backends = detect_backends();
    let manager = DispatchManager::with_backends(backends);
    assert!(!manager.backends().is_empty());
}

#[tokio::test]
async fn test_dispatch_manager_start_dispatch() {
    let mut manager = DispatchManager::new();
    let id = manager.start_dispatch(
        "Fix the bug in parser".to_string(),
        "julie".to_string(),
        "claude".to_string(),
    );

    assert!(id.starts_with("dispatch_"), "ID should start with dispatch_ prefix");
    assert_eq!(id.len(), "dispatch_".len() + 8, "ID should have 8 hex chars after prefix");

    let dispatch = manager.get_dispatch(&id).expect("dispatch should exist");
    assert_eq!(dispatch.task, "Fix the bug in parser");
    assert_eq!(dispatch.project, "julie");
    assert!(matches!(dispatch.status, DispatchStatus::Running));
    assert!(dispatch.output.is_empty());
    assert!(dispatch.error.is_none());
    assert!(dispatch.completed_at.is_none());
}

#[tokio::test]
async fn test_dispatch_manager_append_output() {
    let mut manager = DispatchManager::new();
    let id = manager.start_dispatch("task".to_string(), "proj".to_string(), "claude".to_string());

    manager.append_output(&id, "line 1\n");
    manager.append_output(&id, "line 2\n");

    let dispatch = manager.get_dispatch(&id).unwrap();
    assert_eq!(dispatch.output, "line 1\nline 2\n");
}

#[tokio::test]
async fn test_dispatch_manager_complete_dispatch() {
    let mut manager = DispatchManager::new();
    let id = manager.start_dispatch("task".to_string(), "proj".to_string(), "claude".to_string());
    manager.append_output(&id, "result output");

    manager.complete_dispatch(&id);

    let dispatch = manager.get_dispatch(&id).unwrap();
    assert!(matches!(dispatch.status, DispatchStatus::Completed));
    assert!(dispatch.completed_at.is_some());
}

#[tokio::test]
async fn test_dispatch_manager_fail_dispatch() {
    let mut manager = DispatchManager::new();
    let id = manager.start_dispatch("task".to_string(), "proj".to_string(), "claude".to_string());

    manager.fail_dispatch(&id, "process exited with code 1");

    let dispatch = manager.get_dispatch(&id).unwrap();
    assert!(matches!(dispatch.status, DispatchStatus::Failed));
    assert_eq!(dispatch.error.as_deref(), Some("process exited with code 1"));
    assert!(dispatch.completed_at.is_some());
}

#[tokio::test]
async fn test_dispatch_manager_list_dispatches_sorted() {
    let mut manager = DispatchManager::new();
    // Start dispatches with slight time differences (same millisecond is possible,
    // so we just verify count and that the sort doesn't crash)
    let id1 = manager.start_dispatch("task 1".to_string(), "proj".to_string(), "claude".to_string());
    let id2 = manager.start_dispatch("task 2".to_string(), "proj".to_string(), "claude".to_string());

    let dispatches = manager.list_dispatches();
    assert_eq!(dispatches.len(), 2);

    // Both should be present
    let ids: Vec<&str> = dispatches.iter().map(|d| d.id.as_str()).collect();
    assert!(ids.contains(&id1.as_str()));
    assert!(ids.contains(&id2.as_str()));

    // Sorted by started_at descending — newest first
    assert!(
        dispatches[0].started_at >= dispatches[1].started_at,
        "list_dispatches should return newest first"
    );
}

#[tokio::test]
async fn test_dispatch_manager_get_nonexistent() {
    let manager = DispatchManager::new();
    assert!(manager.get_dispatch("dispatch_00000000").is_none());
}

#[tokio::test]
async fn test_dispatch_id_uniqueness() {
    let mut manager = DispatchManager::new();
    let id1 = manager.start_dispatch("task 1".to_string(), "proj".to_string(), "claude".to_string());
    let id2 = manager.start_dispatch("task 2".to_string(), "proj".to_string(), "claude".to_string());
    assert_ne!(id1, id2, "dispatch IDs should be unique");
}

#[tokio::test]
async fn test_dispatch_broadcast_channel() {
    let mut manager = DispatchManager::new();
    let id = manager.start_dispatch("task".to_string(), "proj".to_string(), "claude".to_string());

    // Subscribe to the broadcast channel
    let mut rx = manager.subscribe(&id).expect("should get receiver");

    // Append output (which also broadcasts)
    manager.append_output(&id, "hello world\n");

    // The subscriber should receive the line
    let received = rx.recv().await.expect("should receive broadcast");
    assert_eq!(received, "hello world\n");
}

#[tokio::test]
async fn test_dispatch_broadcast_multiple_subscribers() {
    let mut manager = DispatchManager::new();
    let id = manager.start_dispatch("task".to_string(), "proj".to_string(), "claude".to_string());

    let mut rx1 = manager.subscribe(&id).expect("sub 1");
    let mut rx2 = manager.subscribe(&id).expect("sub 2");

    manager.append_output(&id, "data\n");

    let r1 = rx1.recv().await.expect("rx1 should receive");
    let r2 = rx2.recv().await.expect("rx2 should receive");
    assert_eq!(r1, "data\n");
    assert_eq!(r2, "data\n");
}

#[test]
fn test_dispatch_status_display() {
    assert_eq!(DispatchStatus::Running.as_str(), "running");
    assert_eq!(DispatchStatus::Completed.as_str(), "completed");
    assert_eq!(DispatchStatus::Failed.as_str(), "failed");
}

#[test]
fn test_dispatch_status_serializable() {
    let json = serde_json::to_value(DispatchStatus::Running).unwrap();
    assert_eq!(json, "running");
    let json = serde_json::to_value(DispatchStatus::Completed).unwrap();
    assert_eq!(json, "completed");
    let json = serde_json::to_value(DispatchStatus::Failed).unwrap();
    assert_eq!(json, "failed");
}

#[test]
fn test_agent_dispatch_serializable() {
    let mut manager = DispatchManager::new();
    let id = manager.start_dispatch("my task".to_string(), "proj".to_string(), "claude".to_string());
    let dispatch = manager.get_dispatch(&id).unwrap();
    let json = serde_json::to_value(dispatch).expect("AgentDispatch should serialize");
    assert_eq!(json["task"], "my task");
    assert_eq!(json["project"], "proj");
    assert_eq!(json["status"], "running");
    // broadcast_tx should be skipped
    assert!(json.get("broadcast_tx").is_none());
}

// ============================================================================
// Context Assembly
// ============================================================================

#[tokio::test]
async fn test_assemble_context_with_no_workspace() {
    // When no workspace/search_index is provided, context should still include the task
    let context = assemble_context(
        None, // no workspace root
        None, // no search index
        "Implement the new feature",
        None,
    )
    .await
    .expect("should succeed without workspace");

    assert!(context.contains("# Task"), "should have Task section");
    assert!(
        context.contains("Implement the new feature"),
        "should contain the task description"
    );
}

#[tokio::test]
async fn test_assemble_context_includes_task_section() {
    let context = assemble_context(None, None, "Fix the parser bug", None)
        .await
        .unwrap();

    // Should have the structured sections
    assert!(context.contains("# Task"));
    assert!(context.contains("Fix the parser bug"));
}

#[tokio::test]
async fn test_assemble_context_with_hints() {
    let hints = ContextHints {
        files: Some(vec!["src/main.rs".to_string(), "src/lib.rs".to_string()]),
        symbols: Some(vec!["SearchIndex".to_string()]),
        extra_context: Some("This is a Rust project using Tantivy".to_string()),
    };

    let context = assemble_context(None, None, "Optimize search", Some(hints))
        .await
        .unwrap();

    assert!(context.contains("# Task"));
    assert!(context.contains("Optimize search"));
    // Hints should be included
    assert!(context.contains("src/main.rs"), "should include hinted files");
    assert!(context.contains("SearchIndex"), "should include hinted symbols");
    assert!(
        context.contains("This is a Rust project using Tantivy"),
        "should include extra context"
    );
}

#[tokio::test]
async fn test_assemble_context_with_search_index() {
    use crate::search::{LanguageConfigs, SearchIndex, SymbolDocument};
    use std::sync::{Arc, Mutex};

    let temp_dir = tempfile::tempdir().unwrap();
    let tantivy_dir = temp_dir.path().join("tantivy");
    std::fs::create_dir_all(&tantivy_dir).unwrap();

    let configs = LanguageConfigs::load_embedded();
    let search_index =
        SearchIndex::create_with_language_configs(&tantivy_dir, &configs).unwrap();

    // Add a test symbol
    search_index
        .add_symbol(&SymbolDocument {
            id: "sym-1".to_string(),
            name: "parse_expression".to_string(),
            signature: "fn parse_expression(input: &str) -> Expr".to_string(),
            doc_comment: "Parse an expression from string input.".to_string(),
            file_path: "src/parser.rs".to_string(),
            kind: "function".to_string(),
            language: "rust".to_string(),
            start_line: 42,
            code_body: "fn parse_expression(input: &str) -> Expr { todo!() }".to_string(),
        })
        .unwrap();
    search_index.commit().unwrap();

    let index = Arc::new(Mutex::new(search_index));

    let context = assemble_context(
        None,
        Some(&index),
        "Fix the parser expression handling",
        None,
    )
    .await
    .unwrap();

    assert!(context.contains("# Task"));
    assert!(context.contains("Fix the parser expression handling"));
    // Should find the symbol via search
    assert!(
        context.contains("parse_expression"),
        "should include search results from the provided SearchIndex"
    );
    assert!(context.contains("## Relevant Code"));
}

#[tokio::test]
async fn test_assemble_context_with_workspace_memories() {
    // Create a temp workspace with .memories for recall
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace_root = temp_dir.path();

    // Create a checkpoint so recall has something to find
    let memories_dir = workspace_root.join(".memories").join("2026-03-07");
    std::fs::create_dir_all(&memories_dir).unwrap();
    std::fs::write(
        memories_dir.join("120000_abcd.md"),
        r#"---
id: checkpoint_abcd1234
timestamp: "2026-03-07T12:00:00.000Z"
summary: Fixed the parser edge case
---
# Fixed the parser edge case

The parser was failing on empty input. Added a guard clause.
"#,
    )
    .unwrap();

    let context = assemble_context(
        Some(workspace_root),
        None, // no search index
        "Review the parser changes",
        None,
    )
    .await
    .unwrap();

    assert!(context.contains("# Task"));
    assert!(context.contains("Review the parser changes"));
    // The key thing is it doesn't crash with a real workspace path
}

#[tokio::test]
async fn test_assemble_context_format_structure() {
    let context = assemble_context(None, None, "My task", None).await.unwrap();

    // Verify the output has the expected structure
    let lines: Vec<&str> = context.lines().collect();
    assert!(
        lines.iter().any(|l| l.starts_with("# Context")),
        "should have Context header"
    );
    assert!(
        lines.iter().any(|l| l.starts_with("# Task")),
        "should have Task header"
    );
}

// ============================================================================
// save_result_as_checkpoint
// ============================================================================

#[tokio::test]
async fn test_save_result_as_checkpoint_completed() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace_root = temp_dir.path();

    let mut manager = DispatchManager::new();
    let id = manager.start_dispatch(
        "Refactor the parser".to_string(),
        "julie".to_string(),
        "claude".to_string(),
    );
    manager.append_output(&id, "Done. Refactored 3 files.\n");
    manager.complete_dispatch(&id);

    let dispatch = manager.get_dispatch(&id).unwrap();
    let snapshot = DispatchSnapshot::from(dispatch);
    let checkpoint = save_result_as_checkpoint(workspace_root, &snapshot, "claude")
        .await
        .expect("should save checkpoint");

    assert!(checkpoint.id.starts_with("checkpoint_"));
    assert!(checkpoint.description.contains("Refactor the parser"));
    assert!(checkpoint.description.contains("Done. Refactored 3 files."));

    let tags = checkpoint.tags.expect("should have tags");
    assert!(tags.contains(&"agent_result".to_string()));
    assert!(tags.contains(&"claude".to_string()));
    assert!(tags.contains(&"julie".to_string()));

    assert_eq!(
        checkpoint.impact.as_deref(),
        Some("Agent task completed successfully")
    );
    assert_eq!(
        checkpoint.context.as_deref(),
        Some("Dispatched to claude backend")
    );
}

#[tokio::test]
async fn test_save_result_as_checkpoint_failed() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace_root = temp_dir.path();

    let mut manager = DispatchManager::new();
    let id = manager.start_dispatch("Bad task".to_string(), "proj".to_string(), "claude".to_string());
    manager.fail_dispatch(&id, "Backend crashed");

    let dispatch = manager.get_dispatch(&id).unwrap();
    let snapshot = DispatchSnapshot::from(dispatch);
    let checkpoint = save_result_as_checkpoint(workspace_root, &snapshot, "claude")
        .await
        .expect("should save checkpoint even for failures");

    assert_eq!(checkpoint.impact.as_deref(), Some("Backend crashed"));
}

// ============================================================================
// Dispatch lifecycle (end-to-end without subprocess)
// ============================================================================

#[tokio::test]
async fn test_dispatch_lifecycle_happy_path() {
    let mut manager = DispatchManager::new();

    // 1. Start dispatch
    let id = manager.start_dispatch(
        "Refactor the parser module".to_string(),
        "julie".to_string(),
        "claude".to_string(),
    );
    assert!(matches!(
        manager.get_dispatch(&id).unwrap().status,
        DispatchStatus::Running
    ));

    // 2. Simulate output streaming
    manager.append_output(&id, "Analyzing codebase...\n");
    manager.append_output(&id, "Found 3 files to refactor.\n");
    manager.append_output(&id, "Refactoring complete.\n");

    // 3. Complete
    manager.complete_dispatch(&id);

    let dispatch = manager.get_dispatch(&id).unwrap();
    assert!(matches!(dispatch.status, DispatchStatus::Completed));
    assert_eq!(
        dispatch.output,
        "Analyzing codebase...\nFound 3 files to refactor.\nRefactoring complete.\n"
    );
    assert!(dispatch.completed_at.is_some());
}

#[tokio::test]
async fn test_dispatch_lifecycle_failure() {
    let mut manager = DispatchManager::new();

    let id = manager.start_dispatch("Bad task".to_string(), "proj".to_string(), "claude".to_string());
    manager.append_output(&id, "Starting...\n");
    manager.fail_dispatch(&id, "Backend not available");

    let dispatch = manager.get_dispatch(&id).unwrap();
    assert!(matches!(dispatch.status, DispatchStatus::Failed));
    assert_eq!(dispatch.error.as_deref(), Some("Backend not available"));
    assert_eq!(dispatch.output, "Starting...\n");
}
