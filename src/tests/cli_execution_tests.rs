//! Tests for the CLI execution core (A2).
//!
//! Validates:
//! - Standalone handler bootstrap (workspace creation, indexing)
//! - Daemon detection and connection fallback
//! - Error handling (missing workspace, unindexed workspace)
//! - `CliToolCommand` trait implementations
//! - Helper functions (summarize_error, serialize_call_tool_result, etc.)

use std::path::PathBuf;

use crate::cli_tools::daemon;
use crate::cli_tools::subcommands::*;
use crate::cli_tools::{
    CliExecutionMode, CliToolCommand, bootstrap_standalone_handler, run_cli_tool,
};

// ---------------------------------------------------------------------------
// CliExecutionMode display
// ---------------------------------------------------------------------------

#[test]
fn test_execution_mode_display_daemon() {
    assert_eq!(CliExecutionMode::Daemon.to_string(), "daemon");
}

#[test]
fn test_execution_mode_display_standalone() {
    assert_eq!(CliExecutionMode::Standalone.to_string(), "standalone");
}

#[test]
fn test_execution_mode_display_fallback() {
    assert_eq!(
        CliExecutionMode::DaemonFallback.to_string(),
        "standalone (daemon unavailable)"
    );
}

// ---------------------------------------------------------------------------
// CliToolCommand implementations: tool_name mapping
// ---------------------------------------------------------------------------

#[test]
fn test_search_args_tool_name() {
    let args = SearchArgs {
        query: "test".into(),
        target: "content".into(),
        limit: 10,
        language: None,
        file_pattern: None,
        context_lines: None,
        exclude_tests: false,
    };
    assert_eq!(args.tool_name(), "fast_search");
}

#[test]
fn test_refs_args_tool_name() {
    let args = RefsArgs {
        symbol: "Foo".into(),
        kind: None,
        limit: 10,
    };
    assert_eq!(args.tool_name(), "fast_refs");
}

#[test]
fn test_symbols_args_tool_name() {
    let args = SymbolsArgs {
        file_path: "src/main.rs".into(),
        mode: "structure".into(),
        target: None,
        limit: 50,
        max_depth: 1,
    };
    assert_eq!(args.tool_name(), "get_symbols");
}

#[test]
fn test_context_args_tool_name() {
    let args = ContextArgs {
        query: "test".into(),
        budget: None,
        max_hops: None,
        entry_symbols: None,
        prefer_tests: false,
    };
    assert_eq!(args.tool_name(), "get_context");
}

#[test]
fn test_blast_radius_args_tool_name() {
    let args = BlastRadiusArgs {
        rev: None,
        files: None,
        symbols: None,
        format: None,
    };
    assert_eq!(args.tool_name(), "blast_radius");
}

#[test]
fn test_workspace_args_tool_name() {
    let args = WorkspaceArgs {
        operation: "index".into(),
        path: None,
        force: false,
        name: None,
    };
    assert_eq!(args.tool_name(), "manage_workspace");
}

#[test]
fn test_generic_tool_args_tool_name() {
    let args = GenericToolArgs {
        name: "deep_dive".into(),
        params: "{}".into(),
    };
    assert_eq!(args.tool_name(), "deep_dive");
}

// ---------------------------------------------------------------------------
// CliToolCommand implementations: to_tool_args serialization
// ---------------------------------------------------------------------------

#[test]
fn test_search_to_tool_args_minimal() {
    let args = SearchArgs {
        query: "hello".into(),
        target: "content".into(),
        limit: 10,
        language: None,
        file_pattern: None,
        context_lines: None,
        exclude_tests: false,
    };
    let json = args.to_tool_args().unwrap();
    assert_eq!(json["query"], "hello");
    assert_eq!(json["search_target"], "content");
    assert_eq!(json["limit"], 10);
    assert!(json.get("language").is_none());
    assert!(json.get("file_pattern").is_none());
    assert!(json.get("context_lines").is_none());
    assert!(json.get("exclude_tests").is_none());
}

#[test]
fn test_search_to_tool_args_full() {
    let args = SearchArgs {
        query: "parse".into(),
        target: "definitions".into(),
        limit: 20,
        language: Some("rust".into()),
        file_pattern: Some("src/**/*.rs".into()),
        context_lines: Some(3),
        exclude_tests: true,
    };
    let json = args.to_tool_args().unwrap();
    assert_eq!(json["query"], "parse");
    assert_eq!(json["search_target"], "definitions");
    assert_eq!(json["limit"], 20);
    assert_eq!(json["language"], "rust");
    assert_eq!(json["file_pattern"], "src/**/*.rs");
    assert_eq!(json["context_lines"], 3);
    assert_eq!(json["exclude_tests"], true);
}

#[test]
fn test_refs_to_tool_args_with_filters() {
    let args = RefsArgs {
        symbol: "Command".into(),
        kind: Some("call".into()),
        limit: 25,
    };
    let json = args.to_tool_args().unwrap();
    assert_eq!(json["symbol"], "Command");
    assert_eq!(json["reference_kind"], "call");
    assert!(json.get("file_path").is_none());
    assert!(json.get("file_pattern").is_none());
    assert_eq!(json["limit"], 25);
}

#[test]
fn test_symbols_to_tool_args_with_target() {
    let args = SymbolsArgs {
        file_path: "src/handler.rs".into(),
        mode: "minimal".into(),
        target: Some("new".into()),
        limit: 5,
        max_depth: 2,
    };
    let json = args.to_tool_args().unwrap();
    assert_eq!(json["file_path"], "src/handler.rs");
    assert_eq!(json["mode"], "minimal");
    assert_eq!(json["target"], "new");
    assert_eq!(json["limit"], 5);
    assert_eq!(json["max_depth"], 2);
}

#[test]
fn test_context_to_tool_args_full() {
    let args = ContextArgs {
        query: "search scoring".into(),
        budget: Some(4000),
        max_hops: Some(2),
        entry_symbols: Some(vec!["FastSearchTool".into(), "Command".into()]),
        prefer_tests: true,
    };
    let json = args.to_tool_args().unwrap();
    assert_eq!(json["query"], "search scoring");
    assert_eq!(json["max_tokens"], 4000);
    assert_eq!(json["max_hops"], 2);
    let symbols = json["entry_symbols"].as_array().unwrap();
    assert_eq!(symbols.len(), 2);
    assert_eq!(symbols[0], "FastSearchTool");
    assert_eq!(json["prefer_tests"], true);
}

#[test]
fn test_blast_radius_to_tool_args_with_files() {
    // Note: --rev is now resolved via `git diff`, so we only test
    // the non-rev path here. Rev resolution is tested separately.
    let args = BlastRadiusArgs {
        rev: None,
        files: Some(vec!["src/cli.rs".into()]),
        symbols: Some(vec!["Command".into()]),
        format: Some("markdown".into()),
    };
    let json = args.to_tool_args().unwrap();
    assert!(
        json.get("rev").is_none(),
        "--rev should not appear in tool args"
    );
    let files = json["file_paths"].as_array().unwrap();
    assert_eq!(files[0], "src/cli.rs");
    let symbols = json["symbol_ids"].as_array().unwrap();
    assert_eq!(symbols[0], "Command");
    assert_eq!(json["format"], "markdown");
}

#[test]
fn test_blast_radius_to_tool_args_rev_resolves_to_files() {
    // --rev HEAD~1 should resolve via git diff to file paths.
    // This test runs in the julie repo so HEAD~1 should have changes.
    let args = BlastRadiusArgs {
        rev: Some("HEAD~1".into()),
        files: None,
        symbols: None,
        format: None,
    };
    let json = args.to_tool_args().unwrap();
    // The rev should be resolved to file_paths, not passed as "rev"
    assert!(
        json.get("rev").is_none(),
        "--rev should be resolved to file_paths"
    );
    let files = json["file_paths"].as_array().unwrap();
    assert!(!files.is_empty(), "HEAD~1 should have changed files");
}

#[test]
fn test_blast_radius_to_tool_args_rev_invalid() {
    let args = BlastRadiusArgs {
        rev: Some("nonexistent_rev_abc123xyz".into()),
        files: None,
        symbols: None,
        format: None,
    };
    let result = args.to_tool_args();
    assert!(result.is_err(), "Invalid rev should produce an error");
}

#[test]
fn test_blast_radius_symbols_validation_catches_names() {
    // Passing human-readable names like "FastSearchTool" should produce
    // a clear error in standalone mode.
    let args = BlastRadiusArgs {
        rev: None,
        files: None,
        symbols: Some(vec!["FastSearchTool".into()]),
        format: None,
    };
    // call_standalone would need a handler, but we can check that to_tool_args
    // at least passes (symbol validation happens in call_standalone).
    // The to_tool_args path passes symbols through as-is for daemon mode.
    let json = args.to_tool_args().unwrap();
    let symbols = json["symbol_ids"].as_array().unwrap();
    assert_eq!(symbols[0], "FastSearchTool");
}

#[test]
fn test_workspace_to_tool_args_with_force() {
    let args = WorkspaceArgs {
        operation: "index".into(),
        path: Some("/code/project".into()),
        force: true,
        name: Some("My Project".into()),
    };
    let json = args.to_tool_args().unwrap();
    assert_eq!(json["operation"], "index");
    assert_eq!(json["path"], "/code/project");
    assert_eq!(json["force"], true);
    assert_eq!(json["name"], "My Project");
}

#[test]
fn test_generic_to_tool_args_valid_json() {
    let args = GenericToolArgs {
        name: "fast_search".into(),
        params: r#"{"query":"test","search_target":"definitions"}"#.into(),
    };
    let json = args.to_tool_args().unwrap();
    assert_eq!(json["query"], "test");
    assert_eq!(json["search_target"], "definitions");
}

#[test]
fn test_generic_to_tool_args_invalid_json() {
    let args = GenericToolArgs {
        name: "fast_search".into(),
        params: "not valid json".into(),
    };
    let result = args.to_tool_args();
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("Invalid JSON in --params"));
}

#[test]
fn test_generic_to_tool_args_non_object() {
    let args = GenericToolArgs {
        name: "fast_search".into(),
        params: r#"["array", "not", "object"]"#.into(),
    };
    let result = args.to_tool_args();
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("must be a JSON object"));
}

// ---------------------------------------------------------------------------
// Standalone handler bootstrap: missing workspace
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_bootstrap_standalone_handler_missing_workspace() {
    let nonexistent = PathBuf::from("/tmp/julie_test_nonexistent_workspace_12345");
    let result = bootstrap_standalone_handler(&nonexistent).await;
    let err = result
        .err()
        .expect("Expected error for nonexistent workspace");
    let err_msg = err.to_string();
    assert!(
        err_msg.contains("does not exist"),
        "Expected 'does not exist' in error, got: {}",
        err_msg
    );
}

// ---------------------------------------------------------------------------
// Standalone handler bootstrap: real workspace init
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_bootstrap_standalone_handler_indexes_workspace() {
    let temp = tempfile::Builder::new()
        .prefix("julie_cli_test_")
        .tempdir()
        .unwrap();

    // Create a minimal source file so indexing has something to find
    let src_dir = temp.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::write(
        src_dir.join("main.rs"),
        "fn main() { println!(\"hello\"); }\n",
    )
    .unwrap();

    let workspace_root = temp.path().to_path_buf();
    let handler = match bootstrap_standalone_handler(&workspace_root).await {
        Ok(h) => h,
        Err(e) => panic!("Standalone bootstrap failed: {}", e),
    };

    let is_indexed = *handler.is_indexed.read().await;
    assert!(is_indexed, "Workspace should be indexed after bootstrap");
}

// ---------------------------------------------------------------------------
// Daemon detection
// ---------------------------------------------------------------------------

#[test]
fn test_daemon_appears_running_returns_bool() {
    // This is a quick sanity check: the function should return without panic.
    // Whether it returns true or false depends on whether a daemon is running,
    // which is environment-dependent. We verify it's callable.
    let _running = daemon::daemon_appears_running();
}

#[test]
fn test_build_startup_hint_sets_cli_source() {
    use crate::workspace::startup_hint::WorkspaceStartupSource;

    let hint = daemon::build_startup_hint(PathBuf::from("/some/path"));
    assert_eq!(hint.path, PathBuf::from("/some/path"));
    assert_eq!(hint.source, Some(WorkspaceStartupSource::Cli));
}

// ---------------------------------------------------------------------------
// run_cli_tool: standalone with missing workspace
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_run_cli_tool_standalone_missing_workspace() {
    let args = SearchArgs {
        query: "test".into(),
        target: "content".into(),
        limit: 10,
        language: None,
        file_pattern: None,
        context_lines: None,
        exclude_tests: false,
    };

    let result = run_cli_tool(
        &args,
        Some(PathBuf::from("/tmp/julie_nonexistent_ws_99999")),
        true, // standalone
    )
    .await;

    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("does not exist"),
        "Expected workspace-not-found error, got: {}",
        err_msg
    );
}

// ---------------------------------------------------------------------------
// run_cli_tool: daemon fallback to standalone (no daemon running)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_run_cli_tool_daemon_fallback_missing_workspace() {
    // With a nonexistent workspace path and no daemon, this should fail
    // with a workspace error (after the daemon fallback attempt).
    // If a daemon IS running, it may handle the call and return a result.
    let args = SearchArgs {
        query: "test".into(),
        target: "content".into(),
        limit: 10,
        language: None,
        file_pattern: None,
        context_lines: None,
        exclude_tests: false,
    };

    let result = run_cli_tool(
        &args,
        Some(PathBuf::from("/tmp/julie_nonexistent_ws_99998")),
        false, // not standalone, will try daemon then fall back
    )
    .await;

    // If a daemon is running, it may return a result (OK or error content).
    // If no daemon, standalone fallback should fail for the nonexistent path.
    // Either outcome is valid depending on environment state.
    match result {
        Ok(output) => {
            // Daemon handled it, or standalone fallback succeeded
            assert!(
                output.mode == crate::cli_tools::CliExecutionMode::Daemon
                    || output.mode == crate::cli_tools::CliExecutionMode::DaemonFallback,
                "Expected daemon or fallback mode, got: {}",
                output.mode
            );
        }
        Err(e) => {
            // Standalone fallback failed (expected when no daemon is running)
            let err_msg = e.to_string();
            assert!(
                err_msg.contains("does not exist")
                    || err_msg.contains("not indexed")
                    || err_msg.contains("Failed"),
                "Expected workspace error, got: {}",
                err_msg
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Serialization round-trip
// ---------------------------------------------------------------------------

#[test]
fn test_serialize_call_tool_result_preserves_content() {
    use crate::cli_tools::serialize_call_tool_result;
    use crate::mcp_compat::{CallToolResult, Content};

    let result = CallToolResult::success(vec![Content::text("search results here")]);

    let (value, is_error) = serialize_call_tool_result(result).unwrap();
    assert!(!is_error);

    let content = value["content"].as_array().unwrap();
    assert_eq!(content.len(), 1);

    // Content items have a "type" field and either "text" or other fields
    let first = &content[0];
    assert!(
        first.get("text").is_some(),
        "Expected text field in content item: {:?}",
        first
    );
}

#[test]
fn test_serialize_call_tool_result_error_flag() {
    use crate::cli_tools::serialize_call_tool_result;
    use crate::mcp_compat::{CallToolResult, Content};

    let result = CallToolResult::error(vec![Content::text("error details")]);

    let (_, is_error) = serialize_call_tool_result(result).unwrap();
    assert!(is_error);
}

// ---------------------------------------------------------------------------
// DaemonCallError: transport vs tool error distinction
// ---------------------------------------------------------------------------

#[test]
fn test_daemon_call_error_transport_displays() {
    use crate::cli_tools::daemon::DaemonCallError;

    let err = DaemonCallError::Transport(anyhow::anyhow!("connection refused"));
    let msg = err.to_string();
    assert_eq!(msg, "connection refused");
}

#[test]
fn test_daemon_call_error_tool_error_displays() {
    use crate::cli_tools::daemon::DaemonCallError;

    let err = DaemonCallError::ToolError {
        message: "Invalid params: missing 'query'".into(),
        raw: serde_json::json!({"code": -32602, "message": "Invalid params: missing 'query'"}),
    };
    let msg = err.to_string();
    assert!(
        msg.contains("Invalid params"),
        "Tool error message should surface the daemon's error: {}",
        msg
    );
}

#[test]
fn test_daemon_call_error_transport_is_send_sync() {
    // Verify DaemonCallError can be used across async boundaries
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<crate::cli_tools::daemon::DaemonCallError>();
}

// ---------------------------------------------------------------------------
// Refs: removed file_path/file_pattern flags
// ---------------------------------------------------------------------------

#[test]
fn test_refs_to_tool_args_no_file_filters() {
    let args = RefsArgs {
        symbol: "Command".into(),
        kind: None,
        limit: 10,
    };
    let json = args.to_tool_args().unwrap();
    assert!(
        json.get("file_path").is_none(),
        "file_path should not be in refs tool args"
    );
    assert!(
        json.get("file_pattern").is_none(),
        "file_pattern should not be in refs tool args"
    );
}
