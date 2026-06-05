use crate::handler::{dispatch_with_deadline, is_write_exempt, parse_request_timeout};
use rmcp::ErrorData as McpError;
use rmcp::model::CallToolResult;
use std::time::Duration;

// ---------------------------------------------------------------------------
// parse_request_timeout — pure helper, synchronous tests
// ---------------------------------------------------------------------------

#[test]
fn test_parse_request_timeout_none_gives_default() {
    let t = parse_request_timeout(None);
    assert_eq!(
        t,
        Some(Duration::from_secs(120)),
        "unset env var must use the 120s default"
    );
}

#[test]
fn test_parse_request_timeout_zero_disables() {
    let t = parse_request_timeout(Some("0".into()));
    assert!(t.is_none(), r#""0" must disable the deadline (no timeout)"#);
}

#[test]
fn test_parse_request_timeout_garbage_gives_default() {
    let t = parse_request_timeout(Some("not-a-number".into()));
    assert_eq!(
        t,
        Some(Duration::from_secs(120)),
        "unparseable value must fall back to 120s default"
    );
}

#[test]
fn test_parse_request_timeout_valid_secs() {
    let t = parse_request_timeout(Some("30".into()));
    assert_eq!(t, Some(Duration::from_secs(30)));
}

// ---------------------------------------------------------------------------
// is_write_exempt — operation-aware exemption logic
// ---------------------------------------------------------------------------

fn args(op: &str) -> serde_json::Map<String, serde_json::Value> {
    let mut m = serde_json::Map::new();
    m.insert("operation".into(), serde_json::Value::String(op.into()));
    m
}

#[test]
fn test_is_write_exempt_edit_file_is_exempt() {
    assert!(is_write_exempt("edit_file", None), "edit_file must always be exempt");
}

#[test]
fn test_is_write_exempt_rename_symbol_is_exempt() {
    assert!(is_write_exempt("rename_symbol", None), "rename_symbol must always be exempt");
}

#[test]
fn test_is_write_exempt_rewrite_symbol_is_exempt() {
    assert!(is_write_exempt("rewrite_symbol", None), "rewrite_symbol must always be exempt");
}

#[test]
fn test_is_write_exempt_fast_search_is_not_exempt() {
    assert!(!is_write_exempt("fast_search", None), "fast_search must be deadline-bounded");
}

#[test]
fn test_is_write_exempt_manage_workspace_index_is_exempt() {
    let a = args("index");
    assert!(
        is_write_exempt("manage_workspace", Some(&a)),
        "manage_workspace index (mutating) must be exempt"
    );
}

#[test]
fn test_is_write_exempt_manage_workspace_stats_is_not_exempt() {
    let a = args("stats");
    assert!(
        !is_write_exempt("manage_workspace", Some(&a)),
        "manage_workspace stats (read-only) must be deadline-bounded"
    );
}

#[test]
fn test_is_write_exempt_manage_workspace_list_is_not_exempt() {
    let a = args("list");
    assert!(
        !is_write_exempt("manage_workspace", Some(&a)),
        "manage_workspace list (read-only) must be deadline-bounded"
    );
}

#[test]
fn test_is_write_exempt_manage_workspace_health_is_not_exempt() {
    let a = args("health");
    assert!(
        !is_write_exempt("manage_workspace", Some(&a)),
        "manage_workspace health (read-only) must be deadline-bounded"
    );
}

#[test]
fn test_is_write_exempt_manage_workspace_no_op_is_not_exempt() {
    // No operation key → unparseable → not exempt (safely bounded).
    assert!(
        !is_write_exempt("manage_workspace", None),
        "manage_workspace with no operation must not be exempt"
    );
}

#[test]
fn test_is_write_exempt_manage_workspace_garbage_op_is_not_exempt() {
    let a = args("not-a-real-op");
    assert!(
        !is_write_exempt("manage_workspace", Some(&a)),
        "manage_workspace with garbage operation must not be exempt"
    );
}

// ---------------------------------------------------------------------------
// dispatch_with_deadline — async, time-controlled
// ---------------------------------------------------------------------------

/// A read/query tool with a stalling future and a tight deadline → times out.
/// `start_paused = true` lets tokio auto-advance the clock to the next timer
/// so the test completes instantly in wall time.
#[tokio::test(start_paused = true)]
async fn test_dispatch_with_deadline_read_tool_times_out() {
    let stalling = async {
        tokio::time::sleep(Duration::from_secs(300)).await;
        // Unreachable in the timed-out case — but gives the future a type.
        Err::<CallToolResult, McpError>(McpError::internal_error("completed".to_string(), None))
    };

    let deadline = Some(Duration::from_millis(50));
    let result = dispatch_with_deadline("fast_search", /*exempt=*/ false, stalling, deadline).await;

    let err = result.expect_err("read tool must be timed out before the 300s future completes");
    assert!(
        err.message.contains("timed out"),
        "timeout error must say 'timed out'; got: {msg}",
        msg = err.message,
    );
    assert!(
        err.message.contains("fast_search"),
        "timeout error must name the tool; got: {msg}",
        msg = err.message,
    );
}

/// An exempt writer tool with the same stalling future and the same tight
/// deadline → NOT timed out (awaited to completion).
/// The writer's future runs to completion (300s in virtual time, 0ms real).
#[tokio::test(start_paused = true)]
async fn test_dispatch_with_deadline_writer_exempt_not_bounded() {
    let stalling = async {
        tokio::time::sleep(Duration::from_secs(300)).await;
        // This distinct error lets us confirm the future ran to completion.
        Err::<CallToolResult, McpError>(McpError::internal_error("completed".to_string(), None))
    };

    // The same tight deadline that fires for a read tool must be IGNORED for writers.
    let deadline = Some(Duration::from_millis(50));
    let result =
        dispatch_with_deadline("edit_file", /*exempt=*/ true, stalling, deadline).await;

    let err = result.expect_err("exempt writer must propagate its own error, not hang");
    assert!(
        err.message.contains("completed"),
        "exempt writer must be awaited to completion, not timed out; got: {msg}",
        msg = err.message,
    );
    assert!(
        !err.message.contains("timed out"),
        "exempt writer must NOT produce a timeout error; got: {msg}",
        msg = err.message,
    );
}
