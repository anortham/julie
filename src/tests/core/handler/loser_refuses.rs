//! T7 gate-invariant test: loser (in-process follower) refuses D1 write-mutating
//! operations with a graceful error; reads remain functional.
//!
//! Gate invariant:
//!   loser  → manage_workspace(index, force=true) returns graceful refusal
//!   loser  → edit_file returns graceful refusal
//!   loser  → manage_workspace(stats) is NOT refused (reads work)
//!   leader → manage_workspace(index) is NOT refused (writes work on leader)
//!   loser  → is_in_process_follower() == true (metrics_tx gate suppresses write)
//!   leader → is_in_process_follower() == false (metrics_tx write proceeds)

use super::*;
use crate::daemon::discovery::DaemonLockGuard;
use crate::handler::JulieServerHandler;
use crate::leadership::LeadershipState;
use crate::tools::workspace::ManageWorkspaceTool;
use crate::workspace::startup_hint::{WorkspaceStartupHint, WorkspaceStartupSource};

const REFUSAL_MSG: &str = "read-only follower";

/// Build a leader handler for the given workspace dir.
/// The `DaemonLockGuard` moves into `LeadershipState::leader` and is held for
/// the handler's lifetime — no need to return it separately.
async fn make_leader(workspace_dir: &tempfile::TempDir) -> JulieServerHandler {
    let lock_path = workspace_dir.path().join(".leader.lock");
    let guard = DaemonLockGuard::try_acquire(&lock_path)
        .expect("lock must be acquirable on a fresh path");
    let hint = WorkspaceStartupHint {
        path: workspace_dir.path().to_path_buf(),
        source: Some(WorkspaceStartupSource::Cli),
    };
    JulieServerHandler::new_in_process(hint, None, LeadershipState::leader(guard))
        .await
        .expect("leader handler construction must succeed")
}

/// Build a follower (in-process loser) handler for the given workspace dir.
async fn make_follower(workspace_dir: &tempfile::TempDir) -> JulieServerHandler {
    let hint = WorkspaceStartupHint {
        path: workspace_dir.path().to_path_buf(),
        source: Some(WorkspaceStartupSource::Cli),
    };
    JulieServerHandler::new_in_process(hint, None, LeadershipState::follower())
        .await
        .expect("follower handler construction must succeed")
}

// ---------------------------------------------------------------------------
// Part A: write-tool refusal on follower
// ---------------------------------------------------------------------------

/// Gate invariant: follower refuses manage_workspace(index, force=true) gracefully.
#[tokio::test]
async fn test_loser_refuses_manage_workspace_index() {
    let workspace_dir = tempfile::tempdir().unwrap();
    let follower = make_follower(&workspace_dir).await;

    assert!(
        follower.is_in_process_follower(),
        "follower must be detected as in-process follower"
    );

    let tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        force: Some(true),
        path: Some(workspace_dir.path().to_string_lossy().to_string()),
        workspace_id: None,
        name: None,
        detailed: None,
    };
    // call_tool returns Ok(...) — refusal is a tool-level error, not a Rust Err.
    let result = tool
        .call_tool(&follower)
        .await
        .expect("call_tool must not panic — refusal is a graceful error");

    let content_text = format!("{:?}", result.content);
    assert!(
        content_text.contains(REFUSAL_MSG),
        "follower must refuse manage_workspace(index) with '{}', got: {}",
        REFUSAL_MSG,
        content_text
    );
}

/// Gate invariant: follower refuses edit_file gracefully (MCP error, not panic).
#[tokio::test(flavor = "multi_thread")]
async fn test_loser_refuses_edit_file() {
    let workspace_dir = tempfile::tempdir().unwrap();
    let follower = make_follower(&workspace_dir).await;

    // edit_file returns Err(McpError) on refusal.
    // call_public_tool returns Ok(false) when the handler returns Err.
    let tool_ok = call_public_tool(
        &follower,
        "edit_file",
        serde_json::json!({
            "file_path": "test.rs",
            "old_text": "foo",
            "new_text": "bar",
            "dry_run": false,
        }),
        1,
    )
    .await
    .unwrap();

    assert!(
        !tool_ok,
        "follower must refuse edit_file (call_public_tool returns false on McpError refusal)"
    );
}

/// Gate invariant: follower does NOT refuse manage_workspace(stats) — reads work.
#[tokio::test]
async fn test_loser_allows_reads_stats() {
    let workspace_dir = tempfile::tempdir().unwrap();
    let follower = make_follower(&workspace_dir).await;

    let tool = ManageWorkspaceTool {
        operation: "stats".to_string(),
        force: None,
        path: None,
        workspace_id: None,
        name: None,
        detailed: None,
    };
    // Must not panic and must not return the refusal message.
    let result = tool
        .call_tool(&follower)
        .await
        .expect("follower must not refuse stats — reads must remain functional");

    let content_text = format!("{:?}", result.content);
    assert!(
        !content_text.contains(REFUSAL_MSG),
        "follower must NOT return refusal message for stats, got: {}",
        content_text
    );
}

/// Gate invariant: leader is NOT refused for manage_workspace(index).
#[tokio::test]
async fn test_leader_not_refused_index() {
    let workspace_dir = tempfile::tempdir().unwrap();
    let leader = make_leader(&workspace_dir).await;

    assert!(leader.is_leader(), "leader must report is_leader()");
    assert!(
        !leader.is_in_process_follower(),
        "leader must not be in_process_follower"
    );

    let tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        force: Some(true),
        path: Some(workspace_dir.path().to_string_lossy().to_string()),
        workspace_id: None,
        name: None,
        detailed: None,
    };
    // Leader may succeed or fail for unrelated reasons (no source files, etc.),
    // but it must NOT return the follower refusal message.
    let result = tool.call_tool(&leader).await;
    let no_refusal = match &result {
        Ok(r) => {
            let text = format!("{:?}", r.content);
            !text.contains(REFUSAL_MSG)
        }
        Err(_) => true, // Rust-level Err is fine; not a follower refusal
    };
    assert!(
        no_refusal,
        "leader must not receive follower-refusal message: {:?}",
        result
    );
}

// ---------------------------------------------------------------------------
// Part B: metrics_tx persistent-write gate (structural assertion)
// ---------------------------------------------------------------------------

/// Gate invariant: is_in_process_follower() flags are set correctly so the
/// `if !self.is_in_process_follower()` gate in record_tool_call_outcome fires
/// for followers (suppressing metrics_tx persistent write) but not for leaders.
#[tokio::test]
async fn test_metrics_gate_leadership_flags() {
    let workspace_dir = tempfile::tempdir().unwrap();
    let leader = make_leader(&workspace_dir).await;
    let follower = make_follower(&workspace_dir).await;

    // Leader: `!is_in_process_follower()` == true → metrics write proceeds.
    assert!(
        !leader.is_in_process_follower(),
        "leader: metrics gate must allow persistent write (!is_in_process_follower() == true)"
    );

    // Follower: `!is_in_process_follower()` == false → metrics write suppressed.
    assert!(
        follower.is_in_process_follower(),
        "follower: metrics gate must suppress persistent write (!is_in_process_follower() == false)"
    );
}
