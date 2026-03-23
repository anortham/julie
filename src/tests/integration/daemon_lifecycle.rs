//! Integration tests for the daemon + adapter system.
//!
//! Verifies end-to-end daemon lifecycle, workspace pool sharing, IPC
//! workspace header protocol, index migration, and handler tool operation
//! against a shared workspace.

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use std::sync::Arc;

    use anyhow::Result;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use crate::daemon::ipc::{IpcConnector, IpcListener};
    use crate::daemon::lifecycle::stop_daemon;
    use crate::daemon::workspace_pool::WorkspacePool;
    use crate::handler::JulieServerHandler;
    use crate::migration::run_migration_for_workspace;
    use crate::paths::DaemonPaths;

    // ---------------------------------------------------------------
    // Test 1: Daemon starts, creates PID + socket, stops cleanly
    // ---------------------------------------------------------------

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_daemon_starts_creates_pid_and_socket_then_stops() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = DaemonPaths::with_home(tmp.path().to_path_buf());
        paths.ensure_dirs().expect("ensure_dirs");

        // Spawn the daemon; it blocks on the accept loop until a signal arrives.
        let paths_for_daemon = paths.clone();
        let daemon_handle =
            tokio::spawn(async move { crate::daemon::run_daemon(paths_for_daemon, 0).await });

        // Wait briefly for startup (PID file + socket bind).
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        // PID file should exist with a valid PID.
        let pid_path = paths.daemon_pid();
        assert!(pid_path.exists(), "PID file should exist after startup");
        let pid_str = std::fs::read_to_string(&pid_path).expect("read PID file");
        let pid: u32 = pid_str.trim().parse().expect("PID should be numeric");
        assert_eq!(pid, std::process::id(), "PID should match our process");

        // Socket file should exist.
        let socket_path = paths.daemon_socket();
        assert!(
            socket_path.exists(),
            "Socket file should exist after startup"
        );

        // Stop via lifecycle::stop_daemon. This sends SIGTERM to ourselves,
        // which the daemon's shutdown_signal handler catches.
        // In test context, we abort the task instead (SIGTERM to self is tricky).
        daemon_handle.abort();
        let _ = daemon_handle.await;

        // After abort, cleanup may not have run. Manually invoke stop_daemon
        // to exercise the cleanup path (it should handle the not-running case).
        let stop_result = stop_daemon(&paths);
        assert!(
            stop_result.is_ok(),
            "stop_daemon should succeed: {:?}",
            stop_result
        );
    }

    // ---------------------------------------------------------------
    // Test 2: WorkspacePool sharing across sessions
    // ---------------------------------------------------------------

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_workspace_pool_sharing_across_sessions() {
        let indexes_tmp = tempfile::tempdir().expect("tempdir for indexes");
        let ws_root = tempfile::tempdir().expect("tempdir for workspace");
        std::fs::create_dir_all(ws_root.path().join(".julie")).expect("create .julie");

        let pool = WorkspacePool::new(indexes_tmp.path().to_path_buf(), None, None, None);

        // First session: get_or_init
        let ws1 = pool
            .get_or_init("shared_ws", ws_root.path().to_path_buf())
            .await
            .expect("first get_or_init");

        // Second session: get_or_init with the same ID
        let ws2 = pool
            .get_or_init("shared_ws", ws_root.path().to_path_buf())
            .await
            .expect("second get_or_init");

        // Both Arcs should point to the same underlying database.
        let db1 = ws1.db.as_ref().expect("ws1 db");
        let db2 = ws2.db.as_ref().expect("ws2 db");
        assert!(
            Arc::ptr_eq(db1, db2),
            "Both sessions should share the same database Arc"
        );

        // Pool should report exactly 1 active workspace (not 2).
        assert_eq!(
            pool.active_count().await,
            1,
            "Pool should have 1 workspace, not 2"
        );
    }

    // ---------------------------------------------------------------
    // Test 3: IPC workspace header protocol
    // ---------------------------------------------------------------

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_ipc_workspace_header_protocol() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let socket_path = tmp.path().join("test_header.sock");

        // Bind a listener
        let listener = IpcListener::bind(&socket_path)
            .await
            .expect("bind listener");

        // Spawn a "server" that reads the workspace header byte-by-byte
        // (same protocol as the daemon: read until newline, parse WORKSPACE: prefix).
        let server = tokio::spawn(async move {
            let mut stream = listener.accept().await.expect("accept");

            // Read header byte-by-byte until newline
            let mut header_bytes = Vec::new();
            let mut buf = [0u8; 1];
            loop {
                stream.read_exact(&mut buf).await.expect("read byte");
                if buf[0] == b'\n' {
                    break;
                }
                header_bytes.push(buf[0]);
            }

            let header_str = String::from_utf8(header_bytes).expect("valid UTF-8");
            let path = header_str
                .strip_prefix("WORKSPACE:")
                .expect("header should start with WORKSPACE:");

            path.to_string()
        });

        // Connect from "client" and send the workspace header
        let workspace_path = "/tmp/test-project";
        let mut client = IpcConnector::connect(&socket_path).await.expect("connect");
        let header = format!("WORKSPACE:{}\n", workspace_path);
        client
            .write_all(header.as_bytes())
            .await
            .expect("send header");

        // Verify the server parsed the correct path
        let parsed = server.await.expect("server task");
        assert_eq!(
            parsed, workspace_path,
            "Server should parse the workspace path from the header"
        );
    }

    // ---------------------------------------------------------------
    // Test 4: Migration end-to-end
    // ---------------------------------------------------------------

    #[test]
    fn test_migration_end_to_end() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let julie_home = tmp.path().join("julie_home");
        let project_root = tmp.path().join("my_project");

        let daemon_paths = DaemonPaths::with_home(julie_home.clone());
        daemon_paths.ensure_dirs().expect("ensure_dirs");

        // Create a fake per-project index at {project}/.julie/indexes/{id}/
        let ws_id = "my_project_1a2b3c4d";
        let project_index = project_root.join(".julie/indexes").join(ws_id);
        let db_dir = project_index.join("db");
        let tantivy_dir = project_index.join("tantivy");
        std::fs::create_dir_all(&db_dir).expect("create db dir");
        std::fs::create_dir_all(&tantivy_dir).expect("create tantivy dir");
        std::fs::write(db_dir.join("symbols.db"), b"fake-sqlite").expect("write symbols.db");
        std::fs::write(tantivy_dir.join("meta.json"), b"{}").expect("write meta.json");

        // Run migration
        run_migration_for_workspace(&daemon_paths, &project_root, None).expect("migration");

        // Index should now exist in the centralized location
        let central = julie_home.join("indexes").join(ws_id);
        assert!(
            central.join("db/symbols.db").exists(),
            "symbols.db should exist in centralized index"
        );
        assert!(
            central.join("tantivy/meta.json").exists(),
            "meta.json should exist in centralized index"
        );

        // Original per-project index should be deleted
        assert!(
            !project_index.exists(),
            "Per-project index should be deleted after migration"
        );

        // Migration state should be saved
        let state_path = daemon_paths.migration_state();
        assert!(
            state_path.exists(),
            "Migration state file should be persisted"
        );
        let state_contents = std::fs::read_to_string(&state_path).expect("read state");
        assert!(
            state_contents.contains(ws_id),
            "Migration state should record the workspace ID"
        );
    }

    // ---------------------------------------------------------------
    // Test 5: Handler + shared workspace + tools work
    // ---------------------------------------------------------------

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_handler_shared_workspace_tools_work() -> Result<()> {
        let ws_root = tempfile::tempdir()?;

        // Create a simple Rust file to index
        std::fs::create_dir_all(ws_root.path().join("src"))?;
        std::fs::write(
            ws_root.path().join("src/lib.rs"),
            "pub fn hello() { let _ = 42; }\n",
        )?;

        // Create a handler and set up the workspace (mirrors the daemon flow
        // where initialize_workspace_with_force creates the .julie structure
        // and then ManageWorkspaceTool indexes the files).
        let handler = Arc::new(JulieServerHandler::new_for_test().await?);

        let ws_path_str = ws_root.path().to_str().unwrap().to_string();

        handler
            .initialize_workspace_with_force(Some(ws_path_str.clone()), true)
            .await?;

        // Index files via ManageWorkspaceTool (this is what the daemon's
        // auto-indexing calls internally).
        use crate::tools::workspace::ManageWorkspaceTool;
        let index_tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(ws_path_str),
            name: None,
            workspace_id: None,
            force: Some(false),
            detailed: None,
        };
        index_tool.call_tool(&handler).await?;

        // Search for the function we wrote
        use crate::tools::search::FastSearchTool;
        let tool = FastSearchTool {
            query: "hello".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some("primary".to_string()),
            search_target: "definitions".to_string(),
            context_lines: None,
            exclude_tests: None,
        };

        let result = tool.call_tool(&handler).await;
        assert!(
            result.is_ok(),
            "fast_search should succeed: {:?}",
            result.err()
        );

        // Verify the result actually contains our function
        let call_result = result.unwrap();
        let text: String = call_result
            .content
            .iter()
            .filter_map(|c| {
                serde_json::to_value(c)
                    .ok()
                    .and_then(|v| v.get("text").and_then(|t| t.as_str().map(String::from)))
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(
            text.contains("hello"),
            "Search results should contain 'hello' function. Got: {}",
            text
        );

        Ok(())
    }

    // ---------------------------------------------------------------
    // Test 6: Phase 2 full data flow — daemon.db + pool + snapshot + tool calls
    // ---------------------------------------------------------------

    /// Covers the complete Phase 2 shared-workspace flow end-to-end:
    ///
    /// 1. `DaemonDatabase` opens and creates daemon.db schema
    /// 2. `WorkspacePool` is wired to daemon.db
    /// 3. Two sessions attach to the same workspace; pool shares the db Arc
    /// 4. Daemon.db records the workspace and tracks session counts
    /// 5. A reference workspace is added and the relationship is stored
    /// 6. A codehealth snapshot is captured from the workspace's SymbolDatabase
    /// 7. Tool calls are recorded in daemon.db
    /// 8. Sessions disconnect; session count returns to 0
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_phase2_daemon_db_full_flow() -> Result<()> {
        let tmp = tempfile::tempdir()?;
        let daemon_db_path = tmp.path().join("daemon.db");
        let indexes_dir = tmp.path().join("indexes");
        let ws_root = tempfile::tempdir()?;
        std::fs::create_dir_all(ws_root.path().join(".julie"))?;

        // Step 1: Open DaemonDatabase (mirrors daemon startup)
        let daemon_db = Arc::new(crate::daemon::database::DaemonDatabase::open(
            &daemon_db_path,
        )?);
        assert!(
            daemon_db.table_exists("workspaces"),
            "schema should be created"
        );
        assert!(
            daemon_db.table_exists("codehealth_snapshots"),
            "snapshots table should be created"
        );

        // Step 2: Create WorkspacePool with daemon_db
        let pool = WorkspacePool::new(
            indexes_dir,
            Some(Arc::clone(&daemon_db)),
            None, // no watcher pool for this test
            None, // no embedding service for this test
        );

        // Step 3: Two sessions attach to the same workspace
        let primary_id = "myproject_deadbeef";
        let ws1 = pool
            .get_or_init(primary_id, ws_root.path().to_path_buf())
            .await?;
        let ws2 = pool
            .get_or_init(primary_id, ws_root.path().to_path_buf())
            .await?;

        // Both sessions should share the same SymbolDatabase Arc
        assert!(
            Arc::ptr_eq(
                ws1.db.as_ref().expect("ws1.db"),
                ws2.db.as_ref().expect("ws2.db")
            ),
            "both sessions must share the same database Arc"
        );

        // Step 4: Workspace registered in daemon.db with session_count=2
        let ws_row = daemon_db
            .get_workspace(primary_id)?
            .expect("workspace should be registered in daemon.db");
        assert_eq!(
            ws_row.path,
            ws_root.path().to_str().unwrap(),
            "workspace path should match"
        );
        assert_eq!(ws_row.session_count, 2, "two sessions should be counted");

        // Step 5: Add a reference workspace and record the relationship
        let ref_root = tempfile::tempdir()?;
        let ref_id = "mylib_cafebabe";
        daemon_db.upsert_workspace(ref_id, ref_root.path().to_str().unwrap(), "ready")?;
        daemon_db.add_reference(primary_id, ref_id)?;

        // Reference should be retrievable
        let refs = daemon_db.list_references(primary_id)?;
        assert_eq!(refs.len(), 1, "should have one reference workspace");
        assert_eq!(refs[0].workspace_id, ref_id);

        // Duplicate add_reference should be silently ignored
        daemon_db.add_reference(primary_id, ref_id)?;
        assert_eq!(
            daemon_db.list_references(primary_id)?.len(),
            1,
            "duplicate reference should be ignored"
        );

        // Step 6: Capture codehealth snapshot from the workspace's SymbolDatabase
        {
            let db_arc = ws1.db.as_ref().expect("ws1.db");
            let db = db_arc.lock().unwrap();

            // Insert a file entry (foreign key required by symbols table)
            db.conn.execute(
                "INSERT INTO files (path, language, hash, size, last_modified)
                 VALUES ('src/lib.rs', 'rust', 'abc123', 200, 0)",
                [],
            )?;

            // Symbol with HIGH security risk
            db.conn.execute(
                "INSERT INTO symbols
                 (id, name, kind, file_path, start_line, end_line, start_col, end_col, language, metadata)
                 VALUES ('s1', 'risky_op', 'Function', 'src/lib.rs', 1, 10, 0, 0, 'rust',
                 '{\"security_risk\":{\"label\":\"HIGH\",\"score\":0.92}}')",
                [],
            )?;

            // Ordinary symbol with no risk metadata
            db.conn.execute(
                "INSERT INTO symbols
                 (id, name, kind, file_path, start_line, end_line, start_col, end_col, language)
                 VALUES ('s2', 'safe_helper', 'Function', 'src/lib.rs', 12, 20, 0, 0, 'rust')",
                [],
            )?;

            // Call snapshot_codehealth_from_db while holding the lock
            daemon_db.snapshot_codehealth_from_db(primary_id, &*db)?;
        }

        let snapshot = daemon_db
            .get_latest_snapshot(primary_id)?
            .expect("snapshot should have been captured");
        assert_eq!(snapshot.total_symbols, 2, "both symbols should be counted");
        assert_eq!(snapshot.security_high, 1, "one HIGH security symbol");
        assert_eq!(snapshot.security_medium, 0, "no MEDIUM security symbols");
        assert_eq!(snapshot.total_files, 1, "one file indexed");

        // Step 7: Record tool calls in daemon.db
        daemon_db.insert_tool_call(
            primary_id,
            "sess_a",
            "fast_search",
            12.5,
            Some(5),
            None,
            Some(800),
            true,
            None,
        )?;
        daemon_db.insert_tool_call(
            primary_id,
            "sess_a",
            "get_context",
            88.0,
            Some(1),
            None,
            Some(2000),
            true,
            None,
        )?;
        daemon_db.insert_tool_call(
            primary_id,
            "sess_b",
            "fast_search",
            15.0,
            Some(3),
            None,
            Some(600),
            true,
            None,
        )?;

        let history = daemon_db.query_tool_call_history(primary_id, 7)?;
        assert_eq!(
            history.total_calls, 3,
            "three tool calls should be recorded"
        );
        assert!(
            history
                .per_tool
                .iter()
                .any(|t| t.tool_name == "fast_search"),
            "fast_search should appear in per-tool breakdown"
        );

        // Step 8: Sessions disconnect; session count should return to 0
        pool.disconnect_session(primary_id).await;
        pool.disconnect_session(primary_id).await;

        let ws_row = daemon_db
            .get_workspace(primary_id)?
            .expect("workspace should still exist after disconnect");
        assert_eq!(
            ws_row.session_count, 0,
            "session count should return to 0 after both sessions disconnect"
        );

        Ok(())
    }
}
