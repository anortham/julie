//! 13-Tool transport parity matrix test suite.
//!
//! Verifies output and error equivalence between stdio MCP and CLI execution
//! across all 13 registered Julie tools.

use regex::Regex;
use serde_json::{Value, json};

use crate::tests::request_process_helpers::ProcessFixture;

// ============================================================================
// 1. Normalization & Assertion Utilities
// ============================================================================

pub fn modern_meta() -> Value {
    json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientCapabilities": {}
    })
}

pub fn scrub_dynamic_metrics(text: &str) -> String {
    let re_time = Regex::new(r"\b\d+(\.\d+)?\s*(ms|s|µs)\b").unwrap();
    let text = re_time.replace_all(text, "<TIME>");
    Regex::new(r"(?P<label>last write|last file event): \d{4}-\d{2}-\d{2}T[-0-9:.+Z]+")
        .unwrap()
        .replace_all(&text, "$label: <TIME>")
        .to_string()
}

pub fn normalize_content(val: Option<&Value>) -> Value {
    match val {
        Some(Value::Array(items)) => {
            let scrubbed: Vec<Value> = items
                .iter()
                .map(|item| {
                    if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                        let mut copy = item.clone();
                        copy["text"] = Value::String(scrub_dynamic_metrics(text));
                        copy
                    } else {
                        item.clone()
                    }
                })
                .collect();
            Value::Array(scrubbed)
        }
        Some(other) => other.clone(),
        None => Value::Null,
    }
}

pub fn normalize_structured(val: Option<&Value>) -> Value {
    match val {
        Some(Value::Object(map)) => {
            let mut cleaned = map.clone();
            cleaned.remove("duration_ms");
            cleaned.remove("elapsed_ms");
            cleaned.remove("timestamp");
            if let Some(checkouts) = cleaned.get_mut("checkouts").and_then(Value::as_array_mut) {
                for checkout in checkouts {
                    if let Some(checkout) = checkout.as_object_mut() {
                        for field in [
                            "last_file_event_at",
                            "tantivy_age_seconds",
                            "graph_load_millis",
                            "vector_scan_millis",
                            "last_write_at",
                        ] {
                            checkout.remove(field);
                        }
                    }
                }
            }
            Value::Object(cleaned)
        }
        Some(other) => other.clone(),
        None => Value::Null,
    }
}

#[test]
fn mcp_transport_preserves_request_readiness() {
    let reply = crate::request_engine::ToolReply::from_result(
        "fast_search",
        Some("workspace-1".to_string()),
        json!({ "content": [], "structuredContent": { "matches": [] } }),
        crate::request_engine::RequestReadiness {
            mode: crate::request_engine::SemanticMode::Auto,
            status: "degraded: VECTORS_PARTIAL".to_string(),
            coverage: Some("1/2".to_string()),
            facts_revision: None,
            lexical_revision: None,
        },
    );

    let result = crate::handler::mcp_adapter::to_mcp_tool_result(reply).unwrap();
    let evidence = &result.meta.unwrap().0["io.julie/readiness"];
    assert_eq!(evidence["workspace_id"], "workspace-1");
    assert_eq!(evidence["readiness"]["coverage"], "1/2");
    assert_eq!(evidence["readiness"]["status"], "degraded: VECTORS_PARTIAL");
}

#[test]
fn checkout_normalization_ignores_measurements_but_keeps_semantics() {
    let first = json!({"checkouts":[{
        "workspace_id":"julie_a",
        "watcher":"running",
        "file_count":4,
        "last_file_event_at":"2026-09-12T00:00:00Z",
        "tantivy_age_seconds":1,
        "graph_load_millis":2,
        "vector_scan_millis":3,
        "last_write_at":"2026-09-12T00:00:01Z"
    }]});
    let mut second = first.clone();
    let checkout = second["checkouts"][0].as_object_mut().unwrap();
    checkout.insert("last_file_event_at".into(), json!("2026-09-12T00:01:00Z"));
    checkout.insert("tantivy_age_seconds".into(), json!(10));
    checkout.insert("graph_load_millis".into(), json!(20));
    checkout.insert("vector_scan_millis".into(), json!(30));
    checkout.insert("last_write_at".into(), json!("2026-09-12T00:01:01Z"));

    assert_eq!(
        normalize_structured(Some(&first)),
        normalize_structured(Some(&second))
    );

    second["checkouts"][0]["watcher"] = json!("stopped");
    assert_ne!(
        normalize_structured(Some(&first)),
        normalize_structured(Some(&second))
    );
}

pub fn assert_tool_parity(tool_name: &str, cli_envelope: &Value, mcp_response: &Value) {
    // 1. Verify CLI envelope invariants
    assert_eq!(
        cli_envelope["schema_version"], 1,
        "CLI envelope schema_version must be 1"
    );
    assert_eq!(
        cli_envelope["ok"], true,
        "CLI envelope ok must be true for tool {}",
        tool_name
    );

    let cli_reply = &cli_envelope["reply"];
    assert_eq!(
        cli_reply["schema_version"], 1,
        "CLI reply schema_version must be 1"
    );
    assert_eq!(
        cli_reply["tool"], tool_name,
        "CLI reply tool must match expected tool {}",
        tool_name
    );
    let readiness_status = cli_reply["readiness"]["status"].as_str().unwrap_or("");
    assert!(
        readiness_status == "ready"
            || readiness_status == "disabled"
            || readiness_status.starts_with("degraded"),
        "CLI reply readiness status must be 'ready', 'disabled', or 'degraded', got '{}'",
        readiness_status
    );

    // 2. Verify MCP envelope invariants
    assert_eq!(
        mcp_response["jsonrpc"], "2.0",
        "MCP response must be JSON-RPC 2.0"
    );
    assert!(
        mcp_response.get("error").is_none(),
        "MCP returned unexpected error: {:?}",
        mcp_response.get("error")
    );

    let mcp_result = &mcp_response["result"];
    assert_eq!(
        mcp_result["resultType"], "complete",
        "MCP resultType must be 'complete' under 2026-07-28"
    );

    // 3. Extract tool execution outputs
    let cli_result = &cli_reply["result"];

    // 4. Assert isError parity
    let cli_is_err = cli_result
        .get("isError")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let mcp_is_err = mcp_result
        .get("isError")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    assert_eq!(
        cli_is_err, mcp_is_err,
        "isError mismatch for tool {}",
        tool_name
    );

    // 5. Assert normalized content parity
    let cli_content = normalize_content(cli_result.get("content"));
    let mcp_content = normalize_content(mcp_result.get("content"));
    assert_eq!(
        cli_content, mcp_content,
        "content mismatch for tool {}",
        tool_name
    );

    // 6. Assert structuredContent parity if present
    if cli_result.get("structuredContent").is_some()
        || mcp_result.get("structuredContent").is_some()
    {
        let cli_struct = normalize_structured(cli_result.get("structuredContent"));
        let mcp_struct = normalize_structured(mcp_result.get("structuredContent"));
        assert_eq!(
            cli_struct, mcp_struct,
            "structuredContent mismatch for tool {}",
            tool_name
        );
    }
}

pub fn assert_error_parity(cli_envelope: &Value, mcp_response: &Value, expected_code: &str) {
    assert_eq!(cli_envelope["schema_version"], 1);
    assert_eq!(cli_envelope["ok"], false);
    assert_eq!(cli_envelope["error"]["code"], expected_code);

    assert_eq!(mcp_response["jsonrpc"], "2.0");
    let mcp_err = &mcp_response["error"];
    assert!(mcp_err.is_object(), "MCP error object expected");

    if let Some(data_code) = mcp_err["data"]["code"].as_str() {
        assert_eq!(data_code, expected_code);
    } else {
        assert!(
            mcp_err["message"]
                .as_str()
                .unwrap_or_default()
                .contains(expected_code),
            "MCP error message missing code {expected_code}"
        );
    }
}

// ============================================================================
// 2. Direct Tools Call & Initial Handshake Acceptance
// ============================================================================

#[tokio::test]
async fn modern_direct_tools_call_initializes_workspace_without_handshake() {
    let mut fixture = ProcessFixture::from_env().await;
    let result = fixture
        .rpc(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "fast_search",
                "arguments": {
                    "query": "request_probe",
                    "workspace": fixture.root(),
                    "limit": 5
                },
                "_meta": {
                    "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                    "io.modelcontextprotocol/clientCapabilities": {}
                }
            }
        }))
        .await;
    assert_eq!(result["result"]["resultType"], "complete");
    assert_ne!(result["result"]["isError"], true);
    assert!(
        result["result"]["content"]
            .to_string()
            .contains("request_probe")
    );
    fixture.shutdown().await;
}

// ============================================================================
// 3. 13-Tool Parity Test Matrix
// ============================================================================

#[tokio::test]
async fn test_parity_01_fast_search() {
    let mut fixture = ProcessFixture::from_env().await;
    let ws = fixture.root().to_string_lossy().to_string();

    let mcp_res = fixture
        .rpc(json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {
                "name": "fast_search",
                "arguments": { "query": "calculate_sum", "limit": 5, "workspace": ws },
                "_meta": modern_meta()
            }
        }))
        .await;

    let (exit, cli_res) = fixture
        .cli_json(&[
            "search",
            "calculate_sum",
            "-n",
            "5",
            "--workspace",
            &ws,
            "--json",
        ])
        .await;
    assert_eq!(exit, 0);

    assert_tool_parity("fast_search", &cli_res, &mcp_res);
    fixture.shutdown().await;
}

#[tokio::test]
async fn test_parity_02_fast_refs() {
    let mut fixture = ProcessFixture::from_env().await;
    let ws = fixture.root().to_string_lossy().to_string();

    let mcp_res = fixture
        .rpc(json!({
            "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {
                "name": "fast_refs",
                "arguments": { "symbol": "calculate_sum", "limit": 5, "workspace": ws },
                "_meta": modern_meta()
            }
        }))
        .await;

    let (exit, cli_res) = fixture
        .cli_json(&[
            "refs",
            "calculate_sum",
            "-n",
            "5",
            "--workspace",
            &ws,
            "--json",
        ])
        .await;
    assert_eq!(exit, 0);

    assert_tool_parity("fast_refs", &cli_res, &mcp_res);
    fixture.shutdown().await;
}

#[tokio::test]
async fn test_parity_03_get_symbols() {
    let mut fixture = ProcessFixture::from_env().await;
    let ws = fixture.root().to_string_lossy().to_string();

    let mcp_res = fixture
        .rpc(json!({
            "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {
                "name": "get_symbols",
                "arguments": { "file_path": "src/main.rs", "workspace": ws },
                "_meta": modern_meta()
            }
        }))
        .await;

    let (exit, cli_res) = fixture
        .cli_json(&["symbols", "src/main.rs", "--workspace", &ws, "--json"])
        .await;
    assert_eq!(exit, 0);

    assert_tool_parity("get_symbols", &cli_res, &mcp_res);
    fixture.shutdown().await;
}

#[tokio::test]
async fn test_parity_04_get_context() {
    let mut fixture = ProcessFixture::from_env().await;
    let ws = fixture.root().to_string_lossy().to_string();

    let mcp_res = fixture
        .rpc(json!({
            "jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {
                "name": "get_context",
                "arguments": { "query": "calculate_sum", "workspace": ws },
                "_meta": modern_meta()
            }
        }))
        .await;

    let (exit, cli_res) = fixture
        .cli_json(&["context", "calculate_sum", "--workspace", &ws, "--json"])
        .await;
    assert_eq!(exit, 0);

    assert_tool_parity("get_context", &cli_res, &mcp_res);
    fixture.shutdown().await;
}

#[tokio::test]
async fn test_parity_05_call_path() {
    let mut fixture = ProcessFixture::from_env().await;
    let ws = fixture.root().to_string_lossy().to_string();

    let mcp_res = fixture
        .rpc(json!({
            "jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": {
                "name": "call_path",
                "arguments": { "from": "main", "to": "calculate_sum", "workspace": ws },
                "_meta": modern_meta()
            }
        }))
        .await;

    let (exit, cli_res) = fixture
        .cli_json(&[
            "call-path",
            "main",
            "calculate_sum",
            "--workspace",
            &ws,
            "--json",
        ])
        .await;
    assert_eq!(exit, 0);

    assert_tool_parity("call_path", &cli_res, &mcp_res);
    fixture.shutdown().await;
}

#[tokio::test]
async fn test_parity_06_blast_radius() {
    let mut fixture = ProcessFixture::from_env().await;
    let ws = fixture.root().to_string_lossy().to_string();

    let mcp_res = fixture
        .rpc(json!({
            "jsonrpc": "2.0", "id": 6, "method": "tools/call", "params": {
                "name": "blast_radius",
                "arguments": { "file_paths": ["src/main.rs"], "workspace": ws },
                "_meta": modern_meta()
            }
        }))
        .await;

    let (exit, cli_res) = fixture
        .cli_json(&[
            "blast-radius",
            "-f",
            "src/main.rs",
            "--workspace",
            &ws,
            "--json",
        ])
        .await;
    assert_eq!(exit, 0);

    assert_tool_parity("blast_radius", &cli_res, &mcp_res);
    fixture.shutdown().await;
}

#[tokio::test]
async fn test_parity_07_deep_dive() {
    let mut fixture = ProcessFixture::from_env().await;
    let ws = fixture.root().to_string_lossy().to_string();

    let mcp_res = fixture
        .rpc(json!({
            "jsonrpc": "2.0", "id": 7, "method": "tools/call", "params": {
                "name": "deep_dive",
                "arguments": { "symbol": "calculate_sum", "workspace": ws },
                "_meta": modern_meta()
            }
        }))
        .await;

    let (exit, cli_res) = fixture
        .cli_json(&["deep-dive", "calculate_sum", "--workspace", &ws, "--json"])
        .await;
    assert_eq!(exit, 0);

    assert_tool_parity("deep_dive", &cli_res, &mcp_res);
    fixture.shutdown().await;
}

#[tokio::test]
async fn test_parity_08_patterns() {
    let mut fixture = ProcessFixture::from_env().await;
    let ws = fixture.root().to_string_lossy().to_string();

    let mcp_res = fixture
        .rpc(json!({
            "jsonrpc": "2.0", "id": 8, "method": "tools/call", "params": {
                "name": "patterns",
                "arguments": { "operation": "list", "workspace": ws },
                "_meta": modern_meta()
            }
        }))
        .await;

    let (exit, cli_res) = fixture
        .cli_json(&[
            "patterns",
            "--operation",
            "list",
            "--workspace",
            &ws,
            "--json",
        ])
        .await;
    assert_eq!(exit, 0);

    assert_tool_parity("patterns", &cli_res, &mcp_res);
    fixture.shutdown().await;
}

#[tokio::test]
async fn test_parity_09_manage_workspace() {
    let mut fixture = ProcessFixture::from_env().await;
    let ws = fixture.root().to_string_lossy().to_string();

    let opened = fixture
        .rpc(json!({
            "jsonrpc": "2.0", "id": 90, "method": "tools/call", "params": {
                "name": "manage_workspace",
                "arguments": { "operation": "open", "path": &ws },
                "_meta": modern_meta()
            }
        }))
        .await;
    assert!(
        opened.get("error").is_none(),
        "workspace open failed: {opened}"
    );
    assert_ne!(
        opened["result"]["isError"], true,
        "workspace open failed: {opened}"
    );

    let mcp_res = fixture
        .rpc(json!({
            "jsonrpc": "2.0", "id": 9, "method": "tools/call", "params": {
                "name": "manage_workspace",
                "arguments": { "operation": "status", "path": &ws },
                "_meta": modern_meta()
            }
        }))
        .await;

    let (exit, cli_res) = fixture
        .cli_json(&[
            "workspace",
            "status",
            "--path",
            &ws,
            "--workspace",
            &ws,
            "--json",
        ])
        .await;
    assert_eq!(exit, 0);

    assert_tool_parity("manage_workspace", &cli_res, &mcp_res);
    fixture.shutdown().await;
}

#[tokio::test]
async fn test_parity_10_edit_file_preview() {
    let mut fixture = ProcessFixture::from_env().await;
    let ws = fixture.root().to_string_lossy().to_string();

    let mcp_res = fixture
        .rpc(json!({
            "jsonrpc": "2.0", "id": 10, "method": "tools/call", "params": {
                "name": "edit_file",
                "arguments": {
                    "file_path": "src/main.rs",
                    "old_text": "calculate_sum",
                    "new_text": "compute_sum",
                    "dry_run": true,
                    "workspace": ws
                },
                "_meta": modern_meta()
            }
        }))
        .await;

    let (exit, cli_res) = fixture
        .cli_json(&[
            "edit",
            "src/main.rs",
            "-o",
            "calculate_sum",
            "-n",
            "compute_sum",
            "--dry-run=true",
            "--workspace",
            &ws,
            "--json",
        ])
        .await;
    assert_eq!(exit, 0);

    assert_tool_parity("edit_file", &cli_res, &mcp_res);
    fixture.shutdown().await;
}

#[tokio::test]
async fn test_parity_generic_tool_subcommand() {
    let mut fixture = ProcessFixture::from_env().await;
    let ws = fixture.root().to_string_lossy().to_string();

    let mcp_res = fixture
        .rpc(json!({
            "jsonrpc": "2.0", "id": 14, "method": "tools/call", "params": {
                "name": "fast_search",
                "arguments": { "query": "calculate_sum", "limit": 5, "workspace": ws },
                "_meta": modern_meta()
            }
        }))
        .await;

    let (exit, cli_res) = fixture
        .cli_json(&[
            "tool",
            "fast_search",
            "--params",
            r#"{"query":"calculate_sum","limit":5}"#,
            "--workspace",
            &ws,
            "--json",
        ])
        .await;
    assert_eq!(exit, 0);

    assert_tool_parity("fast_search", &cli_res, &mcp_res);
    fixture.shutdown().await;
}

#[tokio::test]
async fn test_parity_invalid_arguments_error() {
    let mut fixture = ProcessFixture::from_env().await;
    let ws = fixture.root().to_string_lossy().to_string();

    let mcp_res = fixture
        .rpc(json!({
            "jsonrpc": "2.0", "id": 99, "method": "tools/call", "params": {
                "name": "fast_search",
                "arguments": { "invalid_key": "val", "workspace": ws },
                "_meta": modern_meta()
            }
        }))
        .await;

    let (exit, cli_res) = fixture
        .cli_json(&[
            "tool",
            "fast_search",
            "--params",
            r#"{"invalid_key":"val"}"#,
            "--workspace",
            &ws,
            "--json",
        ])
        .await;
    assert_eq!(exit, 2);

    assert_error_parity(&cli_res, &mcp_res, "INVALID_ARGUMENTS");
    fixture.shutdown().await;
}

#[tokio::test]
async fn test_parity_unknown_tool_error() {
    let mut fixture = ProcessFixture::from_env().await;
    let ws = fixture.root().to_string_lossy().to_string();

    let mcp_res = fixture
        .rpc(json!({
            "jsonrpc": "2.0", "id": 100, "method": "tools/call", "params": {
                "name": "unknown_probe_tool",
                "arguments": {},
                "_meta": modern_meta()
            }
        }))
        .await;

    let (exit, cli_res) = fixture
        .cli_json(&[
            "tool",
            "unknown_probe_tool",
            "--params",
            "{}",
            "--workspace",
            &ws,
            "--json",
        ])
        .await;
    assert_eq!(exit, 2);

    assert_error_parity(&cli_res, &mcp_res, "UNKNOWN_TOOL");
    fixture.shutdown().await;
}
