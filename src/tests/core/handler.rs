// Tests for src/handler.rs — JulieServerHandler construction and lifecycle.

use crate::dashboard::state::DashboardEvent;
use crate::database::types::FileInfo;
use crate::handler::{JulieServerHandler, metrics_db_path_for_workspace};
use crate::tools::metrics::session::ToolCallReport;
use anyhow::Result;
use rmcp::ServerHandler;
use rmcp::model::{CallToolRequestParams, NumberOrString, PaginatedRequestParams};
use rmcp::service::{RequestContext, serve_directly};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tempfile::TempDir;
use tokio::sync::broadcast;

fn json_object(value: Value) -> rmcp::model::JsonObject {
    value
        .as_object()
        .expect("test arguments should be a JSON object")
        .clone()
}

fn collect_schema_enum_strings(root: &Value, schema: &Value, values: &mut Vec<String>) {
    if let Some(enum_values) = schema.get("enum").and_then(Value::as_array) {
        values.extend(
            enum_values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string),
        );
    }

    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        if let Some(pointer) = reference.strip_prefix('#') {
            if let Some(target) = root.pointer(pointer) {
                collect_schema_enum_strings(root, target, values);
            }
        }
    }

    for key in ["anyOf", "oneOf", "allOf"] {
        if let Some(items) = schema.get(key).and_then(Value::as_array) {
            for item in items {
                collect_schema_enum_strings(root, item, values);
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn handler_construction_sets_workspace_root() -> Result<()> {
    let handler = JulieServerHandler::new_for_test().await?;
    let handler_root = handler
        .current_workspace_root()
        .canonicalize()
        .unwrap_or_else(|_| handler.current_workspace_root());
    let cwd = std::env::current_dir()?
        .canonicalize()
        .unwrap_or_else(|_| std::env::current_dir().unwrap());
    let temp_root = std::env::temp_dir()
        .canonicalize()
        .unwrap_or_else(|_| std::env::temp_dir());

    assert!(
        handler_root.starts_with(&temp_root),
        "new_for_test should use isolated temp storage, got {}",
        handler_root.display()
    );
    assert_ne!(
        handler_root, cwd,
        "new_for_test should not anchor handlers in the repo cwd"
    );
    assert_eq!(handler.current_workspace_root(), handler.workspace_root);
    assert_eq!(handler.current_workspace_id(), None);
    // workspace should start as None (lazy init)
    let ws = handler.workspace.read().await;
    assert!(
        ws.is_none(),
        "workspace should be None before initialization"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_tool_list_matches_public_surface() -> Result<()> {
    let handler = JulieServerHandler::new_for_test().await?;

    let (server_transport, client_transport) = tokio::io::duplex(64);
    drop(client_transport);
    let service =
        serve_directly::<rmcp::RoleServer, _, _, _, _>(handler.clone(), server_transport, None);

    let tools = <JulieServerHandler as ServerHandler>::list_tools(
        &handler,
        Some(PaginatedRequestParams::default()),
        RequestContext::new(NumberOrString::Number(1), service.peer().clone()),
    )
    .await?;

    assert!(
        tools
            .tools
            .iter()
            .all(|tool| tool.name.as_ref() != "query_metrics"),
        "query_metrics should not appear in the public tool list"
    );
    assert!(
        tools
            .tools
            .iter()
            .all(|tool| tool.name.as_ref() != "edit_symbol"),
        "edit_symbol should not appear in the public tool list"
    );
    assert!(
        tools
            .tools
            .iter()
            .any(|tool| tool.name.as_ref() == "rewrite_symbol"),
        "rewrite_symbol should appear in the public tool list"
    );
    assert!(
        tools
            .tools
            .iter()
            .any(|tool| tool.name.as_ref() == "call_path"),
        "call_path should appear in the public tool list"
    );
    assert!(
        tools
            .tools
            .iter()
            .any(|tool| tool.name.as_ref() == "blast_radius"),
        "blast_radius should appear in the public tool list"
    );
    assert!(
        tools
            .tools
            .iter()
            .any(|tool| tool.name.as_ref() == "spillover_get"),
        "spillover_get should appear in the public tool list"
    );

    let _ = service.cancel().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_fast_search_public_docs_describe_file_mode() -> Result<()> {
    let handler = JulieServerHandler::new_for_test().await?;

    let (server_transport, client_transport) = tokio::io::duplex(64);
    drop(client_transport);
    let service =
        serve_directly::<rmcp::RoleServer, _, _, _, _>(handler.clone(), server_transport, None);

    let tools = <JulieServerHandler as ServerHandler>::list_tools(
        &handler,
        Some(PaginatedRequestParams::default()),
        RequestContext::new(NumberOrString::Number(1), service.peer().clone()),
    )
    .await?;

    let fast_search = tools
        .tools
        .iter()
        .find(|tool| tool.name.as_ref() == "fast_search")
        .expect("fast_search should appear in the public tool list");

    let description = fast_search
        .description
        .as_deref()
        .expect("fast_search should publish a tool description");
    assert!(
        description.contains("search_target=\"files\""),
        "tool description should mention file search mode, got: {description}"
    );

    let properties = fast_search
        .input_schema
        .get("properties")
        .and_then(Value::as_object)
        .expect("fast_search input schema should expose properties");

    let search_target_description = properties
        .get("search_target")
        .and_then(Value::as_object)
        .and_then(|field| field.get("description"))
        .and_then(Value::as_str)
        .expect("search_target should publish a description");
    assert!(
        search_target_description.contains("\"files\""),
        "search_target docs should mention the files mode, got: {search_target_description}"
    );
    assert!(
        search_target_description.contains("\"paths\""),
        "search_target docs should mention the paths alias, got: {search_target_description}"
    );

    let context_lines_description = properties
        .get("context_lines")
        .and_then(Value::as_object)
        .and_then(|field| field.get("description"))
        .and_then(Value::as_str)
        .expect("context_lines should publish a description");
    assert!(
        context_lines_description.contains("search_target=\"files\""),
        "context_lines docs should explain the files-mode restriction, got: {context_lines_description}"
    );

    let return_format_description = properties
        .get("return_format")
        .and_then(Value::as_object)
        .and_then(|field| field.get("description"))
        .and_then(Value::as_str)
        .expect("return_format should publish a description");
    assert!(
        return_format_description.contains("path-only"),
        "return_format docs should explain file-mode locations output, got: {return_format_description}"
    );

    let _ = service.cancel().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_edit_file_public_surface_marks_apply_destructive_and_occurrence_finite() -> Result<()>
{
    let handler = JulieServerHandler::new_for_test().await?;

    let (server_transport, client_transport) = tokio::io::duplex(64);
    drop(client_transport);
    let service =
        serve_directly::<rmcp::RoleServer, _, _, _, _>(handler.clone(), server_transport, None);

    let tools = <JulieServerHandler as ServerHandler>::list_tools(
        &handler,
        Some(PaginatedRequestParams::default()),
        RequestContext::new(NumberOrString::Number(1), service.peer().clone()),
    )
    .await?;

    let edit_file = tools
        .tools
        .iter()
        .find(|tool| tool.name.as_ref() == "edit_file")
        .expect("edit_file should appear in the public tool list");
    let annotations = edit_file
        .annotations
        .as_ref()
        .expect("edit_file should publish annotations");
    assert_eq!(
        annotations.destructive_hint,
        Some(true),
        "edit_file can write to disk when dry_run=false"
    );

    let root_schema = Value::Object((*edit_file.input_schema).clone());
    let properties = root_schema
        .get("properties")
        .and_then(Value::as_object)
        .expect("edit_file input schema should expose properties");
    let occurrence_schema = properties
        .get("occurrence")
        .expect("edit_file schema should include occurrence");
    let mut values = Vec::new();
    collect_schema_enum_strings(&root_schema, occurrence_schema, &mut values);
    values.sort();
    values.dedup();
    assert_eq!(
        values,
        vec!["all".to_string(), "first".to_string(), "last".to_string()],
        "occurrence should be a finite enum in the published schema"
    );

    let _ = service.cancel().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_manage_workspace_public_surface_is_marked_destructive() -> Result<()> {
    let handler = JulieServerHandler::new_for_test().await?;

    let (server_transport, client_transport) = tokio::io::duplex(64);
    drop(client_transport);
    let service =
        serve_directly::<rmcp::RoleServer, _, _, _, _>(handler.clone(), server_transport, None);

    let tools = <JulieServerHandler as ServerHandler>::list_tools(
        &handler,
        Some(PaginatedRequestParams::default()),
        RequestContext::new(NumberOrString::Number(1), service.peer().clone()),
    )
    .await?;

    let manage_workspace = tools
        .tools
        .iter()
        .find(|tool| tool.name.as_ref() == "manage_workspace")
        .expect("manage_workspace should appear in the public tool list");
    let annotations = manage_workspace
        .annotations
        .as_ref()
        .expect("manage_workspace should publish annotations");
    assert_eq!(
        annotations.destructive_hint,
        Some(true),
        "manage_workspace exposes remove, clean, and force reindex operations"
    );

    let _ = service.cancel().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_edit_file_metrics_attribute_root_file_source_bytes() -> Result<()> {
    use crate::tools::workspace::ManageWorkspaceTool;
    use std::time::Duration;

    let temp_dir = TempDir::new()?;
    let cargo_toml = temp_dir.path().join("Cargo.toml");
    let original = "[package]\nname = \"before\"\nversion = \"0.1.0\"\n";
    std::fs::write(&cargo_toml, original)?;

    let handler = JulieServerHandler::new(temp_dir.path().to_path_buf()).await?;
    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        workspace_id: None,
        path: Some(temp_dir.path().to_string_lossy().to_string()),
        name: None,
        force: Some(false),
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;

    let (server_transport, client_transport) = tokio::io::duplex(64);
    drop(client_transport);
    let service =
        serve_directly::<rmcp::RoleServer, _, _, _, _>(handler.clone(), server_transport, None);

    let request =
        CallToolRequestParams::new("edit_file").with_arguments(json_object(serde_json::json!({
            "file_path": "Cargo.toml",
            "old_text": "name = \"before\"",
            "new_text": "name = \"after\"",
            "dry_run": false
        })));
    let result = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        request,
        RequestContext::new(NumberOrString::Number(2), service.peer().clone()),
    )
    .await?;
    assert!(
        !result.content.is_empty(),
        "edit_file should return a tool response"
    );

    let db_arc = {
        let workspace = handler.workspace.read().await;
        workspace
            .as_ref()
            .and_then(|workspace| workspace.db.as_ref())
            .expect("indexed workspace should have a database")
            .clone()
    };
    let source_bytes = tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if let Some(summary) = {
                let db = db_arc.lock().expect("workspace db should lock");
                db.query_session_summary(&handler.session_metrics.session_id)?
                    .into_iter()
                    .find(|summary| summary.tool_name == "edit_file")
            } {
                break Ok::<u64, anyhow::Error>(summary.total_source_bytes);
            }
            tokio::task::yield_now().await;
        }
    })
    .await??;

    assert!(
        source_bytes > 0,
        "edit_file metrics should attribute source bytes for root-level files"
    );

    let _ = service.cancel().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn checkpoint_active_workspace_wal_returns_none_before_initialization() -> Result<()> {
    let handler = JulieServerHandler::new_for_test().await?;

    let checkpoint = crate::startup::checkpoint_active_workspace_wal(&handler).await?;

    assert!(
        checkpoint.is_none(),
        "no workspace should mean no checkpoint"
    );
    Ok(())
}

/// D-H2: Two concurrent on_initialized calls on a shared is_indexed must not
/// both claim the indexing slot. The write-lock check-and-set pattern is atomic.
#[tokio::test(flavor = "multi_thread")]
async fn test_auto_index_write_lock_prevents_double_spawn() {
    let is_indexed = Arc::new(tokio::sync::RwLock::new(false));
    let spawn_count = Arc::new(AtomicUsize::new(0));

    let check_and_maybe_spawn = |flag: Arc<tokio::sync::RwLock<bool>>,
                                 counter: Arc<AtomicUsize>| async move {
        // This is the fixed on_initialized pattern: write-lock + check-and-set.
        let mut guard = flag.write().await;
        if *guard {
            return;
        }
        *guard = true;
        drop(guard);
        counter.fetch_add(1, Ordering::SeqCst);
    };

    tokio::join!(
        check_and_maybe_spawn(Arc::clone(&is_indexed), Arc::clone(&spawn_count)),
        check_and_maybe_spawn(Arc::clone(&is_indexed), Arc::clone(&spawn_count)),
    );

    assert_eq!(
        spawn_count.load(Ordering::SeqCst),
        1,
        "Only one concurrent caller should claim the indexing slot"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn checkpoint_active_workspace_wal_runs_after_workspace_initialization() -> Result<()> {
    let workspace = TempDir::new()?;
    let handler = JulieServerHandler::new(workspace.path().to_path_buf()).await?;
    handler.initialize_workspace(None).await?;

    let checkpoint = crate::startup::checkpoint_active_workspace_wal(&handler).await?;

    assert!(
        checkpoint.is_some(),
        "initialized workspace should expose a database for checkpointing"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn checkpoint_active_workspace_wal_uses_rebound_current_primary_store() -> Result<()> {
    let first_workspace = TempDir::new()?;
    let rebound_workspace = TempDir::new()?;

    let handler = JulieServerHandler::new(first_workspace.path().to_path_buf()).await?;
    handler.initialize_workspace(None).await?;

    let rebound_root = rebound_workspace.path().canonicalize()?;
    let rebound_id =
        crate::workspace::registry::generate_workspace_id(&rebound_root.to_string_lossy())?;
    handler.set_current_primary_binding(rebound_id.clone(), rebound_root);

    let rebound_db_path = handler.workspace_db_file_path_for(&rebound_id).await?;
    std::fs::create_dir_all(rebound_db_path.parent().expect("rebound db parent"))?;
    let _ = crate::database::SymbolDatabase::new(&rebound_db_path)?;

    let rebound_db = handler.get_database_for_workspace(&rebound_id).await?;
    let _rebound_guard = rebound_db.lock().unwrap();

    let checkpoint_err = crate::startup::checkpoint_active_workspace_wal(&handler)
        .await
        .expect_err(
            "checkpoint should target the rebound current-primary db and hit the held lock",
        );
    assert!(
        checkpoint_err
            .to_string()
            .contains("Could not acquire database lock for checkpoint"),
        "checkpoint should use rebound current-primary db, not stale loaded db: {checkpoint_err}"
    );

    Ok(())
}

#[test]
fn metrics_db_path_helper_uses_current_workspace_root_for_local_storage() {
    let current_root = PathBuf::from("/tmp/rebound-primary");
    let db_path = metrics_db_path_for_workspace(None, &current_root, "ref_workspace");

    assert_eq!(
        db_path,
        PathBuf::from("/tmp/rebound-primary/.julie/indexes/ref_workspace/db/symbols.db")
    );
}

#[test]
fn workspace_root_uri_helper_parses_local_file_uri() {
    let path =
        JulieServerHandler::workspace_path_from_root_uri_for_test("file:///tmp/workspace-root")
            .expect("file uri should parse");

    assert_eq!(path, PathBuf::from("/tmp/workspace-root"));
}

#[cfg(windows)]
#[test]
fn workspace_root_uri_helper_parses_unc_file_uri() {
    let path =
        JulieServerHandler::workspace_path_from_root_uri_for_test("file://server/share/project")
            .expect("UNC file uri should parse");

    assert_eq!(path, PathBuf::from(r"\\server\share\project"));
}

#[test]
fn metrics_db_path_helper_uses_shared_index_parent_when_override_exists() {
    let current_root = PathBuf::from("/tmp/rebound-primary");
    let override_root = PathBuf::from("/tmp/shared/indexes/primary_ws");
    let db_path =
        metrics_db_path_for_workspace(Some(&override_root), &current_root, "ref_workspace");

    assert_eq!(
        db_path,
        PathBuf::from("/tmp/shared/indexes/ref_workspace/db/symbols.db")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_record_tool_call_uses_binding_snapshot_for_metrics_attribution() -> Result<()> {
    use crate::daemon::database::DaemonDatabase;
    use crate::daemon::workspace_pool::WorkspacePool;
    use crate::workspace::registry::generate_workspace_id;
    use rusqlite::Connection;
    use std::time::Duration;

    let temp_dir = TempDir::new()?;
    let indexes_dir = temp_dir.path().join("indexes");
    std::fs::create_dir_all(&indexes_dir)?;

    let original_root = temp_dir.path().join("original-primary");
    let rebound_root = temp_dir.path().join("rebound-primary");
    std::fs::create_dir_all(original_root.join("src"))?;
    std::fs::create_dir_all(rebound_root.join("src"))?;

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db"))?);
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir.clone(),
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));

    let original_path = original_root.canonicalize()?;
    let original_path_str = original_path.to_string_lossy().to_string();
    let original_id = generate_workspace_id(&original_path_str)?;
    daemon_db.upsert_workspace(&original_id, &original_path_str, "ready")?;
    let original_ws = pool
        .get_or_init(&original_id, original_path.clone())
        .await?;
    let source_file_rel = "src/original.rs".to_string();
    let source_bytes = 321_u64;
    std::fs::write(original_root.join(&source_file_rel), "fn original() {}\n")?;
    {
        let db_arc = original_ws
            .db
            .as_ref()
            .expect("original workspace should have a db");
        let db = db_arc.lock().expect("original workspace db should lock");
        db.store_file_info(&FileInfo {
            path: source_file_rel.clone(),
            language: "rust".to_string(),
            hash: "original-hash".to_string(),
            size: source_bytes as i64,
            last_modified: 1,
            last_indexed: 1,
            symbol_count: 0,
            line_count: 1,
            content: Some("fn original() {}\n".to_string()),
        })?;
    }

    let rebound_path = rebound_root.canonicalize()?;
    let rebound_path_str = rebound_path.to_string_lossy().to_string();
    let rebound_id = generate_workspace_id(&rebound_path_str)?;
    daemon_db.upsert_workspace(&rebound_id, &rebound_path_str, "ready")?;

    let (dashboard_tx, mut dashboard_rx) = broadcast::channel(8);
    let handler = JulieServerHandler::new_with_shared_workspace(
        original_ws,
        original_path.clone(),
        Some(Arc::clone(&daemon_db)),
        Some(original_id.clone()),
        None,
        None,
        Some(dashboard_tx),
        None,
        Some(Arc::clone(&pool)),
    )
    .await?;

    let binding_snapshot = handler.require_primary_workspace_binding()?;
    handler.set_current_primary_binding(rebound_id.clone(), rebound_path);
    handler
        .publish_loaded_workspace_swap_teardown_gap_for_test()
        .await;

    let mut report = ToolCallReport::empty();
    report.source_file_paths = vec![source_file_rel.clone()];
    handler.record_tool_call(
        "fast_search",
        Duration::from_millis(5),
        &report,
        Some(&binding_snapshot),
    );

    match dashboard_rx.recv().await? {
        DashboardEvent::ToolCall { workspace, .. } => {
            assert_eq!(
                workspace, original_id,
                "dashboard event should use call-start workspace"
            );
        }
        other => panic!("unexpected dashboard event: {other:?}"),
    }

    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let daemon_count: i64 = {
                let conn = daemon_db.conn_for_test();
                conn.query_row("SELECT COUNT(*) FROM tool_calls", [], |row| row.get(0))?
            };
            let local_count: i64 = {
                let conn =
                    Connection::open(indexes_dir.join(&original_id).join("db").join("symbols.db"))?;
                conn.query_row("SELECT COUNT(*) FROM tool_calls", [], |row| row.get(0))?
            };
            if daemon_count > 0 && local_count > 0 {
                break Ok::<(), rusqlite::Error>(());
            }
            tokio::task::yield_now().await;
        }
    })
    .await??;

    let recorded_workspace: String = {
        let conn = daemon_db.conn_for_test();
        conn.query_row(
            "SELECT workspace_id FROM tool_calls ORDER BY id DESC LIMIT 1",
            [],
            |row| row.get(0),
        )?
    };
    assert_eq!(
        recorded_workspace, original_id,
        "daemon metrics row should use call-start workspace"
    );

    let recorded_daemon_source_bytes: Option<i64> = {
        let conn = daemon_db.conn_for_test();
        conn.query_row(
            "SELECT source_bytes FROM tool_calls ORDER BY id DESC LIMIT 1",
            [],
            |row| row.get(0),
        )?
    };
    assert_eq!(
        recorded_daemon_source_bytes,
        Some(source_bytes as i64),
        "daemon metrics row should preserve source_bytes from the snapshotted workspace db"
    );

    let recorded_local_source_bytes: Option<i64> = {
        let conn = Connection::open(indexes_dir.join(&original_id).join("db").join("symbols.db"))?;
        conn.query_row(
            "SELECT source_bytes FROM tool_calls ORDER BY id DESC LIMIT 1",
            [],
            |row| row.get(0),
        )?
    };
    assert_eq!(
        recorded_local_source_bytes,
        Some(source_bytes as i64),
        "local workspace metrics row should still write during the teardown gap"
    );
    assert_eq!(
        handler.session_metrics.total_source_bytes(),
        source_bytes,
        "session metrics should include source_bytes resolved from the snapshotted workspace db"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_metrics_workspace_binding_uses_target_workspace_param() -> Result<()> {
    use crate::daemon::database::DaemonDatabase;
    use crate::daemon::workspace_pool::WorkspacePool;
    use crate::workspace::registry::generate_workspace_id;

    let temp_dir = TempDir::new()?;
    let indexes_dir = temp_dir.path().join("indexes");
    std::fs::create_dir_all(&indexes_dir)?;

    let primary_root = temp_dir.path().join("primary");
    let target_root = temp_dir.path().join("target");
    std::fs::create_dir_all(&primary_root)?;
    std::fs::create_dir_all(&target_root)?;

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db"))?);
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));

    let primary_path = primary_root.canonicalize()?;
    let primary_path_str = primary_path.to_string_lossy().to_string();
    let primary_id = generate_workspace_id(&primary_path_str)?;
    daemon_db.upsert_workspace(&primary_id, &primary_path_str, "ready")?;
    let primary_ws = pool.get_or_init(&primary_id, primary_path.clone()).await?;

    let target_path = target_root.canonicalize()?;
    let target_path_str = target_path.to_string_lossy().to_string();
    let target_id = generate_workspace_id(&target_path_str)?;
    daemon_db.upsert_workspace(&target_id, &target_path_str, "ready")?;

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
    .await?;

    let binding = handler
        .metrics_workspace_binding_for_workspace_param(Some(&target_id))
        .await
        .expect("target workspace binding should resolve");

    assert_eq!(binding.workspace_id, target_id);
    assert_eq!(binding.workspace_root, target_path);

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_fast_refs_target_workspace_uses_requested_binding_for_metrics_attribution()
-> Result<()> {
    use crate::daemon::database::DaemonDatabase;
    use crate::daemon::workspace_pool::WorkspacePool;
    use crate::extractors::{Symbol, SymbolKind};
    use crate::workspace::registry::generate_workspace_id;
    use std::time::Duration;

    let temp_dir = TempDir::new()?;
    let indexes_dir = temp_dir.path().join("indexes");
    std::fs::create_dir_all(&indexes_dir)?;

    let primary_root = temp_dir.path().join("primary");
    let target_root = temp_dir.path().join("target");
    std::fs::create_dir_all(primary_root.join("src"))?;
    std::fs::create_dir_all(target_root.join("src"))?;

    let file_path = "src/target.rs";
    let primary_content = "pub fn primary_only() {}\n";
    let target_content = "pub fn target_symbol() {}\n\npub fn target_helper() {}\n";
    std::fs::write(primary_root.join(file_path), primary_content)?;
    std::fs::write(target_root.join(file_path), target_content)?;

    let primary_bytes = primary_content.len() as i64;
    let target_bytes = target_content.len() as i64;

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db"))?);
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir.clone(),
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));

    let primary_path = primary_root.canonicalize()?;
    let primary_path_str = primary_path.to_string_lossy().to_string();
    let primary_id = generate_workspace_id(&primary_path_str)?;
    daemon_db.upsert_workspace(&primary_id, &primary_path_str, "ready")?;
    let primary_ws = pool.get_or_init(&primary_id, primary_path.clone()).await?;
    {
        let primary_db = primary_ws
            .db
            .as_ref()
            .expect("primary workspace should have a database")
            .clone();
        let primary_db = primary_db.lock().unwrap();
        primary_db.store_file_info(&FileInfo {
            path: file_path.to_string(),
            language: "rust".to_string(),
            hash: "primary-hash".to_string(),
            size: primary_bytes,
            last_modified: 1,
            last_indexed: 1,
            symbol_count: 0,
            line_count: 1,
            content: Some(primary_content.to_string()),
        })?;
    }

    let target_path = target_root.canonicalize()?;
    let target_path_str = target_path.to_string_lossy().to_string();
    let target_id = generate_workspace_id(&target_path_str)?;
    daemon_db.upsert_workspace(&target_id, &target_path_str, "ready")?;
    let target_ws = pool.get_or_init(&target_id, target_path.clone()).await?;
    {
        let target_db = target_ws
            .db
            .as_ref()
            .expect("target workspace should have a database")
            .clone();
        let mut target_db = target_db.lock().unwrap();
        let symbol = Symbol {
            id: "target-symbol-id".to_string(),
            name: "target_symbol".to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: file_path.to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 1,
            end_column: 24,
            start_byte: 0,
            end_byte: 24,
            signature: Some("pub fn target_symbol()".to_string()),
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
            content_type: None,
            annotations: Vec::new(),
        };
        target_db.bulk_store_fresh_atomic(
            &[FileInfo {
                path: file_path.to_string(),
                language: "rust".to_string(),
                hash: "target-hash".to_string(),
                size: target_bytes,
                last_modified: 1,
                last_indexed: 1,
                symbol_count: 1,
                line_count: 3,
                content: Some(target_content.to_string()),
            }],
            &[symbol],
            &[],
            &[],
            &[],
            &target_id,
        )?;
    }

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
    .await?;

    let (server_transport, client_transport) = tokio::io::duplex(64);
    drop(client_transport);
    let service =
        serve_directly::<rmcp::RoleServer, _, _, _, _>(handler.clone(), server_transport, None);

    let request =
        CallToolRequestParams::new("fast_refs").with_arguments(json_object(serde_json::json!({
            "symbol": "target_symbol",
            "include_definition": true,
            "limit": 10,
            "workspace": target_id.clone(),
        })));

    let result = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        request,
        RequestContext::new(NumberOrString::Number(3), service.peer().clone()),
    )
    .await?;

    assert!(
        !result.content.is_empty(),
        "fast_refs should return a tool response"
    );

    let recorded = tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let row = {
                let conn = daemon_db.conn_for_test();
                conn.query_row(
                    "SELECT workspace_id, source_bytes FROM tool_calls ORDER BY id DESC LIMIT 1",
                    [],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<i64>>(1)?)),
                )
            };

            match row {
                Ok(values) => break Ok::<_, rusqlite::Error>(values),
                Err(rusqlite::Error::QueryReturnedNoRows) => {
                    tokio::task::yield_now().await;
                }
                Err(err) => break Err(err),
            }
        }
    })
    .await??;

    assert_eq!(
        recorded.0, target_id,
        "fast_refs telemetry should record the requested workspace id"
    );
    assert_eq!(
        recorded.1,
        Some(target_bytes),
        "fast_refs telemetry should attribute source bytes to the requested workspace"
    );
    assert_eq!(
        handler.session_metrics.total_source_bytes(),
        target_bytes as u64,
        "fast_refs should count source bytes from the requested workspace"
    );

    let _ = service.cancel().await;
    Ok(())
}
