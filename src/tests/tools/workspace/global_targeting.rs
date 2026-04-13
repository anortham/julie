use std::fs;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use rmcp::{
    ServerHandler,
    model::{CallToolRequestParams, NumberOrString, ServerJsonRpcMessage, ServerRequest},
    service::{RequestContext, serve_directly},
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::daemon::database::DaemonDatabase;
use crate::daemon::workspace_pool::WorkspacePool;
use crate::handler::JulieServerHandler;
use crate::mcp_compat::CallToolResult;
use crate::paths::DaemonPaths;
use crate::tools::FastSearchTool;
use crate::tools::workspace::ManageWorkspaceTool;
use crate::workspace::registry::generate_workspace_id;
use serial_test::serial;

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

async fn send_json_line(writer: &mut (impl AsyncWriteExt + Unpin), value: &serde_json::Value) {
    writer
        .write_all(serde_json::to_string(value).unwrap().as_bytes())
        .await
        .unwrap();
    writer.write_all(b"\n").await.unwrap();
    writer.flush().await.unwrap();
}

async fn read_server_message(
    lines: &mut tokio::io::Lines<BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>>,
) -> ServerJsonRpcMessage {
    let line = lines
        .next_line()
        .await
        .unwrap()
        .expect("server should emit a JSON-RPC message line");
    serde_json::from_str(&line).unwrap()
}

async fn mark_index_ready(handler: &JulieServerHandler) {
    handler
        .indexing_status
        .search_ready
        .store(true, Ordering::Relaxed);
    *handler.is_indexed.write().await = true;
}

async fn setup_known_reference_search_workspace() -> (tempfile::TempDir, JulieServerHandler, String)
{
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let primary_root = temp_dir.path().join("primary");
    let target_root = temp_dir.path().join("target");
    fs::create_dir_all(&primary_root).unwrap();
    fs::create_dir_all(&target_root).unwrap();
    fs::write(primary_root.join("main.rs"), "fn primary_marker() {}\n").unwrap();
    fs::write(
        target_root.join("lib.rs"),
        "pub fn target_search_marker() {}\nconst TARGET_ONLY_MARKER: &str = \"target_search_marker\";\n",
    )
    .unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));

    let primary_path = primary_root.canonicalize().unwrap();
    let primary_path_str = primary_path.to_string_lossy().to_string();
    let primary_id = generate_workspace_id(&primary_path_str).unwrap();
    let primary_ws = pool
        .get_or_init(&primary_id, primary_path.clone())
        .await
        .expect("primary workspace should initialize");

    let seed_handler = JulieServerHandler::new_with_shared_workspace(
        Arc::clone(&primary_ws),
        primary_path.clone(),
        Some(Arc::clone(&daemon_db)),
        Some(primary_id.clone()),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await
    .expect("seed handler should initialize");

    ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(primary_path_str),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&seed_handler)
    .await
    .expect("primary workspace should index");
    mark_index_ready(&seed_handler).await;

    let target_path = target_root.canonicalize().unwrap();
    let target_path_str = target_path.to_string_lossy().to_string();
    let target_id = generate_workspace_id(&target_path_str).unwrap();
    daemon_db
        .upsert_workspace(&target_id, &target_path_str, "ready")
        .expect("reference workspace should be registered in daemon db");
    ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(target_path_str),
        force: Some(true),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&seed_handler)
    .await
    .expect("reference workspace should index");
    mark_index_ready(&seed_handler).await;

    let handler = JulieServerHandler::new_with_shared_workspace(
        primary_ws,
        primary_path,
        Some(Arc::clone(&daemon_db)),
        Some(primary_id),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await
    .expect("fresh handler should initialize");

    (temp_dir, handler, target_id)
}

#[tokio::test]
async fn test_known_workspace_requires_open_before_fast_search() {
    let (_temp_dir, handler, target_id) = setup_known_reference_search_workspace().await;

    assert!(
        !handler.is_workspace_active(&target_id).await,
        "fresh session should not auto-activate known reference workspace"
    );

    let result = FastSearchTool {
        query: "target_search_marker".to_string(),
        limit: 10,
        search_target: "content".to_string(),
        workspace: Some(target_id.clone()),
        ..Default::default()
    }
    .call_tool(&handler)
    .await;

    let error = result.expect_err("known but inactive workspace should be rejected");
    let message = error.to_string();
    assert!(
        message.contains(&target_id),
        "error should name workspace: {message}"
    );
    assert!(
        message.contains("manage_workspace(operation=\"open\", workspace_id=\"")
            && message.contains(&target_id),
        "error should explain how to open the workspace first: {message}"
    );
}

#[tokio::test]
async fn test_persisted_pairing_metadata_does_not_imply_known_workspace_activation() {
    let (temp_dir, handler, target_id) = setup_known_reference_search_workspace().await;
    let daemon_db = handler
        .daemon_db
        .as_ref()
        .expect("test handler should expose daemon db")
        .clone();
    let primary_id = handler
        .loaded_workspace_id()
        .expect("test handler should expose primary workspace id");

    daemon_db
        .add_reference(&primary_id, &target_id)
        .expect("persisted pairing should be recorded");

    let primary_root = temp_dir.path().join("primary").canonicalize().unwrap();
    let primary_ws = handler
        .workspace_pool
        .as_ref()
        .expect("test handler should expose workspace pool")
        .get_or_init(&primary_id, primary_root.clone())
        .await
        .expect("primary workspace should still be loadable");

    let fresh_handler = JulieServerHandler::new_with_shared_workspace(
        primary_ws,
        primary_root,
        Some(Arc::clone(&daemon_db)),
        Some(primary_id),
        None,
        None,
        None,
        None,
        handler.workspace_pool.clone(),
    )
    .await
    .expect("fresh handler should initialize");

    assert!(
        !fresh_handler.is_workspace_active(&target_id).await,
        "persisted pairing must not auto-activate on a new session"
    );

    let result = FastSearchTool {
        query: "target_search_marker".to_string(),
        limit: 10,
        search_target: "content".to_string(),
        workspace: Some(target_id.clone()),
        ..Default::default()
    }
    .call_tool(&fresh_handler)
    .await;

    let error = result.expect_err("paired but unopened workspace should be rejected");
    let message = error.to_string();
    assert!(
        message.contains(&target_id),
        "error should name workspace: {message}"
    );
    assert!(
        message.contains("manage_workspace(operation=\"open\", workspace_id=\"")
            && message.contains(&target_id),
        "error should explain how to open the paired workspace first: {message}"
    );
}

#[tokio::test]
async fn test_manage_workspace_list_includes_unpaired_known_workspace() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let primary_root = temp_dir.path().join("primary");
    let paired_root = temp_dir.path().join("paired");
    let known_root = temp_dir.path().join("known");
    fs::create_dir_all(&primary_root).unwrap();
    fs::create_dir_all(&paired_root).unwrap();
    fs::create_dir_all(&known_root).unwrap();
    fs::write(primary_root.join("main.rs"), "fn primary() {}\n").unwrap();
    fs::write(paired_root.join("lib.rs"), "fn paired() {}\n").unwrap();
    fs::write(known_root.join("lib.rs"), "fn known() {}\n").unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));

    let primary_path = primary_root.canonicalize().unwrap();
    let primary_path_str = primary_path.to_string_lossy().to_string();
    let primary_id = generate_workspace_id(&primary_path_str).unwrap();
    daemon_db
        .upsert_workspace(&primary_id, &primary_path_str, "ready")
        .unwrap();

    let primary_ws = pool
        .get_or_init(&primary_id, primary_path.clone())
        .await
        .expect("primary workspace should initialize");
    let handler = JulieServerHandler::new_with_shared_workspace(
        primary_ws,
        primary_path,
        Some(Arc::clone(&daemon_db)),
        Some(primary_id.clone()),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await
    .expect("handler should initialize");

    let paired_path = paired_root.canonicalize().unwrap();
    let paired_path_str = paired_path.to_string_lossy().to_string();
    let paired_id = generate_workspace_id(&paired_path_str).unwrap();
    daemon_db
        .upsert_workspace(&paired_id, &paired_path_str, "ready")
        .unwrap();
    daemon_db.add_reference(&primary_id, &paired_id).unwrap();

    let known_path = known_root.canonicalize().unwrap();
    let known_path_str = known_path.to_string_lossy().to_string();
    let known_id = generate_workspace_id(&known_path_str).unwrap();
    daemon_db
        .upsert_workspace(&known_id, &known_path_str, "ready")
        .unwrap();

    let result = ManageWorkspaceTool {
        operation: "list".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect("list should succeed");

    let text = extract_text_from_result(&result);
    assert!(
        text.contains(&primary_id),
        "list should include current workspace: {text}"
    );
    assert!(
        text.contains(&paired_id),
        "list should include paired workspace: {text}"
    );
    assert!(
        text.contains(&known_id),
        "list should include unpaired known workspace: {text}"
    );
    assert!(
        text.contains("CURRENT"),
        "list should annotate current workspace: {text}"
    );
    assert!(
        text.contains("PAIRED"),
        "list should annotate paired workspace: {text}"
    );
    assert!(
        text.contains("KNOWN"),
        "list should annotate unpaired known workspace: {text}"
    );
}

#[tokio::test]
async fn test_manage_workspace_list_uses_session_primary_binding_over_legacy_workspace_id() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let legacy_primary_root = temp_dir.path().join("legacy-primary");
    let rebound_primary_root = temp_dir.path().join("rebound-primary");
    let paired_root = temp_dir.path().join("paired");
    fs::create_dir_all(&legacy_primary_root).unwrap();
    fs::create_dir_all(&rebound_primary_root).unwrap();
    fs::create_dir_all(&paired_root).unwrap();
    fs::write(
        legacy_primary_root.join("main.rs"),
        "fn legacy_primary() {}\n",
    )
    .unwrap();
    fs::write(
        rebound_primary_root.join("lib.rs"),
        "fn rebound_primary() {}\n",
    )
    .unwrap();
    fs::write(paired_root.join("lib.rs"), "fn paired() {}\n").unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));

    let legacy_primary_path = legacy_primary_root.canonicalize().unwrap();
    let legacy_primary_path_str = legacy_primary_path.to_string_lossy().to_string();
    let legacy_primary_id = generate_workspace_id(&legacy_primary_path_str).unwrap();
    daemon_db
        .upsert_workspace(&legacy_primary_id, &legacy_primary_path_str, "ready")
        .unwrap();

    let legacy_primary_ws = pool
        .get_or_init(&legacy_primary_id, legacy_primary_path.clone())
        .await
        .expect("legacy primary workspace should initialize");
    let handler = JulieServerHandler::new_with_shared_workspace(
        legacy_primary_ws,
        legacy_primary_path,
        Some(Arc::clone(&daemon_db)),
        Some(legacy_primary_id.clone()),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await
    .expect("handler should initialize");

    let rebound_primary_path = rebound_primary_root.canonicalize().unwrap();
    let rebound_primary_path_str = rebound_primary_path.to_string_lossy().to_string();
    let rebound_primary_id = generate_workspace_id(&rebound_primary_path_str).unwrap();
    daemon_db
        .upsert_workspace(&rebound_primary_id, &rebound_primary_path_str, "ready")
        .unwrap();

    let paired_path = paired_root.canonicalize().unwrap();
    let paired_path_str = paired_path.to_string_lossy().to_string();
    let paired_id = generate_workspace_id(&paired_path_str).unwrap();
    daemon_db
        .upsert_workspace(&paired_id, &paired_path_str, "ready")
        .unwrap();
    daemon_db
        .add_reference(&rebound_primary_id, &paired_id)
        .unwrap();

    handler.set_current_primary_binding(rebound_primary_id.clone(), rebound_primary_path);

    let result = ManageWorkspaceTool {
        operation: "list".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect("list should succeed");

    let text = extract_text_from_result(&result);
    assert!(
        text.contains(&format!("({}) [CURRENT]", rebound_primary_id)),
        "list should mark rebound session primary as CURRENT: {text}"
    );
    assert!(
        text.contains(&format!("({}) [PAIRED]", paired_id)),
        "list should load pairings from rebound session primary: {text}"
    );
    assert!(
        text.contains(&format!("({}) [KNOWN]", legacy_primary_id)),
        "legacy workspace_id should no longer drive CURRENT labeling: {text}"
    );
}

#[tokio::test]
async fn test_manage_workspace_list_triggers_roots_resolution_when_primary_missing() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let startup_root = temp_dir.path().join("startup");
    let roots_root = temp_dir.path().join("roots");
    fs::create_dir_all(&startup_root).unwrap();
    fs::create_dir_all(&roots_root).unwrap();
    fs::write(startup_root.join("main.rs"), "fn startup() {}\n").unwrap();
    fs::write(roots_root.join("lib.rs"), "fn roots_primary() {}\n").unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));

    let startup_path = startup_root.canonicalize().unwrap();
    let startup_path_str = startup_path.to_string_lossy().to_string();
    let startup_id = generate_workspace_id(&startup_path_str).unwrap();
    daemon_db
        .upsert_workspace(&startup_id, &startup_path_str, "ready")
        .unwrap();

    let roots_path = roots_root.canonicalize().unwrap();
    let roots_path_str = roots_path.to_string_lossy().to_string();
    let roots_id = generate_workspace_id(&roots_path_str).unwrap();
    daemon_db
        .upsert_workspace(&roots_id, &roots_path_str, "ready")
        .unwrap();

    let startup_ws = pool
        .get_or_init(&startup_id, startup_path.clone())
        .await
        .expect("startup workspace should initialize");
    let handler = JulieServerHandler::new_with_shared_workspace_startup_hint(
        startup_ws,
        crate::workspace::startup_hint::WorkspaceStartupHint {
            path: startup_path.clone(),
            source: Some(crate::workspace::startup_hint::WorkspaceStartupSource::Cwd),
        },
        Some(Arc::clone(&daemon_db)),
        Some(startup_id.clone()),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await
    .expect("handler should initialize");
    handler.set_client_supports_workspace_roots_for_test(true);
    assert_eq!(handler.current_workspace_id(), None);

    let (server_transport, client_transport) = tokio::io::duplex(256);
    let service =
        serve_directly::<rmcp::RoleServer, _, _, _, _>(handler.clone(), server_transport, None);
    let (read_half, mut write_half) = tokio::io::split(client_transport);
    let mut lines = BufReader::new(read_half).lines();

    let roots_reply = async {
        match read_server_message(&mut lines).await {
            ServerJsonRpcMessage::Request(request) => match request.request {
                ServerRequest::ListRootsRequest(_) => {
                    send_json_line(
                        &mut write_half,
                        &serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": request.id,
                            "result": {
                                "roots": [{ "uri": format!("file://{}", roots_path.to_string_lossy()) }]
                            }
                        }),
                    )
                    .await;
                }
                other => panic!("unexpected server request: {other:?}"),
            },
            other => panic!("unexpected server message: {other:?}"),
        }
    };

    let list = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        CallToolRequestParams::new("manage_workspace").with_arguments(
            serde_json::json!({ "operation": "list" })
                .as_object()
                .expect("manage_workspace args")
                .clone(),
        ),
        RequestContext::new(NumberOrString::Number(9), service.peer().clone()),
    );
    let (_, result) = tokio::join!(roots_reply, list);
    let result = result.expect("manage_workspace list should resolve primary from roots");

    let message = extract_text_from_result(&result);
    assert!(
        message.contains(&roots_id),
        "manage_workspace list should succeed after roots resolution: {message}"
    );
    assert_eq!(
        handler.current_workspace_id().as_deref(),
        Some(roots_id.as_str()),
        "manage_workspace list should bind the roots-selected current primary"
    );

    drop(write_half);
    drop(lines);
    let _ = service.cancel().await;
}

#[tokio::test]
async fn test_manage_workspace_stats_include_all_known_workspaces() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let primary_root = temp_dir.path().join("primary");
    let paired_root = temp_dir.path().join("paired");
    let known_root = temp_dir.path().join("known");
    fs::create_dir_all(&primary_root).unwrap();
    fs::create_dir_all(&paired_root).unwrap();
    fs::create_dir_all(&known_root).unwrap();
    fs::write(primary_root.join("main.rs"), "fn primary() {}\n").unwrap();
    fs::write(paired_root.join("lib.rs"), "fn paired() {}\n").unwrap();
    fs::write(known_root.join("lib.rs"), "fn known() {}\n").unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));

    let primary_path = primary_root.canonicalize().unwrap();
    let primary_path_str = primary_path.to_string_lossy().to_string();
    let primary_id = generate_workspace_id(&primary_path_str).unwrap();
    daemon_db
        .upsert_workspace(&primary_id, &primary_path_str, "ready")
        .unwrap();
    daemon_db
        .update_workspace_stats(&primary_id, 10, 2, None, None, None)
        .unwrap();

    let primary_ws = pool
        .get_or_init(&primary_id, primary_path.clone())
        .await
        .expect("primary workspace should initialize");
    let handler = JulieServerHandler::new_with_shared_workspace(
        primary_ws,
        primary_path,
        Some(Arc::clone(&daemon_db)),
        Some(primary_id.clone()),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await
    .expect("handler should initialize");

    let paired_path = paired_root.canonicalize().unwrap();
    let paired_path_str = paired_path.to_string_lossy().to_string();
    let paired_id = generate_workspace_id(&paired_path_str).unwrap();
    daemon_db
        .upsert_workspace(&paired_id, &paired_path_str, "ready")
        .unwrap();
    daemon_db
        .update_workspace_stats(&paired_id, 20, 3, None, None, None)
        .unwrap();
    daemon_db.add_reference(&primary_id, &paired_id).unwrap();

    let known_path = known_root.canonicalize().unwrap();
    let known_path_str = known_path.to_string_lossy().to_string();
    let known_id = generate_workspace_id(&known_path_str).unwrap();
    daemon_db
        .upsert_workspace(&known_id, &known_path_str, "ready")
        .unwrap();
    daemon_db
        .update_workspace_stats(&known_id, 30, 5, None, None, None)
        .unwrap();

    let result = ManageWorkspaceTool {
        operation: "stats".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect("stats should succeed");

    let text = extract_text_from_result(&result);
    assert!(
        text.contains("Known Workspaces: 3"),
        "stats should count all known workspaces: {text}"
    );
    assert!(
        text.contains("Current Workspace Pairings: 1"),
        "stats should label pairings accurately: {text}"
    );
    assert!(
        text.contains("Total Files: 10"),
        "old paired-only total should fail before fix: {text}"
    );
    assert!(
        text.contains("Total Symbols: 60"),
        "stats should aggregate all known symbols: {text}"
    );
}

#[tokio::test]
async fn test_manage_workspace_stats_rejects_neutral_gap_without_primary_identity() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let primary_root = temp_dir.path().join("primary");
    fs::create_dir_all(&primary_root).unwrap();
    fs::write(primary_root.join("main.rs"), "fn primary() {}\n").unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));

    let primary_path = primary_root.canonicalize().unwrap();
    let primary_path_str = primary_path.to_string_lossy().to_string();
    let primary_id = generate_workspace_id(&primary_path_str).unwrap();
    daemon_db
        .upsert_workspace(&primary_id, &primary_path_str, "ready")
        .unwrap();
    let primary_ws = pool
        .get_or_init(&primary_id, primary_path.clone())
        .await
        .expect("primary workspace should initialize");

    let handler = JulieServerHandler::new_with_shared_workspace(
        primary_ws,
        primary_path,
        Some(Arc::clone(&daemon_db)),
        Some(primary_id),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await
    .expect("handler should initialize");

    handler.publish_loaded_workspace_swap_intent_for_test();

    let err = ManageWorkspaceTool {
        operation: "stats".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect_err("neutral gap should reject workspace stats requests");

    assert!(
        err.to_string()
            .contains("Primary workspace identity unavailable during swap"),
        "unexpected error: {err:#}"
    );
}

#[tokio::test]
async fn test_manage_workspace_stats_explicit_target_succeeds_without_bound_primary_in_deferred_session()
 {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let startup_root = temp_dir.path().join("startup");
    let target_root = temp_dir.path().join("target");
    fs::create_dir_all(&startup_root).unwrap();
    fs::create_dir_all(&target_root).unwrap();
    fs::write(startup_root.join("main.rs"), "fn startup() {}\n").unwrap();
    fs::write(target_root.join("lib.rs"), "fn target() {}\n").unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));

    let startup_path = startup_root.canonicalize().unwrap();
    let startup_id = generate_workspace_id(&startup_path.to_string_lossy()).unwrap();
    let startup_ws = pool
        .get_or_init(&startup_id, startup_path.clone())
        .await
        .expect("startup workspace should initialize");

    let handler = JulieServerHandler::new_with_shared_workspace_startup_hint(
        startup_ws,
        crate::workspace::startup_hint::WorkspaceStartupHint {
            path: startup_path.clone(),
            source: Some(crate::workspace::startup_hint::WorkspaceStartupSource::Cwd),
        },
        Some(Arc::clone(&daemon_db)),
        Some(startup_id),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await
    .expect("handler should initialize");
    assert_eq!(handler.current_workspace_id(), None);

    let target_path = target_root.canonicalize().unwrap();
    let target_path_str = target_path.to_string_lossy().to_string();
    let target_id = generate_workspace_id(&target_path_str).unwrap();
    daemon_db
        .upsert_workspace(&target_id, &target_path_str, "ready")
        .unwrap();
    daemon_db
        .update_workspace_stats(&target_id, 17, 4, None, None, None)
        .unwrap();

    let result = ManageWorkspaceTool {
        operation: "stats".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: Some(target_id.clone()),
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect("explicit-target stats should succeed without a currently bound primary");

    let text = extract_text_from_result(&result);
    assert!(
        text.contains(&format!("Workspace Statistics: {target_id}")),
        "explicit-target stats should return the requested workspace: {text}"
    );
    assert!(
        text.contains("Files: 4 | Symbols: 17"),
        "explicit-target stats should use target workspace stats: {text}"
    );
    assert_eq!(
        handler.current_workspace_id(),
        None,
        "explicit-target stats must not bind the deferred primary workspace"
    );
}

#[tokio::test]
async fn test_manage_workspace_refresh_by_workspace_id_succeeds_without_bound_primary_in_deferred_session()
 {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let startup_root = temp_dir.path().join("startup");
    let target_root = temp_dir.path().join("target");
    fs::create_dir_all(&startup_root).unwrap();
    fs::create_dir_all(&target_root).unwrap();
    fs::write(startup_root.join("main.rs"), "fn startup() {}\n").unwrap();
    fs::write(target_root.join("lib.rs"), "fn target() {}\n").unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));

    let startup_path = startup_root.canonicalize().unwrap();
    let startup_id = generate_workspace_id(&startup_path.to_string_lossy()).unwrap();
    let startup_ws = pool
        .get_or_init(&startup_id, startup_path.clone())
        .await
        .expect("startup workspace should initialize");

    let handler = JulieServerHandler::new_with_shared_workspace_startup_hint(
        startup_ws,
        crate::workspace::startup_hint::WorkspaceStartupHint {
            path: startup_path.clone(),
            source: Some(crate::workspace::startup_hint::WorkspaceStartupSource::Cwd),
        },
        Some(Arc::clone(&daemon_db)),
        Some(startup_id),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await
    .expect("handler should initialize");
    assert_eq!(handler.current_workspace_id(), None);

    let target_path = target_root.canonicalize().unwrap();
    let target_path_str = target_path.to_string_lossy().to_string();
    let target_id = generate_workspace_id(&target_path_str).unwrap();
    daemon_db
        .upsert_workspace(&target_id, &target_path_str, "ready")
        .unwrap();

    let result = ManageWorkspaceTool {
        operation: "refresh".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: Some(target_id.clone()),
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect("explicit-target refresh should succeed without a currently bound primary");

    let text = extract_text_from_result(&result);
    assert!(
        text.contains(&format!("Workspace Refresh: {target_id}")),
        "explicit-target refresh should return the requested workspace: {text}"
    );
    assert!(
        !text.contains("Workspace Refresh Failed"),
        "explicit-target refresh should not fail in a deferred session: {text}"
    );
    assert_eq!(
        handler.current_workspace_id(),
        None,
        "explicit-target refresh must not bind the deferred primary workspace"
    );
}

#[tokio::test]
async fn test_manage_workspace_open_by_workspace_id_succeeds_without_bound_primary_in_deferred_session()
 {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let startup_root = temp_dir.path().join("startup");
    let target_root = temp_dir.path().join("target");
    fs::create_dir_all(&startup_root).unwrap();
    fs::create_dir_all(&target_root).unwrap();
    fs::write(startup_root.join("main.rs"), "fn startup() {}\n").unwrap();
    fs::write(target_root.join("lib.rs"), "fn target() {}\n").unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));

    let startup_path = startup_root.canonicalize().unwrap();
    let startup_id = generate_workspace_id(&startup_path.to_string_lossy()).unwrap();
    let startup_ws = pool
        .get_or_init(&startup_id, startup_path.clone())
        .await
        .expect("startup workspace should initialize");

    let handler = JulieServerHandler::new_with_shared_workspace_startup_hint(
        startup_ws,
        crate::workspace::startup_hint::WorkspaceStartupHint {
            path: startup_path.clone(),
            source: Some(crate::workspace::startup_hint::WorkspaceStartupSource::Cwd),
        },
        Some(Arc::clone(&daemon_db)),
        Some(startup_id),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await
    .expect("handler should initialize");
    assert_eq!(handler.current_workspace_id(), None);

    let target_path = target_root.canonicalize().unwrap();
    let target_path_str = target_path.to_string_lossy().to_string();
    let target_id = generate_workspace_id(&target_path_str).unwrap();
    daemon_db
        .upsert_workspace(&target_id, &target_path_str, "ready")
        .unwrap();

    let result = ManageWorkspaceTool {
        operation: "open".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: Some(target_id.clone()),
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect("explicit-target open should succeed without a currently bound primary");

    let text = extract_text_from_result(&result);
    assert!(
        text.contains("Workspace Opened") && text.contains(&format!("Workspace ID: {target_id}")),
        "explicit-target open should return the requested workspace: {text}"
    );
    assert!(
        handler.is_workspace_active(&target_id).await,
        "known workspace should be active after explicit-target open"
    );
}

#[tokio::test]
#[serial]
async fn test_remove_workspace_uses_global_index_dir_shape() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let fake_home = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let original_home = std::env::var("HOME").ok();
    #[cfg(windows)]
    let original_userprofile = std::env::var("USERPROFILE").ok();

    unsafe {
        std::env::set_var("HOME", fake_home.path());
        #[cfg(windows)]
        std::env::set_var("USERPROFILE", fake_home.path());
    }

    let primary_root = temp_dir.path().join("primary");
    let target_root = temp_dir.path().join("target");
    fs::create_dir_all(&primary_root).unwrap();
    fs::create_dir_all(&target_root).unwrap();
    fs::write(primary_root.join("main.rs"), "fn primary() {}\n").unwrap();
    fs::write(target_root.join("lib.rs"), "fn target() {}\n").unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));

    let primary_path = primary_root.canonicalize().unwrap();
    let primary_path_str = primary_path.to_string_lossy().to_string();
    let primary_id = generate_workspace_id(&primary_path_str).unwrap();
    daemon_db
        .upsert_workspace(&primary_id, &primary_path_str, "ready")
        .unwrap();

    let primary_ws = pool
        .get_or_init(&primary_id, primary_path.clone())
        .await
        .expect("primary workspace should initialize");
    let handler = JulieServerHandler::new_with_shared_workspace(
        Arc::clone(&primary_ws),
        primary_path,
        Some(Arc::clone(&daemon_db)),
        Some(primary_id.clone()),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await
    .expect("handler should initialize");

    let target_path = target_root.canonicalize().unwrap();
    let target_path_str = target_path.to_string_lossy().to_string();
    let target_id = generate_workspace_id(&target_path_str).unwrap();
    daemon_db
        .upsert_workspace(&target_id, &target_path_str, "ready")
        .unwrap();
    daemon_db.add_reference(&primary_id, &target_id).unwrap();

    let daemon_paths = DaemonPaths::new();
    let global_index_dir = daemon_paths.workspace_index_dir(&target_id);
    fs::create_dir_all(global_index_dir.join("db")).unwrap();
    fs::write(global_index_dir.join("db").join("symbols.db"), "target-db").unwrap();

    let legacy_nested_dir = primary_ws.indexes_root_path().join(&target_id);
    fs::create_dir_all(legacy_nested_dir.join("db")).unwrap();
    fs::write(legacy_nested_dir.join("db").join("symbols.db"), "legacy-db").unwrap();

    let result = ManageWorkspaceTool {
        operation: "remove".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: Some(target_id.clone()),
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect("remove should succeed");

    let text = extract_text_from_result(&result);
    assert!(
        text.contains("Workspace Removed Successfully"),
        "remove should confirm success: {text}"
    );
    assert!(
        !global_index_dir.exists(),
        "remove should delete the global daemon index directory shape"
    );
    assert!(
        legacy_nested_dir.exists(),
        "remove should no longer target the old nested-under-primary layout"
    );

    unsafe {
        if let Some(val) = original_home {
            std::env::set_var("HOME", val);
        } else {
            std::env::remove_var("HOME");
        }
        #[cfg(windows)]
        {
            if let Some(val) = original_userprofile {
                std::env::set_var("USERPROFILE", val);
            } else {
                std::env::remove_var("USERPROFILE");
            }
        }
    }
}

#[tokio::test]
async fn test_unknown_workspace_suggests_closest_match() {
    let (_temp_dir, handler, target_id) = setup_known_reference_search_workspace().await;
    let typo_workspace_id = format!("{}x", target_id);

    let result = FastSearchTool {
        query: "target_search_marker".to_string(),
        limit: 10,
        search_target: "content".to_string(),
        workspace: Some(typo_workspace_id.clone()),
        ..Default::default()
    }
    .call_tool(&handler)
    .await;

    let error = result.expect_err("unknown workspace typo should be rejected");
    let message = error.to_string();
    assert!(
        message.contains(&format!("Workspace '{}' not found", typo_workspace_id)),
        "error should identify the unknown workspace: {message}"
    );
    assert!(
        message.contains(&format!("Did you mean '{}'", target_id)),
        "error should suggest the closest known workspace id: {message}"
    );
}

#[tokio::test]
async fn test_opened_workspace_routes_fast_search_by_workspace_id() {
    let (_temp_dir, handler, target_id) = setup_known_reference_search_workspace().await;

    let open_result = ManageWorkspaceTool {
        operation: "open".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: Some(target_id.clone()),
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect("open should succeed for known workspace");
    let open_text = extract_text_from_result(&open_result);
    assert!(
        open_text.contains("Workspace Opened"),
        "open should confirm success: {open_text}"
    );
    assert!(handler.is_workspace_active(&target_id).await);

    let result = FastSearchTool {
        query: "target_search_marker".to_string(),
        limit: 10,
        search_target: "content".to_string(),
        workspace: Some(target_id),
        ..Default::default()
    }
    .call_tool(&handler)
    .await
    .expect("opened workspace should be searchable");

    let text = extract_text_from_result(&result);
    assert!(
        text.contains("target_search_marker"),
        "search should route to the opened reference workspace: {text}"
    );
}

#[tokio::test]
async fn test_manage_workspace_open_registers_missing_workspace_and_returns_workspace_id() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let primary_root = temp_dir.path().join("primary");
    let target_root = temp_dir.path().join("target");
    fs::create_dir_all(&primary_root).unwrap();
    fs::create_dir_all(&target_root).unwrap();
    fs::write(primary_root.join("main.rs"), "fn primary() {}\n").unwrap();
    fs::write(target_root.join("lib.rs"), "fn opened() {}\n").unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));

    let primary_path = primary_root.canonicalize().unwrap();
    let primary_id = generate_workspace_id(&primary_path.to_string_lossy()).unwrap();
    let primary_ws = pool
        .get_or_init(&primary_id, primary_path.clone())
        .await
        .expect("primary workspace should initialize");

    let handler = JulieServerHandler::new_with_shared_workspace(
        primary_ws,
        primary_path.clone(),
        Some(Arc::clone(&daemon_db)),
        Some(primary_id),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await
    .expect("handler should initialize");

    let target_path = target_root.canonicalize().unwrap();
    let target_path_str = target_path.to_string_lossy().to_string();
    let target_id = generate_workspace_id(&target_path_str).unwrap();

    let tool = ManageWorkspaceTool {
        operation: "open".to_string(),
        path: Some(target_path_str.clone()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };

    let result = tool.call_tool(&handler).await.unwrap();
    let text = extract_text_from_result(&result);

    assert!(
        text.contains(&target_id),
        "open result should include workspace ID: {text}"
    );
    assert!(
        text.contains(&target_path_str),
        "open result should include canonical path: {text}"
    );
    assert!(
        handler.is_workspace_active(&target_id).await,
        "opened workspace should be active for the session"
    );

    let row = daemon_db
        .get_workspace(&target_id)
        .unwrap()
        .expect("workspace should be registered in daemon db");
    assert_eq!(row.path, target_path_str);
    assert_eq!(row.status, "ready");
}

#[tokio::test]
async fn test_manage_workspace_add_uses_session_primary_binding_over_legacy_workspace_id() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let legacy_primary_root = temp_dir.path().join("legacy-primary");
    let rebound_primary_root = temp_dir.path().join("rebound-primary");
    let reference_root = temp_dir.path().join("reference");
    fs::create_dir_all(&legacy_primary_root).unwrap();
    fs::create_dir_all(&rebound_primary_root).unwrap();
    fs::create_dir_all(&reference_root).unwrap();
    fs::write(
        legacy_primary_root.join("main.rs"),
        "fn legacy_primary() {}\n",
    )
    .unwrap();
    fs::write(
        rebound_primary_root.join("lib.rs"),
        "fn rebound_primary() {}\n",
    )
    .unwrap();
    fs::write(
        reference_root.join("lib.rs"),
        "pub fn reference_marker() {}\n",
    )
    .unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));

    let legacy_primary_path = legacy_primary_root.canonicalize().unwrap();
    let legacy_primary_path_str = legacy_primary_path.to_string_lossy().to_string();
    let legacy_primary_id = generate_workspace_id(&legacy_primary_path_str).unwrap();
    daemon_db
        .upsert_workspace(&legacy_primary_id, &legacy_primary_path_str, "ready")
        .unwrap();

    let legacy_primary_ws = pool
        .get_or_init(&legacy_primary_id, legacy_primary_path.clone())
        .await
        .expect("legacy primary workspace should initialize");
    let handler = JulieServerHandler::new_with_shared_workspace(
        legacy_primary_ws,
        legacy_primary_path,
        Some(Arc::clone(&daemon_db)),
        Some(legacy_primary_id.clone()),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await
    .expect("handler should initialize");

    let rebound_primary_path = rebound_primary_root.canonicalize().unwrap();
    let rebound_primary_path_str = rebound_primary_path.to_string_lossy().to_string();
    let rebound_primary_id = generate_workspace_id(&rebound_primary_path_str).unwrap();
    daemon_db
        .upsert_workspace(&rebound_primary_id, &rebound_primary_path_str, "ready")
        .unwrap();
    handler.set_current_primary_binding(rebound_primary_id.clone(), rebound_primary_path);

    let reference_path = reference_root.canonicalize().unwrap();
    let reference_path_str = reference_path.to_string_lossy().to_string();
    let reference_id = generate_workspace_id(&reference_path_str).unwrap();

    let result = ManageWorkspaceTool {
        operation: "add".to_string(),
        path: Some(reference_path_str.clone()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect("add should succeed");

    let text = extract_text_from_result(&result);
    assert!(
        text.contains(&reference_id),
        "add output should include reference workspace id: {text}"
    );

    let rebound_refs = daemon_db
        .list_references(&rebound_primary_id)
        .expect("rebound session primary references should load");
    assert!(
        rebound_refs
            .iter()
            .any(|ws| ws.workspace_id == reference_id),
        "reference should be paired to rebound session primary"
    );

    let legacy_refs = daemon_db
        .list_references(&legacy_primary_id)
        .expect("legacy primary references should load");
    assert!(
        legacy_refs.iter().all(|ws| ws.workspace_id != reference_id),
        "stale workspace_id should not receive the new pairing"
    );
}

#[tokio::test]
async fn test_manage_workspace_open_by_workspace_id_marks_known_workspace_active() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let primary_root = temp_dir.path().join("primary");
    let target_root = temp_dir.path().join("target");
    fs::create_dir_all(&primary_root).unwrap();
    fs::create_dir_all(&target_root).unwrap();
    fs::write(primary_root.join("main.rs"), "fn primary() {}\n").unwrap();
    fs::write(target_root.join("lib.rs"), "fn known_target() {}\n").unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));

    let primary_path = primary_root.canonicalize().unwrap();
    let primary_id = generate_workspace_id(&primary_path.to_string_lossy()).unwrap();
    let primary_ws = pool
        .get_or_init(&primary_id, primary_path.clone())
        .await
        .expect("primary workspace should initialize");

    let handler = JulieServerHandler::new_with_shared_workspace(
        primary_ws,
        primary_path.clone(),
        Some(Arc::clone(&daemon_db)),
        Some(primary_id),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await
    .expect("handler should initialize");

    let target_path = target_root.canonicalize().unwrap();
    let target_path_str = target_path.to_string_lossy().to_string();
    let target_id = generate_workspace_id(&target_path_str).unwrap();

    daemon_db
        .upsert_workspace(&target_id, &target_path_str, "ready")
        .unwrap();

    let tool = ManageWorkspaceTool {
        operation: "open".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: Some(target_id.clone()),
        detailed: None,
    };

    let result = tool.call_tool(&handler).await.unwrap();
    let text = extract_text_from_result(&result);

    assert!(
        text.contains(&target_id),
        "open-by-id result should include workspace ID: {text}"
    );
    assert!(
        text.contains(&target_path_str),
        "open-by-id result should include workspace path: {text}"
    );
    assert!(
        handler.is_workspace_active(&target_id).await,
        "known workspace should be active after open"
    );
}

#[tokio::test]
async fn test_manage_workspace_open_does_not_activate_workspace_when_refresh_fails() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let primary_root = temp_dir.path().join("primary");
    fs::create_dir_all(&primary_root).unwrap();
    fs::write(primary_root.join("main.rs"), "fn primary() {}\n").unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));

    let primary_path = primary_root.canonicalize().unwrap();
    let primary_id = generate_workspace_id(&primary_path.to_string_lossy()).unwrap();
    let primary_ws = pool
        .get_or_init(&primary_id, primary_path.clone())
        .await
        .expect("primary workspace should initialize");

    let handler = JulieServerHandler::new_with_shared_workspace(
        primary_ws,
        primary_path,
        Some(Arc::clone(&daemon_db)),
        Some(primary_id),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await
    .expect("handler should initialize");

    let missing_target = temp_dir.path().join("missing-target");
    let target_path_str = missing_target.to_string_lossy().to_string();
    let target_id = generate_workspace_id(&target_path_str).unwrap();
    daemon_db
        .upsert_workspace(&target_id, &target_path_str, "ready")
        .unwrap();

    let tool = ManageWorkspaceTool {
        operation: "open".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: Some(target_id.clone()),
        detailed: None,
    };

    let result = tool.call_tool(&handler).await.unwrap();
    let text = extract_text_from_result(&result);

    assert!(
        text.contains("Workspace Refresh Failed") || text.contains("Failed"),
        "open should surface refresh failure text: {text}"
    );
    assert!(
        !handler.is_workspace_active(&target_id).await,
        "workspace should remain inactive when refresh fails"
    );
}

#[tokio::test]
async fn test_manage_workspace_open_is_idempotent_for_active_workspace() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let primary_root = temp_dir.path().join("primary");
    let target_root = temp_dir.path().join("target");
    fs::create_dir_all(&primary_root).unwrap();
    fs::create_dir_all(&target_root).unwrap();
    fs::write(primary_root.join("main.rs"), "fn primary() {}\n").unwrap();
    fs::write(target_root.join("lib.rs"), "fn known_target() {}\n").unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));

    let primary_path = primary_root.canonicalize().unwrap();
    let primary_id = generate_workspace_id(&primary_path.to_string_lossy()).unwrap();
    let primary_ws = pool
        .get_or_init(&primary_id, primary_path.clone())
        .await
        .expect("primary workspace should initialize");

    let handler = JulieServerHandler::new_with_shared_workspace(
        primary_ws,
        primary_path,
        Some(Arc::clone(&daemon_db)),
        Some(primary_id),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await
    .expect("handler should initialize");

    let target_path = target_root.canonicalize().unwrap();
    let target_path_str = target_path.to_string_lossy().to_string();
    let target_id = generate_workspace_id(&target_path_str).unwrap();
    daemon_db
        .upsert_workspace(&target_id, &target_path_str, "ready")
        .unwrap();

    let tool = ManageWorkspaceTool {
        operation: "open".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: Some(target_id.clone()),
        detailed: None,
    };

    tool.call_tool(&handler).await.unwrap();
    let first_row = daemon_db
        .get_workspace(&target_id)
        .unwrap()
        .expect("target workspace should exist after first open");
    assert_eq!(
        first_row.session_count, 1,
        "first open should attach the workspace once"
    );

    tool.call_tool(&handler).await.unwrap();
    let second_row = daemon_db
        .get_workspace(&target_id)
        .unwrap()
        .expect("target workspace should exist after second open");
    assert_eq!(
        second_row.session_count, 1,
        "second open in the same session must not increment session_count"
    );
}

#[tokio::test]
async fn test_manage_workspace_open_short_circuits_when_active() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let primary_root = temp_dir.path().join("primary");
    let target_root = temp_dir.path().join("target");
    fs::create_dir_all(&primary_root).unwrap();
    fs::create_dir_all(&target_root).unwrap();
    fs::write(primary_root.join("main.rs"), "fn primary() {}\n").unwrap();
    fs::write(target_root.join("lib.rs"), "fn known_target() {}\n").unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));

    let primary_path = primary_root.canonicalize().unwrap();
    let primary_id = generate_workspace_id(&primary_path.to_string_lossy()).unwrap();
    let primary_ws = pool
        .get_or_init(&primary_id, primary_path)
        .await
        .expect("primary workspace should initialize");

    let handler = JulieServerHandler::new_with_shared_workspace(
        primary_ws,
        primary_root.canonicalize().unwrap(),
        Some(Arc::clone(&daemon_db)),
        Some(primary_id),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await
    .expect("handler should initialize");

    let target_path = target_root.canonicalize().unwrap();
    let target_path_str = target_path.to_string_lossy().to_string();
    let target_id = generate_workspace_id(&target_path_str).unwrap();
    daemon_db
        .upsert_workspace(&target_id, &target_path_str, "ready")
        .unwrap();

    let tool = ManageWorkspaceTool {
        operation: "open".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: Some(target_id.clone()),
        detailed: None,
    };

    let first = tool.call_tool(&handler).await.unwrap();
    let first_text = extract_text_from_result(&first);
    assert!(first_text.contains("Workspace Opened"));
    assert!(handler.is_workspace_active(&target_id).await);

    let renamed_target = temp_dir.path().join("target-renamed");
    fs::rename(&target_root, &renamed_target).unwrap();

    let second = tool.call_tool(&handler).await.unwrap();
    let second_text = extract_text_from_result(&second);
    assert!(
        second_text.contains("Workspace Opened"),
        "active workspace reopen should short-circuit before refresh: {second_text}"
    );
    assert!(
        handler.is_workspace_active(&target_id).await,
        "workspace should remain active after short-circuited reopen"
    );
}

#[tokio::test]
async fn test_manage_workspace_open_force_active_workspace_runs_refresh() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let primary_root = temp_dir.path().join("primary");
    let target_root = temp_dir.path().join("target");
    fs::create_dir_all(&primary_root).unwrap();
    fs::create_dir_all(&target_root).unwrap();
    fs::write(primary_root.join("main.rs"), "fn primary() {}\n").unwrap();
    fs::write(target_root.join("lib.rs"), "fn known_target() {}\n").unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));

    let primary_path = primary_root.canonicalize().unwrap();
    let primary_id = generate_workspace_id(&primary_path.to_string_lossy()).unwrap();
    let primary_ws = pool
        .get_or_init(&primary_id, primary_path)
        .await
        .expect("primary workspace should initialize");

    let handler = JulieServerHandler::new_with_shared_workspace(
        primary_ws,
        primary_root.canonicalize().unwrap(),
        Some(Arc::clone(&daemon_db)),
        Some(primary_id),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await
    .expect("handler should initialize");

    let target_path = target_root.canonicalize().unwrap();
    let target_path_str = target_path.to_string_lossy().to_string();
    let target_id = generate_workspace_id(&target_path_str).unwrap();
    daemon_db
        .upsert_workspace(&target_id, &target_path_str, "ready")
        .unwrap();

    let open_tool = ManageWorkspaceTool {
        operation: "open".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: Some(target_id.clone()),
        detailed: None,
    };

    let initial = open_tool.call_tool(&handler).await.unwrap();
    let initial_text = extract_text_from_result(&initial);
    assert!(initial_text.contains("Workspace Opened"));
    assert!(handler.is_workspace_active(&target_id).await);

    let renamed_target = temp_dir.path().join("target-renamed");
    fs::rename(&target_root, &renamed_target).unwrap();

    let force_open_tool = ManageWorkspaceTool {
        operation: "open".to_string(),
        path: None,
        force: Some(true),
        name: None,
        workspace_id: Some(target_id.clone()),
        detailed: None,
    };

    let forced = force_open_tool.call_tool(&handler).await.unwrap();
    let forced_text = extract_text_from_result(&forced);
    assert!(
        forced_text.contains("Workspace Refresh Failed") || forced_text.contains("Failed"),
        "force open should surface refresh failure text: {forced_text}"
    );
    assert!(
        handler.is_workspace_active(&target_id).await,
        "failed force refresh should not silently deactivate the active workspace"
    );
}

#[tokio::test]
async fn test_manage_workspace_open_uses_session_primary_binding_over_legacy_workspace_id() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let primary_root = temp_dir.path().join("primary");
    let target_root = temp_dir.path().join("target");
    fs::create_dir_all(&primary_root).unwrap();
    fs::create_dir_all(&target_root).unwrap();
    fs::write(primary_root.join("main.rs"), "fn primary() {}\n").unwrap();
    fs::write(target_root.join("lib.rs"), "fn target() {}\n").unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));

    let primary_path = primary_root.canonicalize().unwrap();
    let primary_path_str = primary_path.to_string_lossy().to_string();
    let primary_id = generate_workspace_id(&primary_path_str).unwrap();
    let primary_ws = pool
        .get_or_init(&primary_id, primary_path.clone())
        .await
        .expect("primary workspace should initialize");

    let handler = JulieServerHandler::new_with_shared_workspace(
        primary_ws,
        primary_path,
        Some(Arc::clone(&daemon_db)),
        Some(primary_id.clone()),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await
    .expect("handler should initialize");

    let target_path = target_root.canonicalize().unwrap();
    let target_path_str = target_path.to_string_lossy().to_string();
    let target_id = generate_workspace_id(&target_path_str).unwrap();
    daemon_db
        .upsert_workspace(&target_id, &target_path_str, "ready")
        .unwrap();

    handler.set_current_primary_binding(target_id.clone(), target_path.clone());

    let stats_tool = ManageWorkspaceTool {
        operation: "stats".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    let stats_result = stats_tool.call_tool(&handler).await.unwrap();
    let stats_text = extract_text_from_result(&stats_result);
    assert!(
        stats_text.contains(&format!("Current Workspace: {}", target_id)),
        "stats should use session primary binding, not stale workspace_id: {stats_text}"
    );

    let renamed_target = temp_dir.path().join("target-renamed");
    fs::rename(&target_root, &renamed_target).unwrap();

    let open_tool = ManageWorkspaceTool {
        operation: "open".to_string(),
        path: None,
        force: Some(true),
        name: None,
        workspace_id: Some(target_id.clone()),
        detailed: None,
    };
    let open_result = open_tool.call_tool(&handler).await.unwrap();
    let open_text = extract_text_from_result(&open_result);
    assert!(
        open_text.contains("Workspace Opened"),
        "open should treat rebound session primary as primary: {open_text}"
    );
    assert!(
        !open_text.contains("Workspace Refresh Failed"),
        "open should not refresh a rebound session primary: {open_text}"
    );
}

#[tokio::test]
async fn test_manage_workspace_index_rebind_uses_workspace_pool_session_state() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let workspace_a_root = temp_dir.path().join("workspace-a");
    let workspace_b_root = temp_dir.path().join("workspace-b");
    fs::create_dir_all(&workspace_a_root).unwrap();
    fs::create_dir_all(&workspace_b_root).unwrap();
    fs::write(workspace_a_root.join("main.rs"), "fn workspace_a() {}\n").unwrap();
    fs::write(workspace_b_root.join("lib.rs"), "fn workspace_b() {}\n").unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));

    let workspace_a_path = workspace_a_root.canonicalize().unwrap();
    let workspace_a_path_str = workspace_a_path.to_string_lossy().to_string();
    let workspace_a_id = generate_workspace_id(&workspace_a_path_str).unwrap();
    let workspace_a_ws = pool
        .get_or_init(&workspace_a_id, workspace_a_path.clone())
        .await
        .expect("workspace A should initialize");

    let handler = JulieServerHandler::new_with_shared_workspace(
        workspace_a_ws,
        workspace_a_path.clone(),
        Some(Arc::clone(&daemon_db)),
        Some(workspace_a_id.clone()),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await
    .expect("handler should initialize");

    let workspace_b_path = workspace_b_root.canonicalize().unwrap();
    let workspace_b_path_str = workspace_b_path.to_string_lossy().to_string();
    let workspace_b_id = generate_workspace_id(&workspace_b_path_str).unwrap();
    daemon_db
        .upsert_workspace(&workspace_a_id, &workspace_a_path_str, "ready")
        .unwrap();
    daemon_db
        .upsert_workspace(&workspace_b_id, &workspace_b_path_str, "ready")
        .unwrap();

    handler.set_current_primary_binding(workspace_b_id.clone(), workspace_b_path.clone());

    let index_result = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect("index-driven rebind should succeed");
    let index_text = extract_text_from_result(&index_result);
    assert!(
        index_text.contains("Workspace indexing complete")
            || index_text.contains("Workspace already indexed"),
        "index-driven rebind should complete: {index_text}"
    );

    let rebound_row = daemon_db
        .get_workspace(&workspace_b_id)
        .unwrap()
        .expect("workspace B row should exist");
    assert_eq!(
        rebound_row.session_count, 1,
        "index-driven rebind should attach workspace B through the pool"
    );
    assert!(
        pool.get(&workspace_b_id).await.is_some(),
        "workspace B should be present in the pool after rebind"
    );

    let open_result = ManageWorkspaceTool {
        operation: "open".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: Some(workspace_b_id.clone()),
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect("open on rebound primary should succeed");
    let open_text = extract_text_from_result(&open_result);
    assert!(open_text.contains("Workspace Opened"));

    let rebound_row_after_open = daemon_db
        .get_workspace(&workspace_b_id)
        .unwrap()
        .expect("workspace B row should still exist");
    assert_eq!(
        rebound_row_after_open.session_count, 1,
        "open(B) after index-driven rebind should not skip or duplicate pool attachment"
    );
}

#[tokio::test]
async fn test_manage_workspace_index_path_rebind_updates_daemon_stats_for_new_primary() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let workspace_a_root = temp_dir.path().join("workspace-a");
    let workspace_b_root = temp_dir.path().join("workspace-b");
    fs::create_dir_all(&workspace_a_root).unwrap();
    fs::create_dir_all(&workspace_b_root).unwrap();
    fs::write(workspace_a_root.join("main.rs"), "fn workspace_a() {}\n").unwrap();
    fs::write(workspace_b_root.join("lib.rs"), "fn workspace_b() {}\n").unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));

    let workspace_a_path = workspace_a_root.canonicalize().unwrap();
    let workspace_a_path_str = workspace_a_path.to_string_lossy().to_string();
    let workspace_a_id = generate_workspace_id(&workspace_a_path_str).unwrap();
    let workspace_a_ws = pool
        .get_or_init(&workspace_a_id, workspace_a_path.clone())
        .await
        .expect("workspace A should initialize");

    let handler = JulieServerHandler::new_with_shared_workspace(
        workspace_a_ws,
        workspace_a_path.clone(),
        Some(Arc::clone(&daemon_db)),
        Some(workspace_a_id.clone()),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await
    .expect("handler should initialize");

    let workspace_b_path = workspace_b_root.canonicalize().unwrap();
    let workspace_b_path_str = workspace_b_path.to_string_lossy().to_string();
    let workspace_b_id = generate_workspace_id(&workspace_b_path_str).unwrap();
    daemon_db
        .upsert_workspace(&workspace_a_id, &workspace_a_path_str, "ready")
        .unwrap();
    let index_result = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_b_path_str.clone()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect("index(path=B) should perform the rebind");
    let index_text = extract_text_from_result(&index_result);
    assert!(
        index_text.contains("Workspace indexing complete")
            || index_text.contains("Workspace already indexed"),
        "index(path=B) should complete: {index_text}"
    );

    assert_eq!(handler.current_workspace_id(), Some(workspace_b_id.clone()));

    let workspace_b_row = daemon_db
        .get_workspace(&workspace_b_id)
        .unwrap()
        .expect("workspace B row should exist");
    assert_eq!(workspace_b_row.status, "ready");
    assert_eq!(workspace_b_row.session_count, 1);

    let workspace_a_row = daemon_db
        .get_workspace(&workspace_a_id)
        .unwrap()
        .expect("workspace A row should exist");
    assert_eq!(workspace_a_row.session_count, 1);
}

#[tokio::test]
async fn test_manage_workspace_index_path_succeeds_without_bound_primary_in_deferred_cwd_session_when_target_registered()
 {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let startup_root = temp_dir.path().join("startup");
    let target_root = temp_dir.path().join("target");
    fs::create_dir_all(&startup_root).unwrap();
    fs::create_dir_all(&target_root).unwrap();
    fs::write(startup_root.join("main.rs"), "fn startup() {}\n").unwrap();
    fs::write(target_root.join("lib.rs"), "fn target() {}\n").unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));

    let startup_path = startup_root.canonicalize().unwrap();
    let startup_id = generate_workspace_id(&startup_path.to_string_lossy()).unwrap();
    let startup_ws = pool
        .get_or_init(&startup_id, startup_path.clone())
        .await
        .expect("startup workspace should initialize");

    let handler = JulieServerHandler::new_with_shared_workspace_startup_hint(
        startup_ws,
        crate::workspace::startup_hint::WorkspaceStartupHint {
            path: startup_path.clone(),
            source: Some(crate::workspace::startup_hint::WorkspaceStartupSource::Cwd),
        },
        Some(Arc::clone(&daemon_db)),
        Some(startup_id.clone()),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await
    .expect("handler should initialize");
    assert_eq!(handler.current_workspace_id(), None);

    let target_path = target_root.canonicalize().unwrap();
    let target_id = generate_workspace_id(&target_path.to_string_lossy()).unwrap();
    daemon_db
        .upsert_workspace(&target_id, &target_path.to_string_lossy(), "ready")
        .unwrap();
    let result = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(target_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect("explicit-path index should succeed without a currently bound primary");

    let text = extract_text_from_result(&result);
    assert!(
        text.contains("Workspace indexing complete") || text.contains("Workspace already indexed"),
        "explicit-path index should complete: {text}"
    );
    assert_eq!(handler.current_workspace_id(), Some(target_id.clone()));

    let target_row = daemon_db
        .get_workspace(&target_id)
        .unwrap()
        .expect("target workspace row should exist after explicit-path index");
    assert_eq!(target_row.status, "ready");
}

#[tokio::test]
async fn test_manage_workspace_index_path_succeeds_without_bound_primary_in_deferred_cwd_session_when_target_unregistered()
 {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let startup_root = temp_dir.path().join("startup");
    let target_root = temp_dir.path().join("target");
    fs::create_dir_all(&startup_root).unwrap();
    fs::create_dir_all(&target_root).unwrap();
    fs::write(startup_root.join("main.rs"), "fn startup() {}\n").unwrap();
    fs::write(target_root.join("lib.rs"), "fn target() {}\n").unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));

    let startup_path = startup_root.canonicalize().unwrap();
    let startup_id = generate_workspace_id(&startup_path.to_string_lossy()).unwrap();
    let startup_ws = pool
        .get_or_init(&startup_id, startup_path.clone())
        .await
        .expect("startup workspace should initialize");

    let handler = JulieServerHandler::new_with_shared_workspace_startup_hint(
        startup_ws,
        crate::workspace::startup_hint::WorkspaceStartupHint {
            path: startup_path.clone(),
            source: Some(crate::workspace::startup_hint::WorkspaceStartupSource::Cwd),
        },
        Some(Arc::clone(&daemon_db)),
        Some(startup_id.clone()),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await
    .expect("handler should initialize");
    assert_eq!(handler.current_workspace_id(), None);

    let target_path = target_root.canonicalize().unwrap();
    let target_id = generate_workspace_id(&target_path.to_string_lossy()).unwrap();
    let result = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(target_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect("explicit-path index should succeed without a currently bound primary");

    let text = extract_text_from_result(&result);
    assert!(
        text.contains("Workspace indexing complete") || text.contains("Workspace already indexed"),
        "explicit-path index should complete: {text}"
    );
    assert_eq!(handler.current_workspace_id(), Some(target_id.clone()));

    let target_row = daemon_db
        .get_workspace(&target_id)
        .unwrap()
        .expect("target workspace row should exist after explicit-path index");
    assert_eq!(target_row.status, "ready");
}

#[tokio::test]
async fn test_manage_workspace_open_rebound_primary_still_attaches_pool() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let workspace_a_root = temp_dir.path().join("workspace-a");
    let workspace_b_root = temp_dir.path().join("workspace-b");
    fs::create_dir_all(&workspace_a_root).unwrap();
    fs::create_dir_all(&workspace_b_root).unwrap();
    fs::write(workspace_a_root.join("main.rs"), "fn workspace_a() {}\n").unwrap();
    fs::write(workspace_b_root.join("lib.rs"), "fn workspace_b() {}\n").unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));

    let workspace_a_path = workspace_a_root.canonicalize().unwrap();
    let workspace_a_path_str = workspace_a_path.to_string_lossy().to_string();
    let workspace_a_id = generate_workspace_id(&workspace_a_path_str).unwrap();
    let workspace_a_ws = pool
        .get_or_init(&workspace_a_id, workspace_a_path.clone())
        .await
        .expect("workspace A should initialize");

    let handler = JulieServerHandler::new_with_shared_workspace(
        workspace_a_ws,
        workspace_a_path,
        Some(Arc::clone(&daemon_db)),
        Some(workspace_a_id.clone()),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await
    .expect("handler should initialize");

    let workspace_b_path = workspace_b_root.canonicalize().unwrap();
    let workspace_b_path_str = workspace_b_path.to_string_lossy().to_string();
    let workspace_b_id = generate_workspace_id(&workspace_b_path_str).unwrap();
    daemon_db
        .upsert_workspace(&workspace_a_id, &workspace_a_path_str, "ready")
        .unwrap();
    daemon_db
        .upsert_workspace(&workspace_b_id, &workspace_b_path_str, "ready")
        .unwrap();

    handler.set_current_primary_binding(workspace_b_id.clone(), workspace_b_path.clone());

    let open_result = ManageWorkspaceTool {
        operation: "open".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: Some(workspace_b_id.clone()),
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect("open(B) after current-primary set should still attach through pool");
    let open_text = extract_text_from_result(&open_result);
    assert!(open_text.contains("Workspace Opened"));

    let workspace_b_row = daemon_db
        .get_workspace(&workspace_b_id)
        .unwrap()
        .expect("workspace B row should exist");
    assert_eq!(workspace_b_row.session_count, 1);
    assert!(pool.get(&workspace_b_id).await.is_some());

    let reopen_result = ManageWorkspaceTool {
        operation: "open".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: Some(workspace_b_id.clone()),
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect("reopening already-attached rebound primary should succeed");
    let reopen_text = extract_text_from_result(&reopen_result);
    assert!(reopen_text.contains("Workspace Opened"));

    let workspace_b_row_after_reopen = daemon_db
        .get_workspace(&workspace_b_id)
        .unwrap()
        .expect("workspace B row should still exist");
    assert_eq!(
        workspace_b_row_after_reopen.session_count, 1,
        "reopening an already-attached rebound primary must not increment session_count again"
    );
}
