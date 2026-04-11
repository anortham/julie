use std::fs;
use std::sync::Arc;
use std::sync::atomic::Ordering;

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
        .workspace_id
        .clone()
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
