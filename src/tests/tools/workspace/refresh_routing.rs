use std::fs;
use std::sync::Arc;

use rmcp::{
    ServerHandler,
    model::{CallToolRequestParams, NumberOrString},
    service::{RequestContext, serve_directly},
};

use crate::daemon::database::DaemonDatabase;
use crate::handler::JulieServerHandler;
use crate::mcp_compat::CallToolResult;
use crate::tools::workspace::ManageWorkspaceTool;
use crate::tools::workspace::indexing::route::IndexRoute;
use crate::workspace::registry::generate_workspace_id;

use crate::tests::helpers::workspace::make_isolated_workspace_root;

fn extract_text_from_result(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|content_block| {
            serde_json::to_value(content_block).ok().and_then(|json| {
                json.get("text")
                    .and_then(|value| value.as_str())
                    .map(|text| text.to_string())
            })
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
async fn test_primary_force_refresh_uses_full_index_path() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let primary_root = make_isolated_workspace_root(temp_dir.path(), "primary");
    fs::write(primary_root.join("main.rs"), "fn primary_marker() {}\n").unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());

    let primary_path = primary_root.canonicalize().unwrap();
    let primary_path_str = primary_path.to_string_lossy().to_string();
    let primary_id = generate_workspace_id(&primary_path_str).unwrap();
    let primary_ws = Arc::new(
        crate::workspace::JulieWorkspace::initialize(primary_path.clone())
            .await
            .expect("primary workspace should initialize"),
    );

    let handler = JulieServerHandler::new_with_shared_workspace(
        primary_ws,
        primary_path,
        Some(Arc::clone(&daemon_db)),
        Some(primary_id.clone()),
        None,
        None,
        None,
    )
    .await
    .expect("handler should initialize");

    daemon_db
        .upsert_workspace(&primary_id, &primary_path_str, "ready")
        .unwrap();

    let result = ManageWorkspaceTool {
        operation: "refresh".to_string(),
        path: None,
        force: Some(true),
        name: None,
        workspace_id: Some(primary_id),
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect("primary force refresh should succeed");

    let text = extract_text_from_result(&result);
    assert!(
        text.contains("Workspace indexing complete"),
        "primary force refresh should delegate to full index path: {text}"
    );
    assert!(
        !text.contains("Workspace Refresh:"),
        "primary force refresh should not use refresh payload: {text}"
    );
}

#[tokio::test]
async fn test_manage_workspace_refresh_force_uses_rebound_session_primary_root() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let original_primary_root = make_isolated_workspace_root(temp_dir.path(), "original-primary");
    let rebound_primary_root = make_isolated_workspace_root(temp_dir.path(), "rebound-primary");
    fs::write(
        original_primary_root.join("main.rs"),
        "fn original_primary_marker() {}\n",
    )
    .unwrap();
    fs::write(
        rebound_primary_root.join("lib.rs"),
        "fn rebound_primary_marker() {}\n",
    )
    .unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());

    let original_primary_path = original_primary_root.canonicalize().unwrap();
    let original_primary_path_str = original_primary_path.to_string_lossy().to_string();
    let original_primary_id = generate_workspace_id(&original_primary_path_str).unwrap();
    let original_primary_ws = Arc::new(
        crate::workspace::JulieWorkspace::initialize(original_primary_path.clone())
            .await
            .expect("original primary workspace should initialize"),
    );

    let handler = JulieServerHandler::new_with_shared_workspace(
        original_primary_ws,
        original_primary_path,
        Some(Arc::clone(&daemon_db)),
        Some(original_primary_id.clone()),
        None,
        None,
        None,
    )
    .await
    .expect("handler should initialize");

    let rebound_primary_path = rebound_primary_root.canonicalize().unwrap();
    let rebound_primary_path_str = rebound_primary_path.to_string_lossy().to_string();
    let rebound_primary_id = generate_workspace_id(&rebound_primary_path_str).unwrap();
    daemon_db
        .upsert_workspace(&original_primary_id, &original_primary_path_str, "ready")
        .unwrap();
    daemon_db
        .upsert_workspace(&rebound_primary_id, &rebound_primary_path_str, "ready")
        .unwrap();

    handler.set_current_primary_binding(rebound_primary_id.clone(), rebound_primary_path);

    #[cfg(not(windows))]
    {
        let renamed_original_root = temp_dir.path().join("original-primary-renamed");
        fs::rename(&original_primary_root, &renamed_original_root).unwrap();
    }

    let result = ManageWorkspaceTool {
        operation: "refresh".to_string(),
        path: None,
        force: Some(true),
        name: None,
        workspace_id: Some(rebound_primary_id.clone()),
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect("rebound primary force refresh should succeed");

    let text = extract_text_from_result(&result);
    assert!(
        text.contains("Workspace indexing complete"),
        "force refresh should index rebound session primary root: {text}"
    );
    assert!(
        !text.contains("Workspace indexing failed"),
        "force refresh should not fall back to stale handler.workspace_root: {text}"
    );

    let rebound_snapshot = daemon_db
        .get_latest_snapshot(&rebound_primary_id)
        .expect("rebound primary snapshot lookup should succeed");
    assert!(
        rebound_snapshot.is_some(),
        "force refresh should attribute codehealth snapshot to rebound session primary"
    );

    let original_snapshot = daemon_db
        .get_latest_snapshot(&original_primary_id)
        .expect("original primary snapshot lookup should succeed");
    assert!(
        original_snapshot.is_none(),
        "stale handler.workspace_id should not receive the rebound primary snapshot"
    );
}

#[tokio::test]
async fn test_daemon_rebound_primary_storage_anchor_keeps_shared_index_root() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let startup_root = make_isolated_workspace_root(temp_dir.path(), "startup-primary");
    let rebound_root = make_isolated_workspace_root(temp_dir.path(), "roots-primary");

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());

    let startup_path = startup_root.canonicalize().unwrap();
    let startup_id = generate_workspace_id(&startup_path.to_string_lossy()).unwrap();
    let startup_ws = Arc::new(
        crate::workspace::JulieWorkspace::initialize(startup_path.clone())
            .await
            .expect("startup workspace should initialize"),
    );

    let mut handler = JulieServerHandler::new_with_shared_workspace(
        startup_ws,
        startup_path,
        Some(Arc::clone(&daemon_db)),
        Some(startup_id.clone()),
        None,
        None,
        None,
    )
    .await
    .expect("handler should initialize");
    // The in-process leader replaces the deleted WorkspacePool as the shared-root
    // source: pin it so workspace_index_dir_for resolves every workspace
    // (including a rebound primary) as a sibling under `indexes_dir`.
    handler.in_process_index_root = Some(indexes_dir.join(&startup_id));

    let rebound_path = rebound_root.canonicalize().unwrap();
    let rebound_id = generate_workspace_id(&rebound_path.to_string_lossy()).unwrap();
    handler.set_current_primary_binding(rebound_id.clone(), rebound_path.clone());

    let db_path = handler
        .workspace_db_file_path_for(&rebound_id)
        .await
        .expect("rebound current primary should still resolve a daemon DB path");

    assert_eq!(
        db_path,
        indexes_dir.join(&rebound_id).join("db").join("symbols.db"),
        "daemon rebound primary should keep the shared daemon index root instead of falling back to the rebound workspace's local .julie store"
    );
}

#[tokio::test]
async fn test_roots_list_error_falls_back_to_startup_hint() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let startup_root = make_isolated_workspace_root(temp_dir.path(), "startup-primary");
    fs::create_dir_all(startup_root.join("src")).unwrap();
    fs::write(
        startup_root.join("src/lib.rs"),
        "pub fn startup_hint_fallback_marker() {}\n",
    )
    .unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());

    let startup_path = startup_root.canonicalize().unwrap();
    let startup_path_str = startup_path.to_string_lossy().to_string();
    let startup_id = generate_workspace_id(&startup_path_str).unwrap();
    let startup_ws = Arc::new(
        crate::workspace::JulieWorkspace::initialize(startup_path.clone())
            .await
            .expect("startup workspace should initialize"),
    );

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
    )
    .await
    .expect("handler should initialize");

    handler.set_client_supports_workspace_roots_for_test(true);
    assert_eq!(
        handler.current_workspace_id(),
        None,
        "cwd startup hint should start without a bound current primary"
    );

    let (server_transport, client_transport) = tokio::io::duplex(64);
    drop(client_transport);
    let service =
        serve_directly::<rmcp::RoleServer, _, _, _, _>(handler.clone(), server_transport, None);

    let result = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        CallToolRequestParams::new("manage_workspace").with_arguments(
            serde_json::json!({ "operation": "list" })
                .as_object()
                .expect("manage_workspace list args")
                .clone(),
        ),
        RequestContext::new(NumberOrString::Number(1), service.peer().clone()),
    )
    .await
    .expect("roots/list failure should fall back to the startup hint");

    let text = extract_text_from_result(&result);
    assert!(
        text.contains(&startup_id),
        "manage_workspace list should still succeed after fallback: {text}"
    );
    assert_eq!(
        handler.current_workspace_id().as_deref(),
        Some(startup_id.as_str()),
        "roots/list failure should bind the startup hint as the current primary"
    );
    assert_eq!(
        handler.current_workspace_root(),
        startup_path,
        "roots/list failure should fall back to the startup hint root"
    );
}

#[tokio::test]
async fn test_manage_workspace_refresh_force_reference_keeps_reference_snapshot_id() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let loaded_primary_root = make_isolated_workspace_root(temp_dir.path(), "loaded-primary");
    let rebound_primary_root = make_isolated_workspace_root(temp_dir.path(), "rebound-primary");
    let reference_root = make_isolated_workspace_root(temp_dir.path(), "reference");
    fs::write(
        loaded_primary_root.join("main.rs"),
        "fn loaded_primary() {}\n",
    )
    .unwrap();
    fs::write(
        rebound_primary_root.join("lib.rs"),
        "fn rebound_primary() {}\n",
    )
    .unwrap();
    fs::write(
        reference_root.join("ref.rs"),
        "fn reference_workspace() {}\n",
    )
    .unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());

    let loaded_primary_path = loaded_primary_root.canonicalize().unwrap();
    let loaded_primary_path_str = loaded_primary_path.to_string_lossy().to_string();
    let loaded_primary_id = generate_workspace_id(&loaded_primary_path_str).unwrap();
    let loaded_primary_ws = Arc::new(
        crate::workspace::JulieWorkspace::initialize(loaded_primary_path.clone())
            .await
            .expect("loaded primary workspace should initialize"),
    );

    let handler = JulieServerHandler::new_with_shared_workspace(
        loaded_primary_ws,
        loaded_primary_path,
        Some(Arc::clone(&daemon_db)),
        Some(loaded_primary_id.clone()),
        None,
        None,
        None,
    )
    .await
    .expect("handler should initialize");

    let rebound_primary_path = rebound_primary_root.canonicalize().unwrap();
    let rebound_primary_path_str = rebound_primary_path.to_string_lossy().to_string();
    let rebound_primary_id = generate_workspace_id(&rebound_primary_path_str).unwrap();
    daemon_db
        .upsert_workspace(&loaded_primary_id, &loaded_primary_path_str, "ready")
        .unwrap();
    daemon_db
        .upsert_workspace(&rebound_primary_id, &rebound_primary_path_str, "ready")
        .unwrap();
    handler.set_current_primary_binding(rebound_primary_id.clone(), rebound_primary_path);

    let reference_path = reference_root.canonicalize().unwrap();
    let reference_path_str = reference_path.to_string_lossy().to_string();
    let reference_id = generate_workspace_id(&reference_path_str).unwrap();
    daemon_db
        .upsert_workspace(&reference_id, &reference_path_str, "ready")
        .unwrap();

    let result = ManageWorkspaceTool {
        operation: "refresh".to_string(),
        path: None,
        force: Some(true),
        name: None,
        workspace_id: Some(reference_id.clone()),
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect("reference force refresh should succeed");

    let text = extract_text_from_result(&result);
    assert!(
        text.contains("Workspace Refresh:") || text.contains("Workspace indexing complete"),
        "reference force refresh should complete: {text}"
    );

    let reference_snapshot = daemon_db
        .get_latest_snapshot(&reference_id)
        .expect("reference snapshot lookup should succeed");
    assert!(
        reference_snapshot.is_some(),
        "reference refresh should snapshot the reference workspace id"
    );

    let rebound_snapshot = daemon_db
        .get_latest_snapshot(&rebound_primary_id)
        .expect("rebound primary snapshot lookup should succeed");
    assert!(
        rebound_snapshot.is_none(),
        "reference refresh should not attribute snapshot to current primary"
    );
}

#[tokio::test]
async fn test_workspace_index_route_for_reference_keeps_reference_storage_under_rebound_primary() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let loaded_primary_root = make_isolated_workspace_root(temp_dir.path(), "loaded-primary");
    let rebound_primary_root = make_isolated_workspace_root(temp_dir.path(), "rebound-primary");
    let reference_root = make_isolated_workspace_root(temp_dir.path(), "reference");
    fs::write(
        loaded_primary_root.join("main.rs"),
        "fn loaded_primary() {}\n",
    )
    .unwrap();
    fs::write(
        rebound_primary_root.join("lib.rs"),
        "fn rebound_primary() {}\n",
    )
    .unwrap();
    fs::write(
        reference_root.join("ref.rs"),
        "fn reference_workspace() {}\n",
    )
    .unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());

    let loaded_primary_path = loaded_primary_root.canonicalize().unwrap();
    let loaded_primary_path_str = loaded_primary_path.to_string_lossy().to_string();
    let loaded_primary_id = generate_workspace_id(&loaded_primary_path_str).unwrap();
    let loaded_primary_ws = Arc::new(
        crate::workspace::JulieWorkspace::initialize(loaded_primary_path.clone())
            .await
            .expect("loaded primary workspace should initialize"),
    );

    let mut handler = JulieServerHandler::new_with_shared_workspace(
        loaded_primary_ws,
        loaded_primary_path,
        Some(Arc::clone(&daemon_db)),
        Some(loaded_primary_id.clone()),
        None,
        None,
        None,
    )
    .await
    .expect("handler should initialize");
    // In-process leader replaces the deleted WorkspacePool as the shared-root
    // source: pin it so the reference (secondary) route resolves under the
    // shared `indexes_dir`, not the rebound primary's project-local `.julie`.
    handler.in_process_index_root = Some(indexes_dir.join(&loaded_primary_id));

    let rebound_primary_path = rebound_primary_root.canonicalize().unwrap();
    let rebound_primary_path_str = rebound_primary_path.to_string_lossy().to_string();
    let rebound_primary_id = generate_workspace_id(&rebound_primary_path_str).unwrap();
    daemon_db
        .upsert_workspace(&loaded_primary_id, &loaded_primary_path_str, "ready")
        .unwrap();
    daemon_db
        .upsert_workspace(&rebound_primary_id, &rebound_primary_path_str, "ready")
        .unwrap();
    handler.set_current_primary_binding(rebound_primary_id, rebound_primary_path.clone());

    let reference_path = reference_root.canonicalize().unwrap();
    let reference_id = generate_workspace_id(&reference_path.to_string_lossy()).unwrap();

    let route = IndexRoute::for_workspace_path(&handler, &reference_path)
        .await
        .expect("reference route should resolve");

    assert!(!route.is_primary);
    assert_eq!(route.workspace_id, reference_id);
    assert_eq!(route.workspace_root, reference_path);
    // The in-process leader pins `in_process_index_root` to the shared
    // `indexes_dir`, so a reference (secondary) workspace keeps shared-root
    // storage even when the primary is rebound (the deleted WorkspacePool used
    // to carry this anchor).
    assert_eq!(
        route.db_path,
        indexes_dir
            .join(&route.workspace_id)
            .join("db")
            .join("symbols.db"),
        "reference workspace DB stays under the shared indexes_dir under a rebound primary"
    );
    assert_eq!(
        route.tantivy_path,
        indexes_dir.join(&route.workspace_id).join("tantivy"),
        "reference workspace tantivy stays under the shared indexes_dir under a rebound primary"
    );
}

#[tokio::test]
async fn test_manage_workspace_index_non_force_uses_rebound_session_primary_root() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let loaded_primary_root = make_isolated_workspace_root(temp_dir.path(), "loaded-primary");
    let rebound_primary_root = make_isolated_workspace_root(temp_dir.path(), "rebound-primary");
    fs::write(
        loaded_primary_root.join("main.rs"),
        "fn loaded_primary() {}\n",
    )
    .unwrap();
    fs::write(
        rebound_primary_root.join("lib.rs"),
        "fn rebound_primary() {}\n",
    )
    .unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());

    let loaded_primary_path = loaded_primary_root.canonicalize().unwrap();
    let loaded_primary_path_str = loaded_primary_path.to_string_lossy().to_string();
    let loaded_primary_id = generate_workspace_id(&loaded_primary_path_str).unwrap();
    let loaded_primary_ws = Arc::new(
        crate::workspace::JulieWorkspace::initialize(loaded_primary_path.clone())
            .await
            .expect("loaded primary workspace should initialize"),
    );

    let handler = JulieServerHandler::new_with_shared_workspace(
        loaded_primary_ws,
        loaded_primary_path.clone(),
        Some(Arc::clone(&daemon_db)),
        Some(loaded_primary_id.clone()),
        None,
        None,
        None,
    )
    .await
    .expect("handler should initialize");

    let rebound_primary_path = rebound_primary_root.canonicalize().unwrap();
    let rebound_primary_path_str = rebound_primary_path.to_string_lossy().to_string();
    let rebound_primary_id = generate_workspace_id(&rebound_primary_path_str).unwrap();
    daemon_db
        .upsert_workspace(&loaded_primary_id, &loaded_primary_path_str, "ready")
        .unwrap();
    daemon_db
        .upsert_workspace(&rebound_primary_id, &rebound_primary_path_str, "ready")
        .unwrap();
    handler.set_current_primary_binding(rebound_primary_id, rebound_primary_path.clone());

    let result = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect("non-force primary index should succeed");

    let text = extract_text_from_result(&result);
    assert!(
        text.contains("Workspace indexing complete") || text.contains("Workspace already indexed"),
        "non-force primary index should complete successfully: {text}"
    );

    let workspace = handler
        .get_workspace()
        .await
        .expect("workspace lookup should succeed")
        .expect("workspace should remain initialized");
    assert_eq!(
        workspace.root.canonicalize().unwrap(),
        rebound_primary_path,
        "non-force primary index should reload the handler workspace to the rebound primary root"
    );
}

#[tokio::test]
async fn test_manage_workspace_index_subdirectory_under_current_root_stays_primary() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let workspace_root = temp_dir.path().join("workspace");
    let subdir = workspace_root.join("src").join("nested");
    fs::create_dir_all(&subdir).unwrap();
    fs::create_dir_all(workspace_root.join(".git")).unwrap();
    fs::write(
        workspace_root.join("src").join("main.rs"),
        "fn root_marker() {}\n",
    )
    .unwrap();
    fs::write(subdir.join("child.rs"), "fn child_marker() {}\n").unwrap();

    let workspace_path = workspace_root.canonicalize().unwrap();
    let subdir_path = subdir.canonicalize().unwrap();

    let handler = JulieServerHandler::new(workspace_path.clone())
        .await
        .expect("handler should initialize");
    handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await
        .expect("workspace should initialize");

    let result = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(subdir_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect("indexing a subdirectory under the current root should succeed");

    let text = extract_text_from_result(&result);
    assert!(
        text.contains("Workspace indexing complete") || text.contains("Workspace already indexed"),
        "subdirectory indexing should still run against the primary workspace: {text}"
    );

    let loaded_workspace = handler
        .get_workspace()
        .await
        .expect("workspace lookup should succeed")
        .expect("workspace should remain loaded");
    assert_eq!(
        loaded_workspace.root.canonicalize().unwrap(),
        workspace_path,
        "subdirectory path inside the current root must not create a bogus child workspace"
    );
    assert_eq!(
        handler.current_workspace_root().canonicalize().unwrap(),
        workspace_path,
        "current primary root should remain the workspace root after subdirectory indexing"
    );
}

/// Regression (`codex/windows-daemon-fixes-v7.13`): an implicit `index`
/// (path=None) run while the handler is rooted at a SUBDIR of a marked project
/// must walk up to the project root via workspace markers — it must not index
/// the subdir as if it were the workspace root.
///
/// The branch regressed this by adding `!explicit_path_requested` to the
/// `use_requested_root_directly` short-circuit in
/// `handle_index_command_internal`: the implicit case then used
/// `current_workspace_root()` (the subdir) verbatim, skipping
/// `resolve_workspace_path` -> `find_workspace_root` marker discovery. A daemon
/// started from `<project>/src/...` would then index the subdir, fragmenting the
/// index and breaking cross-file navigation for the rest of the project.
#[tokio::test]
async fn test_manage_workspace_implicit_index_from_subdir_resolves_to_project_root() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let workspace_root = temp_dir.path().join("workspace");
    let subdir = workspace_root.join("src").join("nested");
    fs::create_dir_all(&subdir).unwrap();
    fs::create_dir_all(workspace_root.join(".git")).unwrap();
    fs::write(
        workspace_root.join("src").join("main.rs"),
        "fn root_marker() {}\n",
    )
    .unwrap();
    fs::write(subdir.join("child.rs"), "fn child_marker() {}\n").unwrap();

    let workspace_path = workspace_root.canonicalize().unwrap();
    let subdir_path = subdir.canonicalize().unwrap();

    // Handler rooted at the SUBDIR with no workspace loaded yet — this mirrors a
    // daemon started from a project subdirectory before its first index. With no
    // loaded workspace, the implicit-index path reads `current_workspace_root()`
    // (the subdir) as the primary-root candidate.
    let handler = JulieServerHandler::new(subdir_path.clone())
        .await
        .expect("handler should initialize");
    assert!(
        handler
            .get_workspace()
            .await
            .expect("workspace lookup should succeed")
            .is_none(),
        "precondition: no workspace loaded, so the implicit-index path uses current_workspace_root()"
    );
    assert_eq!(
        handler.current_workspace_root(),
        subdir_path,
        "precondition: current root must be the subdir so marker discovery has to walk up"
    );

    let result = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect("implicit index from a project subdir should succeed");

    let text = extract_text_from_result(&result);
    assert!(
        text.contains("Workspace indexing complete") || text.contains("Workspace already indexed"),
        "implicit index should complete successfully: {text}"
    );

    let loaded_workspace = handler
        .get_workspace()
        .await
        .expect("workspace lookup should succeed")
        .expect("workspace should be loaded after implicit index");
    assert_eq!(
        loaded_workspace.root.canonicalize().unwrap(),
        workspace_path,
        "implicit index from a subdir must resolve up to the marked project root, not index the subdir as the workspace root"
    );
    assert_eq!(
        handler.current_workspace_root().canonicalize().unwrap(),
        workspace_path,
        "current primary root should be the project root after implicit index from a subdir"
    );
}

#[tokio::test]
async fn test_manage_workspace_refresh_non_force_uses_rebound_session_primary_root() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let loaded_primary_root = make_isolated_workspace_root(temp_dir.path(), "loaded-primary");
    let rebound_primary_root = make_isolated_workspace_root(temp_dir.path(), "rebound-primary");
    fs::write(
        loaded_primary_root.join("main.rs"),
        "fn loaded_primary() {}\n",
    )
    .unwrap();
    fs::write(
        rebound_primary_root.join("lib.rs"),
        "fn rebound_primary() {}\n",
    )
    .unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());

    let loaded_primary_path = loaded_primary_root.canonicalize().unwrap();
    let loaded_primary_path_str = loaded_primary_path.to_string_lossy().to_string();
    let loaded_primary_id = generate_workspace_id(&loaded_primary_path_str).unwrap();
    let loaded_primary_ws = Arc::new(
        crate::workspace::JulieWorkspace::initialize(loaded_primary_path.clone())
            .await
            .expect("loaded primary workspace should initialize"),
    );

    let handler = JulieServerHandler::new_with_shared_workspace(
        loaded_primary_ws,
        loaded_primary_path,
        Some(Arc::clone(&daemon_db)),
        Some(loaded_primary_id.clone()),
        None,
        None,
        None,
    )
    .await
    .expect("handler should initialize");

    let rebound_primary_path = rebound_primary_root.canonicalize().unwrap();
    let rebound_primary_path_str = rebound_primary_path.to_string_lossy().to_string();
    let rebound_primary_id = generate_workspace_id(&rebound_primary_path_str).unwrap();
    daemon_db
        .upsert_workspace(&loaded_primary_id, &loaded_primary_path_str, "ready")
        .unwrap();
    daemon_db
        .upsert_workspace(&rebound_primary_id, &rebound_primary_path_str, "ready")
        .unwrap();
    handler.set_current_primary_binding(rebound_primary_id.clone(), rebound_primary_path.clone());

    let result = ManageWorkspaceTool {
        operation: "refresh".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: Some(rebound_primary_id),
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect("non-force rebound primary refresh should succeed");

    let text = extract_text_from_result(&result);
    assert!(
        text.contains("Workspace indexing complete")
            || text.contains("Workspace already indexed")
            || text.contains("changed files re-indexed")
            || text.contains("Already up-to-date"),
        "non-force rebound primary refresh should use current-primary index path: {text}"
    );

    let workspace = handler
        .get_workspace()
        .await
        .expect("workspace lookup should succeed")
        .expect("workspace should remain initialized");
    assert_eq!(
        workspace.root.canonicalize().unwrap(),
        rebound_primary_path,
        "non-force rebound primary refresh should align the loaded workspace with the session current primary"
    );
}
