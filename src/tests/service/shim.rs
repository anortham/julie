use super::http_api::Running;
use crate::service::client::connect_or_start;
use crate::service::shim::forward;
use serde_json::{Value, json};

fn meta() -> Value {
    json!({"io.modelcontextprotocol/protocolVersion": "2026-07-28", "io.modelcontextprotocol/clientCapabilities": {}})
}

#[tokio::test]
async fn shim_forwards_requests_and_drops_notifications() {
    let running = Running::start(None).await;
    let client = connect_or_start(&running.paths, || Ok(())).await.unwrap();
    let input = format!(
        "{}\n{}\n{}\n",
        json!({"jsonrpc":"2.0","id":1,"method":"server/discover","params":{"_meta": meta()}}),
        json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":99,"_meta": meta()}}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"manage_workspace","arguments":{"operation":"list"},"_meta": meta()}}),
    );
    let mut output = Vec::new();
    forward(
        &running.paths,
        &|| Ok(()),
        &running.paths.julie_home(),
        client,
        tokio::io::BufReader::new(input.as_bytes()),
        &mut output,
    )
    .await
    .unwrap();
    let lines: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(
        lines.len(),
        2,
        "one line per request, none for the notification"
    );
    assert_eq!(lines[0]["id"], 1);
    assert!(lines[0]["result"]["supportedVersions"].is_array());
    assert_eq!(lines[1]["id"], 2);
    assert_eq!(lines[1]["result"]["resultType"], "complete");
}

#[tokio::test]
async fn shim_result_equals_direct_http_result_for_the_same_call() {
    let running = Running::start(None).await;
    let client = connect_or_start(&running.paths, || Ok(())).await.unwrap();
    let call = json!({"jsonrpc":"2.0","id":7,"method":"tools/list","params":{"_meta": meta()}});
    let direct: Value = client
        .post_mcp(call.to_string().as_bytes(), "tools/list")
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let mut output = Vec::new();
    forward(
        &running.paths,
        &|| Ok(()),
        &running.paths.julie_home(),
        client,
        tokio::io::BufReader::new(format!("{call}\n").as_bytes()),
        &mut output,
    )
    .await
    .unwrap();
    let via_shim: Value =
        serde_json::from_str(String::from_utf8(output).unwrap().lines().next().unwrap()).unwrap();
    assert_eq!(direct["result"]["tools"], via_shim["result"]["tools"]);
}

#[tokio::test]
async fn shim_reconnects_when_the_service_goes_away_mid_session() {
    let running = Running::start(None).await;
    let paths = running.paths.clone();
    let first = connect_or_start(&paths, || Ok(())).await.unwrap();
    first.post_shutdown().await.unwrap();
    running.finished().await.unwrap();
    assert!(
        crate::service::discovery::read_record(&paths)
            .unwrap()
            .is_none()
    );

    let respawn_paths = paths.clone();
    let spawn = move || {
        let paths = respawn_paths.clone();
        tokio::spawn(async move {
            let app = crate::service::ServiceApp::new(crate::service::ServiceConfig {
                idle: None,
                registry_paths: paths,
            })
            .unwrap();
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            app.serve(listener).await.unwrap();
        });
        Ok(())
    };
    let call = json!({"jsonrpc":"2.0","id":9,"method":"tools/list","params":{"_meta": meta()}});
    let mut output = Vec::new();
    forward(
        &paths,
        &spawn,
        &paths.julie_home(),
        first,
        tokio::io::BufReader::new(format!("{call}\n").as_bytes()),
        &mut output,
    )
    .await
    .unwrap();
    let reply: Value =
        serde_json::from_str(String::from_utf8(output).unwrap().lines().next().unwrap()).unwrap();
    assert_eq!(reply["id"], 9);
    assert!(reply["result"]["tools"].is_array(), "got {reply}");
}

#[test]
fn shim_binds_tool_calls_to_its_working_directory_unless_a_workspace_is_named() {
    use crate::service::shim::bind_default_workspace;
    let root = std::path::Path::new("/repo/checkout");
    let call = |arguments: Value| {
        json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"fast_search","arguments":arguments}})
    };

    let mut omitted = call(json!({"query":"x"}));
    bind_default_workspace(&mut omitted, root);
    assert_eq!(omitted["params"]["arguments"]["workspace"], "/repo/checkout");

    let mut primary = call(json!({"query":"x","workspace":"primary"}));
    bind_default_workspace(&mut primary, root);
    assert_eq!(primary["params"]["arguments"]["workspace"], "/repo/checkout");

    let mut named = call(json!({"query":"x","workspace":"julie_5cb3ea69"}));
    bind_default_workspace(&mut named, root);
    assert_eq!(named["params"]["arguments"]["workspace"], "julie_5cb3ea69");

    let mut list = json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}});
    let before = list.clone();
    bind_default_workspace(&mut list, root);
    assert_eq!(list, before);
}
