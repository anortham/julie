//! Real-world acceptance pipeline scenarios.
//!
//! Exercises multi-step developer and agent workflows across both stdio MCP
//! and CLI transports:
//! 1. Codebase Exploration Pipeline (search -> symbols -> context -> call path -> patterns)
//! 2. Refactoring Pipeline (refs -> blast radius -> preview edit -> preview rename -> apply)
//! 3. Replay Audit Pipeline (JSONL trace recording -> MCP execution -> CLI replay parity)

use serde_json::json;

use crate::tests::request_process_helpers::ProcessFixture;
use crate::tests::request_transport_parity::{assert_tool_parity, modern_meta};

/// Scenario 1: Codebase Exploration Pipeline
/// fast_search -> get_symbols -> get_context -> call_path -> patterns
#[tokio::test]
async fn test_scenario_1_codebase_exploration_pipeline() {
    let mut fixture = ProcessFixture::from_env().await;
    let ws = fixture.root().to_string_lossy().to_string();

    // Step 1: Discover entry point symbol via fast_search
    let mcp_1 = fixture
        .rpc(json!({
            "jsonrpc": "2.0", "id": 101, "method": "tools/call", "params": {
                "name": "fast_search", "arguments": {
                    "query": "calculate_sum", "workspace": ws, "limit": 5
                }, "_meta": modern_meta()
            }
        }))
        .await;
    let (code1, cli_1) = fixture
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
    assert_eq!(code1, 0);
    assert_tool_parity("fast_search", &cli_1, &mcp_1);
    let s1_text = mcp_1["result"]["content"][0]["text"].as_str().unwrap();
    assert!(s1_text.contains("src/main.rs") || s1_text.contains("main.rs"));

    // Step 2: List symbols in discovered file src/main.rs
    let mcp_2 = fixture
        .rpc(json!({
            "jsonrpc": "2.0", "id": 102, "method": "tools/call", "params": {
                "name": "get_symbols", "arguments": {
                    "file_path": "src/main.rs", "workspace": ws
                }, "_meta": modern_meta()
            }
        }))
        .await;
    let (code2, cli_2) = fixture
        .cli_json(&["symbols", "src/main.rs", "--workspace", &ws, "--json"])
        .await;
    assert_eq!(code2, 0);
    assert_tool_parity("get_symbols", &cli_2, &mcp_2);
    let s2_text = mcp_2["result"]["content"][0]["text"].as_str().unwrap();
    assert!(s2_text.contains("calculate_sum"));
    assert!(s2_text.contains("main"));

    // Step 3: Fetch token-budgeted context for calculate_sum
    let mcp_3 = fixture
        .rpc(json!({
            "jsonrpc": "2.0", "id": 103, "method": "tools/call", "params": {
                "name": "get_context", "arguments": {
                    "query": "calculate_sum", "workspace": ws
                }, "_meta": modern_meta()
            }
        }))
        .await;
    let (code3, cli_3) = fixture
        .cli_json(&["context", "calculate_sum", "--workspace", &ws, "--json"])
        .await;
    assert_eq!(code3, 0);
    assert_tool_parity("get_context", &cli_3, &mcp_3);
    let s3_text = mcp_3["result"]["content"][0]["text"].as_str().unwrap();
    assert!(s3_text.contains("calculate_sum"));

    // Step 4: Trace call path from main to calculate_sum
    let mcp_4 = fixture
        .rpc(json!({
            "jsonrpc": "2.0", "id": 104, "method": "tools/call", "params": {
                "name": "call_path", "arguments": {
                    "from": "main", "to": "calculate_sum", "workspace": ws
                }, "_meta": modern_meta()
            }
        }))
        .await;
    let (code4, cli_4) = fixture
        .cli_json(&[
            "call-path",
            "main",
            "calculate_sum",
            "--workspace",
            &ws,
            "--json",
        ])
        .await;
    assert_eq!(code4, 0);
    assert_tool_parity("call_path", &cli_4, &mcp_4);
    let s4_text = mcp_4["result"]["content"][0]["text"].as_str().unwrap();
    assert!(s4_text.contains("main") && s4_text.contains("calculate_sum"));

    // Step 5: Query code-shape patterns via patterns tool
    let mcp_5 = fixture
        .rpc(json!({
            "jsonrpc": "2.0", "id": 105, "method": "tools/call", "params": {
                "name": "patterns", "arguments": {
                    "operation": "list", "workspace": ws
                }, "_meta": modern_meta()
            }
        }))
        .await;
    let (code5, cli_5) = fixture
        .cli_json(&[
            "patterns",
            "--operation",
            "list",
            "--workspace",
            &ws,
            "--json",
        ])
        .await;
    assert_eq!(code5, 0);
    assert_tool_parity("patterns", &cli_5, &mcp_5);
    let _s5_text = mcp_5["result"]["content"][0]["text"].as_str().unwrap();
    assert!(mcp_5["result"]["content"].is_array());
    assert!(cli_5["reply"]["result"]["content"].is_array());

    fixture.shutdown().await;
}

/// Scenario 2: Refactoring Pipeline
/// fast_refs -> blast_radius -> edit_file (preview) -> apply
#[tokio::test]
async fn test_scenario_2_refactoring_pipeline() {
    let mut fixture = ProcessFixture::from_env().await;
    let ws = fixture.root().to_string_lossy().to_string();

    // Step 1: Find all references to calculate_sum before refactoring
    let mcp_r1 = fixture
        .rpc(json!({
            "jsonrpc": "2.0", "id": 201, "method": "tools/call", "params": {
                "name": "fast_refs", "arguments": {
                    "symbol": "calculate_sum", "workspace": ws, "limit": 10
                }, "_meta": modern_meta()
            }
        }))
        .await;
    let (code1, cli_r1) = fixture
        .cli_json(&[
            "refs",
            "calculate_sum",
            "-n",
            "10",
            "--workspace",
            &ws,
            "--json",
        ])
        .await;
    assert_eq!(code1, 0);
    assert_tool_parity("fast_refs", &cli_r1, &mcp_r1);

    // Step 2: Analyze blast radius of changes to src/main.rs
    let mcp_r2 = fixture
        .rpc(json!({
            "jsonrpc": "2.0", "id": 202, "method": "tools/call", "params": {
                "name": "blast_radius", "arguments": {
                    "file_paths": ["src/main.rs"], "workspace": ws
                }, "_meta": modern_meta()
            }
        }))
        .await;
    let (code2, cli_r2) = fixture
        .cli_json(&[
            "blast-radius",
            "-f",
            "src/main.rs",
            "--workspace",
            &ws,
            "--json",
        ])
        .await;
    assert_eq!(code2, 0);
    assert_tool_parity("blast_radius", &cli_r2, &mcp_r2);

    // Step 3: Preview edit_file with dry_run: true
    let mcp_r3 = fixture
        .rpc(json!({
            "jsonrpc": "2.0", "id": 203, "method": "tools/call", "params": {
                "name": "edit_file", "arguments": {
                    "file_path": "src/main.rs",
                    "old_text": "calculate_sum",
                    "new_text": "compute_sum",
                    "dry_run": true,
                    "workspace": ws
                }, "_meta": modern_meta()
            }
        }))
        .await;
    let (code3, cli_r3) = fixture
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
    assert_eq!(code3, 0);
    assert_tool_parity("edit_file", &cli_r3, &mcp_r3);
    let r3_content = mcp_r3["result"]["content"].to_string();
    assert!(r3_content.contains("Dry run preview"));
    assert!(r3_content.contains("compute_sum"));

    // Assert disk unchanged during dry_run
    let content_orig = std::fs::read_to_string(fixture.root().join("src/main.rs")).unwrap();
    assert!(content_orig.contains("calculate_sum"));
    assert!(!content_orig.contains("compute_sum"));

    // Step 4: Shut down leader MCP and apply edit in a fresh workspace
    fixture.shutdown().await;

    let mut apply_fixture = ProcessFixture::from_env().await;
    let apply_ws = apply_fixture.root().to_string_lossy().to_string();

    let (apply_exit, apply_res) = apply_fixture
        .cli_json(&[
            "edit",
            "src/main.rs",
            "-o",
            "calculate_sum",
            "-n",
            "compute_sum",
            "--occurrence",
            "all",
            "--dry-run=false",
            "--workspace",
            &apply_ws,
            "--json",
        ])
        .await;
    assert_eq!(apply_exit, 0);
    assert_eq!(apply_res["ok"], true);

    let updated_main = std::fs::read_to_string(apply_fixture.root().join("src/main.rs")).unwrap();
    assert!(updated_main.contains("compute_sum"));
    assert!(!updated_main.contains("calculate_sum"));

    apply_fixture.shutdown().await;
}

/// Scenario 3: Replay Audit Pipeline
/// Record JSONL trace -> MCP execution -> CLI tools replay -> Assert Parity & Exit Codes
#[tokio::test]
async fn test_scenario_3_replay_audit_pipeline() {
    let mut fixture = ProcessFixture::from_env().await;
    let ws = fixture.root().to_string_lossy().to_string();

    let trace_records = vec![
        (
            "audit-01",
            "fast_search",
            json!({"query": "calculate_sum", "limit": 5, "workspace": ws}),
        ),
        (
            "audit-02",
            "get_symbols",
            json!({"file_path": "src/main.rs", "workspace": ws}),
        ),
        (
            "audit-03",
            "get_context",
            json!({"query": "calculate_sum", "workspace": ws}),
        ),
        (
            "audit-04",
            "call_path",
            json!({"from": "main", "to": "calculate_sum", "workspace": ws}),
        ),
    ];

    // 1. Run each record through MCP stdio and collect responses
    let mut mcp_responses = Vec::new();
    for (i, (req_id, tool_name, args)) in trace_records.iter().enumerate() {
        let resp = fixture
            .rpc(json!({
                "jsonrpc": "2.0",
                "id": i + 1,
                "method": "tools/call",
                "params": {
                    "name": tool_name,
                    "arguments": args,
                    "_meta": modern_meta()
                }
            }))
            .await;
        mcp_responses.push((req_id.to_string(), resp));
    }

    // 2. Write JSONL trace file
    let jsonl_path = fixture.home().join("replay_audit.jsonl");
    let mut jsonl_body = String::new();
    for (req_id, tool_name, args) in &trace_records {
        let record = json!({
            "schema_version": 1,
            "request_id": req_id,
            "request": {
                "name": tool_name,
                "arguments": args,
                "workspace": ws,
                "semantics": "auto"
            }
        });
        jsonl_body.push_str(&serde_json::to_string(&record).unwrap());
        jsonl_body.push('\n');
    }
    std::fs::write(&jsonl_path, jsonl_body).expect("write replay jsonl");

    // 3. Run CLI replay: julie-server tools replay --input <path> --workspace <ws> --json
    let cli_replay = fixture
        .cli(&[
            "tools",
            "replay",
            "--input",
            jsonl_path.to_str().unwrap(),
            "--workspace",
            &ws,
            "--json",
        ])
        .await;
    assert_eq!(
        cli_replay.exit_code, 0,
        "Replay failed stderr: {}",
        cli_replay.stderr
    );

    // 4. Compare each line of replay output with matching MCP response
    let replay_lines: Vec<&str> = cli_replay
        .stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .collect();
    assert_eq!(replay_lines.len(), trace_records.len());

    for (line, (expected_id, mcp_resp)) in replay_lines.iter().zip(mcp_responses.iter()) {
        let cli_env: serde_json::Value = serde_json::from_str(line).expect("parse replay line");
        assert_eq!(cli_env["schema_version"], 1);
        assert_eq!(cli_env["ok"], true);
        assert_eq!(cli_env["request_id"], *expected_id);

        let tool_name = cli_env["reply"]["tool"].as_str().unwrap();
        assert_tool_parity(tool_name, &cli_env, mcp_resp);
    }

    // 5a. Test replay failure halting and error exit code on malformed JSON framing
    let malformed_jsonl_path = fixture.home().join("malformed_replay.jsonl");
    std::fs::write(
        &malformed_jsonl_path,
        r#"{"schema_version":1,"request":{incomplete"#,
    )
    .expect("write malformed jsonl");

    let malformed_replay = fixture
        .cli(&[
            "tools",
            "replay",
            "--input",
            malformed_jsonl_path.to_str().unwrap(),
            "--json",
        ])
        .await;
    assert_eq!(malformed_replay.exit_code, 2);

    let malformed_line = malformed_replay.stdout.lines().next().unwrap();
    let malformed_env: serde_json::Value = serde_json::from_str(malformed_line).unwrap();
    assert_eq!(malformed_env["ok"], false);
    assert_eq!(malformed_env["error"]["code"], "INVALID_ARGUMENTS");
    assert!(
        malformed_env["error"]["message"]
            .as_str()
            .unwrap()
            .contains("malformed framing"),
        "error message must contain 'malformed framing', got: {}",
        malformed_env["error"]["message"]
    );

    // 5b. Test unknown tool handling halts with exit code 2 and UNKNOWN_TOOL error code
    let bad_jsonl_path = fixture.home().join("bad_replay.jsonl");
    let bad_body = format!(
        "{}\n",
        json!({
            "schema_version": 1,
            "request_id": "bad-req-1",
            "request": {
                "name": "unknown_tool_replay",
                "arguments": {},
                "workspace": ws
            }
        })
    );
    std::fs::write(&bad_jsonl_path, bad_body).expect("write bad jsonl");

    let bad_replay = fixture
        .cli(&[
            "tools",
            "replay",
            "--input",
            bad_jsonl_path.to_str().unwrap(),
            "--workspace",
            &ws,
            "--json",
        ])
        .await;
    assert_eq!(bad_replay.exit_code, 2);

    let bad_line = bad_replay.stdout.lines().next().unwrap();
    let bad_env: serde_json::Value = serde_json::from_str(bad_line).unwrap();
    assert_eq!(bad_env["ok"], false);
    assert_eq!(bad_env["error"]["code"], "UNKNOWN_TOOL");

    fixture.shutdown().await;
}
