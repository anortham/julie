use super::http_api::Running;
use serde_json::{Value, json};

async fn rpc(running: &Running, body: Value) -> (u16, Value) {
    let res = running
        .client()
        .post(format!("{}/mcp", running.base))
        .bearer_auth(&running.token)
        .header("Accept", "application/json, text/event-stream")
        .header("Mcp-Method", body["method"].as_str().unwrap_or(""))
        .json(&body)
        .send()
        .await
        .unwrap();
    let status = res.status().as_u16();
    let text = res.text().await.unwrap();
    let value = serde_json::from_str(&text).unwrap_or_else(|_| json!({"raw": text}));
    (status, value)
}

fn meta() -> Value {
    json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientCapabilities": {},
        "io.modelcontextprotocol/clientInfo": {"name": "julie-test", "version": "0"}
    })
}

#[tokio::test]
async fn mcp_without_token_is_401() {
    let running = Running::start(None).await;
    let res = running
        .client()
        .post(format!("{}/mcp", running.base))
        .json(&json!({"jsonrpc":"2.0","id":1,"method":"server/discover","params":{}}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn server_discover_lists_2026_07_28_without_a_session_header() {
    let running = Running::start(None).await;
    let res = running
        .client()
        .post(format!("{}/mcp", running.base))
        .bearer_auth(&running.token)
        .header("Accept", "application/json, text/event-stream")
        .header("Mcp-Method", "server/discover")
        .json(
            &json!({"jsonrpc":"2.0","id":1,"method":"server/discover","params":{"_meta": meta()}}),
        )
        .send()
        .await
        .unwrap();
    assert!(res.headers().get("Mcp-Session-Id").is_none());
    let body: Value = res.json().await.unwrap();
    let versions = body["result"]["supportedVersions"]
        .as_array()
        .expect("supportedVersions");
    assert!(versions.iter().any(|v| v == "2026-07-28"));
}

#[tokio::test]
async fn tools_list_over_http_matches_the_adapter_catalog() {
    let running = Running::start(None).await;
    let (_, body) = rpc(
        &running,
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{"_meta": meta()}}),
    )
    .await;
    let names: Vec<&str> = body["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    let expected: Vec<&str> = crate::request_engine::catalog::ToolCatalog::list()
        .iter()
        .map(|t| t.name)
        .collect();
    assert_eq!(names, expected);
    let (_, again) = rpc(
        &running,
        json!({"jsonrpc":"2.0","id":3,"method":"tools/list","params":{"_meta": meta()}}),
    )
    .await;
    assert_eq!(
        body["result"]["tools"], again["result"]["tools"],
        "tool order must be deterministic"
    );
}

#[tokio::test]
async fn tools_call_manage_workspace_list_returns_a_complete_result() {
    let running = Running::start(None).await;
    let (status, body) = rpc(
        &running,
        json!({
            "jsonrpc":"2.0","id":4,"method":"tools/call",
            "params":{"name":"manage_workspace","arguments":{"operation":"list"},"_meta": meta()}
        }),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body["result"]["resultType"], "complete");
    assert!(body["result"]["content"].is_array());
}

#[tokio::test]
async fn mcp_traffic_counts_as_activity_for_idle_exit_and_the_status_log() {
    let running = Running::start(Some(std::time::Duration::from_millis(400))).await;
    for id in 0..12 {
        let (status, _) = rpc(
            &running,
            json!({
                "jsonrpc":"2.0","id":id,"method":"tools/call",
                "params":{"name":"manage_workspace","arguments":{"operation":"list"},"_meta": meta()}
            }),
        )
        .await;
        assert_eq!(status, 200);
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let record = crate::service::discovery::read_record(&running.paths).unwrap();
    assert!(
        record.is_some(),
        "service exited while MCP calls were arriving"
    );
    let doc: Value = running
        .client()
        .get(format!("{}/status", running.base))
        .bearer_auth(&running.token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let requests = doc["requests"].as_array().unwrap();
    assert!(
        !requests.is_empty(),
        "MCP calls must appear on the status page"
    );
    assert_eq!(requests[0]["tool"], "manage_workspace");
    assert_eq!(requests[0]["outcome"], "ok");
}
