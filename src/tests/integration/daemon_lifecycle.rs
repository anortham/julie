//! Integration tests for the daemon + adapter system.
//!
//! Verifies end-to-end daemon lifecycle, workspace pool sharing,
//! workspace header protocol, index migration, and handler tool operation
//! against a shared workspace.

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use std::sync::Arc;
    use std::sync::RwLock as StdRwLock;
    use std::time::Duration;

    use anyhow::Result;

    use crate::daemon::database::DaemonDatabase;
    use crate::daemon::lifecycle::stop_daemon;
    use crate::daemon::transport::TransportEndpoint;
    use crate::daemon::watcher_pool::WatcherPool;
    use crate::daemon::workspace_pool::WorkspacePool;
    use crate::daemon::workspace_registry_store::WorkspaceRegistryStore;
    use crate::daemon::workspace_session_attachment::WorkspaceSessionAttachment;
    use crate::handler::JulieServerHandler;
    use crate::handler::session_workspace::SessionWorkspaceState;
    use crate::migration::run_migration_for_workspace;
    use crate::paths::DaemonPaths;
    use crate::tools::workspace::commands::registry::cleanup::{
        WorkspaceCleanupActivity, run_cleanup_sweep,
    };
    use crate::workspace::startup_hint::WorkspaceStartupHint;

    fn session_attachment(
        pool: Arc<WorkspacePool>,
        daemon_db: Arc<DaemonDatabase>,
        watcher_pool: Option<Arc<WatcherPool>>,
        workspace_root: std::path::PathBuf,
    ) -> WorkspaceSessionAttachment {
        WorkspaceSessionAttachment::new(
            Some(pool),
            Some(daemon_db),
            watcher_pool,
            None,
            Arc::new(StdRwLock::new(SessionWorkspaceState::new(
                WorkspaceStartupHint {
                    path: workspace_root,
                    source: None,
                },
            ))),
        )
    }

    // ---------------------------------------------------------------
    // Test 1: Daemon starts, creates PID, stops cleanly
    // ---------------------------------------------------------------

    /// Poll for a file to appear, up to a deadline.
    async fn wait_for_file(path: &std::path::Path, timeout: std::time::Duration) -> bool {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if path.exists() {
                return true;
            }
            if tokio::time::Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }

    async fn wait_for_daemon_ready(
        paths: &DaemonPaths,
        daemon_handle: &mut tokio::task::JoinHandle<anyhow::Result<()>>,
        timeout: std::time::Duration,
    ) -> anyhow::Result<()> {
        let deadline = tokio::time::Instant::now() + timeout;
        let state_path = paths.daemon_state();
        let transport_path = paths.daemon_mcp_transport();

        loop {
            if daemon_handle.is_finished() {
                match daemon_handle.await {
                    Ok(Ok(())) => anyhow::bail!(
                        "daemon exited before readiness; state_path={}, transport_path={}",
                        state_path.display(),
                        transport_path.display()
                    ),
                    Ok(Err(err)) => anyhow::bail!(
                        "daemon exited before readiness: {err:#}; state_path={}, transport_path={}",
                        state_path.display(),
                        transport_path.display()
                    ),
                    Err(err) => anyhow::bail!(
                        "daemon task ended before readiness: {err}; state_path={}, transport_path={}",
                        state_path.display(),
                        transport_path.display()
                    ),
                }
            }

            let ready_state = std::fs::read_to_string(&state_path)
                .map(|contents| contents.trim() == "ready")
                .unwrap_or(false);

            let ready_transport = TransportEndpoint::read_discovery(&transport_path)
                .map(|endpoint| endpoint.probe_readiness().is_ready())
                .unwrap_or(false);

            if ready_state || ready_transport {
                return Ok(());
            }

            if tokio::time::Instant::now() >= deadline {
                let state = std::fs::read_to_string(&state_path)
                    .map(|contents| contents.trim().to_owned())
                    .unwrap_or_else(|_| "<missing>".to_string());
                anyhow::bail!(
                    "daemon did not become ready within {:?}; state={}, state_path={}, transport_path={}",
                    timeout,
                    state,
                    state_path.display(),
                    transport_path.display()
                );
            }

            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_daemon_starts_creates_pid_then_stops() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = DaemonPaths::with_home(tmp.path().to_path_buf());
        paths.ensure_dirs().expect("ensure_dirs");

        // Spawn the daemon; it blocks on the accept loop until a signal arrives.
        let paths_for_daemon = paths.clone();
        let mut daemon_handle =
            tokio::spawn(async move { crate::daemon::run_daemon(paths_for_daemon, 0, true).await });

        // Poll for the PID file rather than using a fixed sleep. The embedding
        // service init can take several seconds on first run, so a fixed 300ms
        // window is too tight.
        let pid_path = paths.daemon_pid();
        assert!(
            wait_for_file(&pid_path, std::time::Duration::from_secs(30)).await,
            "PID file should appear within 30s at {}",
            pid_path.display()
        );
        // After the v7.7.x format change the file is `<pid> <ctime> <mtime>`,
        // so use the first-field parser instead of treating the whole file as
        // a single integer.
        let pid = crate::daemon::pid::PidFile::read_pid(&pid_path)
            .expect("PID file should be readable");
        assert_eq!(pid, std::process::id(), "PID should match our process");

        // State file plus HTTP readiness is the daemon startup contract.
        wait_for_daemon_ready(
            &paths,
            &mut daemon_handle,
            std::time::Duration::from_secs(30),
        )
        .await
        .expect("daemon should become ready within 30s");

        // In this test the daemon runs as an in-process task, so abort it
        // directly instead of sending SIGTERM to the current test process.
        daemon_handle.abort();
        let _ = daemon_handle.await;

        // `stop_daemon` is for an out-of-process daemon. Drop the in-process
        // PID file first so it takes the stale cleanup path instead of waiting
        // for the current test process to exit.
        let _ = std::fs::remove_file(&pid_path);
        let stop_result = stop_daemon(&paths);
        assert!(
            stop_result.is_ok(),
            "stop_daemon should succeed: {:?}",
            stop_result
        );
        assert!(
            !paths.daemon_state().exists(),
            "Daemon state file should be removed during cleanup"
        );
    }

    // ---------------------------------------------------------------
    // Lazy embedding init: daemon reaches `ready` before slow init completes
    // ---------------------------------------------------------------

    /// The whole point of the lazy-init refactor (Tasks 1-6 of the daemon
    /// lazy embedding init plan): even when `create_embedding_provider`
    /// blocks for a long time (Python sidecar + torch + model load on
    /// production hardware, simulated here via `JULIE_EMBEDDING_TEST_DELAY_MS`),
    /// the daemon must reach `ready` state and publish the HTTP transport
    /// concurrently, not after the embedding init completes.
    ///
    /// This test simulates a 2 second slow init and asserts that the
    /// `daemon.state` file contains `ready` well before that 2 seconds
    /// elapses. If anyone re-blocks the init on the critical path, this
    /// test will fail because `ready` will be written after the 2s sleep
    /// instead of within ~1s of PID file creation.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[serial_test::serial(embedding_env)]
    async fn test_daemon_reaches_ready_before_slow_embedding_init_completes() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = DaemonPaths::with_home(tmp.path().to_path_buf());
        paths.ensure_dirs().expect("ensure_dirs");

        // SAFETY: env vars are not thread-safe in Rust 2024. Serialize this
        // test on the shared embedding_env lock so it cannot race other tests
        // that toggle embedding-related process environment.
        unsafe {
            // Force the embedding service to use a no-provider path so we
            // skip the real Python sidecar machinery.
            std::env::set_var("JULIE_EMBEDDING_PROVIDER", "none");
            // Make `create_embedding_provider` sleep 2 seconds before
            // returning, simulating slow Python sidecar bootstrap.
            std::env::set_var("JULIE_EMBEDDING_TEST_DELAY_MS", "2000");
        }

        let paths_for_daemon = paths.clone();
        let spawn_time = std::time::Instant::now();
        let daemon_handle =
            tokio::spawn(async move { crate::daemon::run_daemon(paths_for_daemon, 0, true).await });

        // Poll for daemon.state == "ready". The new lazy-init path should
        // reach this state in well under the 2 second slow-init sleep,
        // because the embedding init runs in a background task spawned
        // AFTER write_daemon_state("ready").
        let state_path = paths.daemon_state();
        let ready_deadline = std::time::Instant::now() + std::time::Duration::from_millis(1500);
        let mut ready_observed_at = None;
        loop {
            if let Ok(contents) = std::fs::read_to_string(&state_path) {
                if contents.trim() == "ready" {
                    ready_observed_at = Some(std::time::Instant::now());
                    break;
                }
            }
            if std::time::Instant::now() >= ready_deadline {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }

        let ready_at = ready_observed_at.unwrap_or_else(|| {
            // Clean up env before panicking.
            unsafe {
                std::env::remove_var("JULIE_EMBEDDING_PROVIDER");
                std::env::remove_var("JULIE_EMBEDDING_TEST_DELAY_MS");
            }
            daemon_handle.abort();
            panic!(
                "daemon.state did not become 'ready' within 1500ms. \
                 Either the daemon failed to start or someone reverted the \
                 lazy-init change and put create_embedding_provider back on \
                 the critical path. State file: {}",
                state_path.display()
            );
        });

        let ready_elapsed = ready_at.duration_since(spawn_time);
        // Critical assertion: the daemon reached ready BEFORE the simulated
        // 2 second embedding init could possibly have completed. We check
        // <1500ms which gives a wide margin for slow CI hardware while
        // still definitively proving the lazy property.
        assert!(
            ready_elapsed < std::time::Duration::from_millis(1500),
            "daemon reached ready in {:?}, which is too slow — \
             create_embedding_provider should NOT be on the critical path. \
             Expected <1500ms; the embedding init was simulated to take 2000ms.",
            ready_elapsed
        );

        // Clean up env vars before stopping the daemon (so test cleanup
        // doesn't see them set if the abort below races).
        unsafe {
            std::env::remove_var("JULIE_EMBEDDING_PROVIDER");
            std::env::remove_var("JULIE_EMBEDDING_TEST_DELAY_MS");
        }

        // Like the sibling lifecycle test, this daemon runs as an in-process
        // task, so `stop_daemon` must take the stale-cleanup path instead of
        // signaling the current cargo test process.
        let pid_path = paths.daemon_pid();
        daemon_handle.abort();
        let _ = daemon_handle.await;
        let _ = std::fs::remove_file(&pid_path);
        let _ = stop_daemon(&paths);
    }

    // ---------------------------------------------------------------
    // Test 2: WorkspacePool sharing across sessions
    // ---------------------------------------------------------------

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_workspace_pool_sharing_across_sessions() {
        let indexes_tmp = tempfile::tempdir().expect("tempdir for indexes");
        let ws_root = tempfile::tempdir().expect("tempdir for workspace");
        std::fs::create_dir_all(ws_root.path().join(".julie")).expect("create .julie");

        let pool = WorkspacePool::new(indexes_tmp.path().to_path_buf(), None);

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

        let cached = pool.get("shared_ws").await.expect("shared_ws cached");
        assert!(
            Arc::ptr_eq(&ws1, &cached),
            "pool cache should keep the shared workspace instance"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_cleanup_sweep_blocks_missing_workspace_until_sessions_disconnect() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let daemon_db =
            Arc::new(DaemonDatabase::open(&tmp.path().join("daemon.db")).expect("open daemon db"));
        let watcher_pool = Arc::new(WatcherPool::new(Duration::from_secs(300)));
        let indexes_dir = tmp.path().join("indexes");
        std::fs::create_dir_all(&indexes_dir).expect("create indexes dir");

        let pool = Arc::new(WorkspacePool::new(
            indexes_dir.clone(),
            Some(Arc::clone(&daemon_db)),
        ));

        let ws_root = tempfile::tempdir().expect("tempdir for workspace");
        std::fs::create_dir_all(ws_root.path().join(".julie")).expect("create .julie");
        let ws_path = ws_root.path().to_path_buf();
        let ws_id = crate::workspace::registry::generate_workspace_id(&ws_path.to_string_lossy())
            .expect("generate workspace id");

        let first_session = session_attachment(
            Arc::clone(&pool),
            Arc::clone(&daemon_db),
            Some(Arc::clone(&watcher_pool)),
            ws_path.clone(),
        );
        let second_session = session_attachment(
            Arc::clone(&pool),
            Arc::clone(&daemon_db),
            Some(Arc::clone(&watcher_pool)),
            ws_path.clone(),
        );

        first_session
            .attach_workspace_once(&ws_id, ws_path.clone())
            .await
            .expect("first workspace attach should succeed");
        second_session
            .attach_workspace_once(&ws_id, ws_path.clone())
            .await
            .expect("second workspace attach should reuse the pooled workspace");

        std::fs::remove_dir_all(&ws_path).expect("remove workspace path");

        let registry_store = WorkspaceRegistryStore::new(Arc::clone(&daemon_db), indexes_dir);
        let cleanup_activity = WorkspaceCleanupActivity::new(Some(&pool), Some(&watcher_pool));

        let blocked = run_cleanup_sweep(&registry_store, &cleanup_activity)
            .await
            .expect("cleanup sweep should succeed");
        assert!(
            blocked.pruned_workspaces.is_empty(),
            "cleanup should not prune a workspace while sessions are still attached"
        );
        assert!(
            blocked
                .blocked_workspaces
                .iter()
                .any(|(workspace_id, reason)| {
                    workspace_id == &ws_id && reason.contains("active session")
                }),
            "cleanup should explain that active sessions still block pruning: {:?}",
            blocked.blocked_workspaces
        );
        assert!(
            daemon_db
                .get_workspace(&ws_id)
                .expect("lookup workspace row")
                .is_some(),
            "blocked cleanup should keep the workspace row visible"
        );

        first_session
            .detach_workspace_once(&ws_id)
            .await
            .expect("first workspace detach should succeed");
        let still_blocked = run_cleanup_sweep(&registry_store, &cleanup_activity)
            .await
            .expect("cleanup sweep should still succeed after one disconnect");
        assert!(
            still_blocked
                .blocked_workspaces
                .iter()
                .any(|(workspace_id, reason)| {
                    workspace_id == &ws_id && reason.contains("active session")
                }),
            "cleanup should remain blocked until the last attached session disconnects: {:?}",
            still_blocked.blocked_workspaces
        );

        second_session
            .detach_workspace_once(&ws_id)
            .await
            .expect("second workspace detach should succeed");
        let pruned = run_cleanup_sweep(&registry_store, &cleanup_activity)
            .await
            .expect("cleanup sweep should prune the missing workspace once detached");
        assert!(
            pruned
                .pruned_workspaces
                .iter()
                .any(|workspace_id| workspace_id == &ws_id),
            "cleanup should prune the missing workspace after the last session disconnects: {:?}",
            pruned.pruned_workspaces
        );
        assert!(
            daemon_db
                .get_workspace(&ws_id)
                .expect("lookup workspace row after prune")
                .is_none(),
            "pruned workspace row should be removed from daemon db"
        );
    }

    // ---------------------------------------------------------------
    // Test 3: Migration end-to-end
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
            ..Default::default()
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
        let pool = Arc::new(WorkspacePool::new(
            indexes_dir,
            Some(Arc::clone(&daemon_db)),
        ));

        // Step 3: Two sessions attach to the same workspace
        let primary_id = "myproject_deadbeef";
        let first_session = session_attachment(
            Arc::clone(&pool),
            Arc::clone(&daemon_db),
            None,
            ws_root.path().to_path_buf(),
        );
        let second_session = session_attachment(
            Arc::clone(&pool),
            Arc::clone(&daemon_db),
            None,
            ws_root.path().to_path_buf(),
        );
        first_session
            .attach_workspace_once(primary_id, ws_root.path().to_path_buf())
            .await?;
        second_session
            .attach_workspace_once(primary_id, ws_root.path().to_path_buf())
            .await?;
        let ws1 = pool.get(primary_id).await.expect("first pooled workspace");
        let ws2 = pool.get(primary_id).await.expect("second pooled workspace");

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

        // Step 5: Record a cleanup event for a secondary workspace
        let ref_root = tempfile::tempdir()?;
        let ref_id = "mylib_cafebabe";
        daemon_db.upsert_workspace(ref_id, ref_root.path().to_str().unwrap(), "ready")?;
        daemon_db.insert_cleanup_event(
            ref_id,
            ref_root.path().to_str().unwrap(),
            "auto_prune",
            "missing_path",
        )?;
        daemon_db.insert_cleanup_event(
            ref_id,
            ref_root.path().to_str().unwrap(),
            "manual_delete",
            "user_request",
        )?;
        let cleanup_events = daemon_db.list_cleanup_events(10)?;
        assert_eq!(
            cleanup_events
                .iter()
                .filter(|event| event.workspace_id == ref_id)
                .count(),
            2,
            "cleanup events should be queryable for known secondary workspaces"
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

            // Symbol with metadata used by snapshot aggregation
            db.conn.execute(
                "INSERT INTO symbols
                 (id, name, kind, file_path, start_line, end_line, start_col, end_col, language, metadata)
                 VALUES ('s1', 'risky_op', 'Function', 'src/lib.rs', 1, 10, 0, 0, 'rust',
                 '{\"change_risk\":{\"label\":\"HIGH\",\"score\":0.92}}')",
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
        first_session.detach_workspace_once(primary_id).await?;
        second_session.detach_workspace_once(primary_id).await?;

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
