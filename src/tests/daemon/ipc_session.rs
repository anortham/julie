mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;

    use anyhow::Context;
    use tokio::io::AsyncWriteExt;
    use tokio::net::UnixStream;
    use tokio::time::sleep;

    use crate::daemon::database::DaemonDatabase;
    use crate::daemon::embedding_service::EmbeddingService;
    use crate::daemon::handle_ipc_session;
    use crate::daemon::ipc_session::workspace_ids_to_disconnect;
    use crate::daemon::workspace_pool::WorkspacePool;
    use crate::handler::JulieServerHandler;
    use crate::workspace::startup_hint::{WorkspaceStartupHint, WorkspaceStartupSource};

    fn wait_for_session_count(
        daemon_db: &DaemonDatabase,
        workspace_id: &str,
        expected: i64,
    ) -> impl std::future::Future<Output = ()> {
        async move {
            let mut last = None;
            for _ in 0..100 {
                if let Ok(Some(row)) = daemon_db.get_workspace(workspace_id) {
                    if row.session_count == expected {
                        return;
                    }
                    last = Some(row.session_count);
                }
                sleep(Duration::from_millis(50)).await;
            }

            let last = last.unwrap_or(-1);
            panic!(
                "Timed out waiting for workspace '{workspace_id}' session_count={expected}, last observed={last}"
            );
        }
    }

    #[tokio::test]
    async fn test_new_with_shared_workspace_preserves_startup_hint() {
        let indexes_dir = tempfile::tempdir().expect("temporary index directory");
        let primary_workspace_root = tempfile::tempdir().expect("primary workspace root");

        std::fs::create_dir_all(primary_workspace_root.path().join(".julie"))
            .expect("create primary .julie");

        let primary_path = primary_workspace_root.path().to_path_buf();
        let primary_id =
            crate::workspace::registry::generate_workspace_id(&primary_path.to_string_lossy())
                .expect("generate primary workspace id");

        let daemon_db_path = PathBuf::from(indexes_dir.path()).join("daemon.db");
        let daemon_db = Arc::new(
            DaemonDatabase::open(&daemon_db_path)
                .context("open daemon db")
                .expect("open daemon db"),
        );

        daemon_db
            .upsert_workspace(&primary_id, &primary_path.to_string_lossy(), "ready")
            .expect("insert primary workspace row");

        let embedding_service = Arc::new(EmbeddingService::initializing());
        let pool = Arc::new(WorkspacePool::new(
            indexes_dir.path().to_path_buf(),
            Some(Arc::clone(&daemon_db)),
            None,
            Some(Arc::clone(&embedding_service)),
        ));

        let workspace = pool
            .get_or_init(&primary_id, primary_path.clone())
            .await
            .expect("preload primary workspace");

        let startup_hint = WorkspaceStartupHint {
            path: primary_path.clone(),
            source: None,
        };

        let handler = JulieServerHandler::new_with_shared_workspace_startup_hint(
            workspace,
            startup_hint.clone(),
            Some(Arc::clone(&daemon_db)),
            Some(primary_id),
            Some(Arc::clone(&embedding_service)),
            Some(Arc::new(AtomicBool::new(false))),
            None,
            None,
            Some(Arc::clone(&pool)),
        )
        .await
        .expect("create daemon handler");

        assert_eq!(handler.workspace_startup_hint(), startup_hint);
    }

    #[tokio::test]
    async fn test_handle_ipc_session_cleans_up_secondary_workspaces_on_serve_error() {
        let indexes_dir = tempfile::tempdir().expect("temporary index directory");
        let primary_workspace_root = tempfile::tempdir().expect("primary workspace root");
        let reference_workspace_root = tempfile::tempdir().expect("reference workspace root");

        std::fs::create_dir_all(primary_workspace_root.path().join(".julie"))
            .expect("create primary .julie");
        std::fs::create_dir_all(reference_workspace_root.path().join(".julie"))
            .expect("create reference .julie");

        let primary_path = primary_workspace_root.path().to_path_buf();
        let reference_path = reference_workspace_root.path().to_path_buf();

        let primary_id =
            crate::workspace::registry::generate_workspace_id(&primary_path.to_string_lossy())
                .expect("generate primary workspace id");
        let reference_id =
            crate::workspace::registry::generate_workspace_id(&reference_path.to_string_lossy())
                .expect("generate reference workspace id");

        let daemon_db_path = PathBuf::from(indexes_dir.path()).join("daemon.db");
        let daemon_db = Arc::new(
            DaemonDatabase::open(&daemon_db_path)
                .context("open daemon db")
                .expect("open daemon db"),
        );

        daemon_db
            .upsert_workspace(&primary_id, &primary_path.to_string_lossy(), "ready")
            .expect("insert primary workspace row");
        daemon_db
            .upsert_workspace(&reference_id, &reference_path.to_string_lossy(), "ready")
            .expect("insert reference workspace row");

        let embedding_service = Arc::new(EmbeddingService::initializing());
        let daemon_db_for_pool = Arc::clone(&daemon_db);
        let pool = Arc::new(WorkspacePool::new(
            indexes_dir.path().to_path_buf(),
            Some(daemon_db_for_pool),
            None,
            Some(Arc::clone(&embedding_service)),
        ));

        pool.get_or_init(&primary_id, primary_path.clone())
            .await
            .expect("preload primary workspace");
        pool.get_or_init(&reference_id, reference_path.clone())
            .await
            .expect("preload reference workspace");
        pool.disconnect_session(&primary_id).await;
        pool.disconnect_session(&reference_id).await;
        wait_for_session_count(&daemon_db, &primary_id, 0).await;
        wait_for_session_count(&daemon_db, &reference_id, 0).await;

        for _ in 0..50 {
            let (mut client_stream, server_stream) = UnixStream::pair().expect("stream pair");
            let restart_pending = Arc::new(AtomicBool::new(false));

            let session_future = tokio::spawn({
                let pool = Arc::clone(&pool);
                let daemon_db = Some(Arc::clone(&daemon_db));
                let embedding_service = Arc::clone(&embedding_service);
                let restart_pending = Arc::clone(&restart_pending);
                let startup_hint = WorkspaceStartupHint {
                    path: primary_path.clone(),
                    source: Some(WorkspaceStartupSource::Cli),
                };

                async move {
                    handle_ipc_session(
                        server_stream,
                        pool,
                        "session-handle-ipc",
                        &daemon_db,
                        &embedding_service,
                        &restart_pending,
                        None,
                        startup_hint,
                        None,
                        None,
                    )
                    .await
                }
            });

            client_stream
                .write_all(&[0xff])
                .await
                .expect("send malformed MCP frame");
            client_stream
                .shutdown()
                .await
                .expect("shutdown malformed client stream");

            session_future
                .await
                .expect("handle_ipc_session task completed")
                .expect_err("expected malformed MCP to produce error");

            wait_for_session_count(&daemon_db, &primary_id, 0).await;
            wait_for_session_count(&daemon_db, &reference_id, 0).await;
        }
    }

    #[tokio::test]
    async fn test_handle_ipc_session_weak_cwd_startup_is_not_attached_before_first_bind() {
        let indexes_dir = tempfile::tempdir().expect("temporary index directory");
        let startup_workspace_root = tempfile::tempdir().expect("startup workspace root");

        std::fs::create_dir_all(startup_workspace_root.path().join("src"))
            .expect("create startup src");
        std::fs::write(
            startup_workspace_root.path().join("src/lib.rs"),
            "pub fn startup() {}\n",
        )
        .expect("write startup source");

        let startup_path = startup_workspace_root.path().to_path_buf();
        let startup_id =
            crate::workspace::registry::generate_workspace_id(&startup_path.to_string_lossy())
                .expect("generate startup workspace id");

        let daemon_db_path = PathBuf::from(indexes_dir.path()).join("daemon.db");
        let daemon_db = Arc::new(
            DaemonDatabase::open(&daemon_db_path)
                .context("open daemon db")
                .expect("open daemon db"),
        );

        let embedding_service = Arc::new(EmbeddingService::initializing());
        let pool = Arc::new(WorkspacePool::new(
            indexes_dir.path().to_path_buf(),
            Some(Arc::clone(&daemon_db)),
            None,
            Some(Arc::clone(&embedding_service)),
        ));

        let (client_stream, server_stream) = UnixStream::pair().expect("stream pair");
        let restart_pending = Arc::new(AtomicBool::new(false));

        let session_future = tokio::spawn({
            let pool = Arc::clone(&pool);
            let daemon_db = Some(Arc::clone(&daemon_db));
            let embedding_service = Arc::clone(&embedding_service);
            let restart_pending = Arc::clone(&restart_pending);
            let startup_hint = WorkspaceStartupHint {
                path: startup_path.clone(),
                source: Some(WorkspaceStartupSource::Cwd),
            };

            async move {
                handle_ipc_session(
                    server_stream,
                    pool,
                    "session-weak-cwd-deferred",
                    &daemon_db,
                    &embedding_service,
                    &restart_pending,
                    None,
                    startup_hint,
                    None,
                    None,
                )
                .await
            }
        });

        sleep(Duration::from_millis(100)).await;

        assert!(
            daemon_db
                .get_workspace(&startup_id)
                .expect("query startup workspace row")
                .is_none(),
            "weak cwd startup should not register the startup workspace before first primary bind"
        );
        assert!(
            pool.get(&startup_id).await.is_none(),
            "weak cwd startup should not attach the startup workspace in the pool before first primary bind"
        );
        assert!(
            !indexes_dir.path().join(&startup_id).exists(),
            "weak cwd startup should not create shared index state before first primary bind"
        );

        drop(client_stream);

        let _ = session_future
            .await
            .expect("handle_ipc_session task completed");

        assert!(
            daemon_db
                .get_workspace(&startup_id)
                .expect("query startup workspace row after disconnect")
                .is_none(),
            "cleanup should not synthesize a startup workspace row for an unbound weak cwd session"
        );
    }

    #[tokio::test]
    async fn test_known_workspace_is_not_auto_activated_on_new_session() {
        let indexes_dir = tempfile::tempdir().expect("temporary index directory");
        let primary_workspace_root = tempfile::tempdir().expect("primary workspace root");
        let reference_workspace_root = tempfile::tempdir().expect("reference workspace root");

        std::fs::create_dir_all(primary_workspace_root.path().join(".julie"))
            .expect("create primary .julie");
        std::fs::create_dir_all(reference_workspace_root.path().join(".julie"))
            .expect("create reference .julie");

        let primary_path = primary_workspace_root.path().to_path_buf();
        let reference_path = reference_workspace_root.path().to_path_buf();

        let primary_id =
            crate::workspace::registry::generate_workspace_id(&primary_path.to_string_lossy())
                .expect("generate primary workspace id");
        let reference_id =
            crate::workspace::registry::generate_workspace_id(&reference_path.to_string_lossy())
                .expect("generate reference workspace id");

        let daemon_db_path = PathBuf::from(indexes_dir.path()).join("daemon.db");
        let daemon_db = Arc::new(
            DaemonDatabase::open(&daemon_db_path)
                .context("open daemon db")
                .expect("open daemon db"),
        );

        daemon_db
            .upsert_workspace(&primary_id, &primary_path.to_string_lossy(), "ready")
            .expect("insert primary workspace row");
        daemon_db
            .upsert_workspace(&reference_id, &reference_path.to_string_lossy(), "ready")
            .expect("insert reference workspace row");

        let embedding_service = Arc::new(EmbeddingService::initializing());
        let pool = Arc::new(WorkspacePool::new(
            indexes_dir.path().to_path_buf(),
            Some(Arc::clone(&daemon_db)),
            None,
            Some(Arc::clone(&embedding_service)),
        ));

        pool.get_or_init(&primary_id, primary_path.clone())
            .await
            .expect("preload primary workspace");
        pool.get_or_init(&reference_id, reference_path.clone())
            .await
            .expect("preload reference workspace");
        pool.disconnect_session(&primary_id).await;
        pool.disconnect_session(&reference_id).await;
        wait_for_session_count(&daemon_db, &primary_id, 0).await;
        wait_for_session_count(&daemon_db, &reference_id, 0).await;

        let (client_stream, server_stream) = UnixStream::pair().expect("stream pair");
        let restart_pending = Arc::new(AtomicBool::new(false));

        let session_future = tokio::spawn({
            let pool = Arc::clone(&pool);
            let daemon_db = Some(Arc::clone(&daemon_db));
            let embedding_service = Arc::clone(&embedding_service);
            let restart_pending = Arc::clone(&restart_pending);
            let startup_hint = WorkspaceStartupHint {
                path: primary_path.clone(),
                source: Some(WorkspaceStartupSource::Env),
            };

            async move {
                handle_ipc_session(
                    server_stream,
                    pool,
                    "session-no-auto-attach",
                    &daemon_db,
                    &embedding_service,
                    &restart_pending,
                    None,
                    startup_hint,
                    None,
                    None,
                )
                .await
            }
        });

        wait_for_session_count(&daemon_db, &primary_id, 1).await;
        let reference_row = daemon_db
            .get_workspace(&reference_id)
            .expect("load reference workspace row")
            .expect("reference workspace row should exist");
        assert_eq!(
            reference_row.session_count, 0,
            "known workspace rows must not auto-activate the secondary workspace"
        );

        drop(client_stream);

        let _ = session_future
            .await
            .expect("handle_ipc_session task completed");

        wait_for_session_count(&daemon_db, &primary_id, 0).await;
        wait_for_session_count(&daemon_db, &reference_id, 0).await;
    }

    #[tokio::test]
    async fn test_handle_ipc_session_cleanup_disconnects_startup_and_rebound_primary() {
        let indexes_dir = tempfile::tempdir().expect("temporary index directory");
        let startup_workspace_root = tempfile::tempdir().expect("startup workspace root");
        let rebound_workspace_root = tempfile::tempdir().expect("rebound workspace root");

        std::fs::create_dir_all(startup_workspace_root.path().join(".julie"))
            .expect("create startup .julie");
        std::fs::create_dir_all(rebound_workspace_root.path().join(".julie"))
            .expect("create rebound .julie");

        let startup_path = startup_workspace_root.path().to_path_buf();
        let rebound_path = rebound_workspace_root.path().to_path_buf();
        let startup_id =
            crate::workspace::registry::generate_workspace_id(&startup_path.to_string_lossy())
                .expect("generate startup workspace id");
        let rebound_id =
            crate::workspace::registry::generate_workspace_id(&rebound_path.to_string_lossy())
                .expect("generate rebound workspace id");

        let daemon_db_path = PathBuf::from(indexes_dir.path()).join("daemon.db");
        let daemon_db = Arc::new(
            DaemonDatabase::open(&daemon_db_path)
                .context("open daemon db")
                .expect("open daemon db"),
        );

        daemon_db
            .upsert_workspace(&startup_id, &startup_path.to_string_lossy(), "ready")
            .expect("insert startup workspace row");
        daemon_db
            .upsert_workspace(&rebound_id, &rebound_path.to_string_lossy(), "ready")
            .expect("insert rebound workspace row");

        let embedding_service = Arc::new(EmbeddingService::initializing());
        let pool = Arc::new(WorkspacePool::new(
            indexes_dir.path().to_path_buf(),
            Some(Arc::clone(&daemon_db)),
            None,
            Some(Arc::clone(&embedding_service)),
        ));

        pool.get_or_init(&startup_id, startup_path.clone())
            .await
            .expect("attach startup workspace session");
        pool.get_or_init(&rebound_id, rebound_path.clone())
            .await
            .expect("attach rebound workspace session");
        wait_for_session_count(&daemon_db, &startup_id, 1).await;
        wait_for_session_count(&daemon_db, &rebound_id, 1).await;

        for workspace_id in workspace_ids_to_disconnect(&startup_id, vec![rebound_id.clone()], true)
        {
            pool.disconnect_session(&workspace_id).await;
        }

        wait_for_session_count(&daemon_db, &startup_id, 0).await;
        wait_for_session_count(&daemon_db, &rebound_id, 0).await;
    }

    #[tokio::test]
    async fn test_handle_ipc_session_rebind_keeps_other_pooled_session_workspace_usable() {
        let indexes_dir = tempfile::tempdir().expect("temporary index directory");
        let workspace_a_root = tempfile::tempdir().expect("workspace A root");
        let workspace_b_root = tempfile::tempdir().expect("workspace B root");

        std::fs::create_dir_all(workspace_a_root.path().join(".julie")).expect("create A .julie");
        std::fs::create_dir_all(workspace_b_root.path().join(".julie")).expect("create B .julie");

        let workspace_a_path = workspace_a_root.path().to_path_buf();
        let workspace_b_path = workspace_b_root.path().to_path_buf();
        let workspace_a_id =
            crate::workspace::registry::generate_workspace_id(&workspace_a_path.to_string_lossy())
                .expect("generate workspace A id");
        let workspace_b_id =
            crate::workspace::registry::generate_workspace_id(&workspace_b_path.to_string_lossy())
                .expect("generate workspace B id");

        let daemon_db_path = PathBuf::from(indexes_dir.path()).join("daemon.db");
        let daemon_db = Arc::new(
            DaemonDatabase::open(&daemon_db_path)
                .context("open daemon db")
                .expect("open daemon db"),
        );

        daemon_db
            .upsert_workspace(
                &workspace_a_id,
                &workspace_a_path.to_string_lossy(),
                "ready",
            )
            .expect("insert workspace A row");
        daemon_db
            .upsert_workspace(
                &workspace_b_id,
                &workspace_b_path.to_string_lossy(),
                "ready",
            )
            .expect("insert workspace B row");

        let embedding_service = Arc::new(EmbeddingService::initializing());
        let pool = Arc::new(WorkspacePool::new(
            indexes_dir.path().to_path_buf(),
            Some(Arc::clone(&daemon_db)),
            None,
            Some(Arc::clone(&embedding_service)),
        ));

        let pooled_a_for_handler_one = pool
            .get_or_init(&workspace_a_id, workspace_a_path.clone())
            .await
            .expect("attach workspace A for handler one");
        let pooled_a_for_handler_two = pool
            .get_or_init(&workspace_a_id, workspace_a_path.clone())
            .await
            .expect("attach workspace A for handler two");

        let handler_one = JulieServerHandler::new_with_shared_workspace_startup_hint(
            pooled_a_for_handler_one,
            WorkspaceStartupHint {
                path: workspace_a_path.clone(),
                source: Some(WorkspaceStartupSource::Cli),
            },
            Some(Arc::clone(&daemon_db)),
            Some(workspace_a_id.clone()),
            Some(Arc::clone(&embedding_service)),
            Some(Arc::new(AtomicBool::new(false))),
            None,
            None,
            Some(Arc::clone(&pool)),
        )
        .await
        .expect("create handler one");

        let handler_two = JulieServerHandler::new_with_shared_workspace_startup_hint(
            pooled_a_for_handler_two,
            WorkspaceStartupHint {
                path: workspace_a_path.clone(),
                source: Some(WorkspaceStartupSource::Cli),
            },
            Some(Arc::clone(&daemon_db)),
            Some(workspace_a_id.clone()),
            Some(Arc::clone(&embedding_service)),
            Some(Arc::new(AtomicBool::new(false))),
            None,
            None,
            Some(Arc::clone(&pool)),
        )
        .await
        .expect("create handler two");

        let workspace_a_search_index = handler_two
            .get_workspace()
            .await
            .expect("workspace lookup should succeed")
            .expect("handler two should keep workspace A loaded")
            .search_index
            .as_ref()
            .expect("workspace A search index should exist")
            .clone();

        handler_one.set_current_primary_binding(workspace_b_id.clone(), workspace_b_path.clone());
        handler_one
            .initialize_workspace_with_force(
                Some(workspace_b_path.to_string_lossy().to_string()),
                false,
            )
            .await
            .expect("handler one should rebind to workspace B");

        let workspace_a_search_index = workspace_a_search_index.lock().unwrap();
        assert!(
            !workspace_a_search_index.is_shutdown(),
            "rebind in one daemon session must not shut down pooled workspace A search index while another session still uses it"
        );

        let handler_two_workspace = handler_two
            .get_workspace()
            .await
            .expect("workspace lookup should succeed")
            .expect("handler two should still have workspace A");
        assert_eq!(
            handler_two_workspace.root.canonicalize().unwrap(),
            workspace_a_path.canonicalize().unwrap(),
            "other daemon session should remain attached to workspace A after handler one rebounds"
        );
    }

    #[tokio::test]
    async fn test_handle_ipc_session_helper_calls_fail_when_rebound_daemon_workspace_missing_from_pool()
     {
        let indexes_dir = tempfile::tempdir().expect("temporary index directory");
        let workspace_a_root = tempfile::tempdir().expect("workspace A root");
        let workspace_b_root = tempfile::tempdir().expect("workspace B root");

        std::fs::create_dir_all(workspace_a_root.path().join(".julie")).expect("create A .julie");
        std::fs::create_dir_all(workspace_b_root.path().join(".julie")).expect("create B .julie");

        let workspace_a_path = workspace_a_root.path().to_path_buf();
        let workspace_b_path = workspace_b_root.path().to_path_buf();
        let workspace_a_id =
            crate::workspace::registry::generate_workspace_id(&workspace_a_path.to_string_lossy())
                .expect("generate workspace A id");
        let workspace_b_id =
            crate::workspace::registry::generate_workspace_id(&workspace_b_path.to_string_lossy())
                .expect("generate workspace B id");

        let daemon_db_path = PathBuf::from(indexes_dir.path()).join("daemon.db");
        let daemon_db = Arc::new(
            DaemonDatabase::open(&daemon_db_path)
                .context("open daemon db")
                .expect("open daemon db"),
        );

        daemon_db
            .upsert_workspace(
                &workspace_a_id,
                &workspace_a_path.to_string_lossy(),
                "ready",
            )
            .expect("insert workspace A row");
        daemon_db
            .upsert_workspace(
                &workspace_b_id,
                &workspace_b_path.to_string_lossy(),
                "ready",
            )
            .expect("insert workspace B row");

        let embedding_service = Arc::new(EmbeddingService::initializing());
        let pool = Arc::new(WorkspacePool::new(
            indexes_dir.path().to_path_buf(),
            Some(Arc::clone(&daemon_db)),
            None,
            Some(Arc::clone(&embedding_service)),
        ));

        let pooled_a = pool
            .get_or_init(&workspace_a_id, workspace_a_path.clone())
            .await
            .expect("attach workspace A");

        let handler = JulieServerHandler::new_with_shared_workspace_startup_hint(
            pooled_a,
            WorkspaceStartupHint {
                path: workspace_a_path.clone(),
                source: Some(WorkspaceStartupSource::Cli),
            },
            Some(Arc::clone(&daemon_db)),
            Some(workspace_a_id.clone()),
            Some(Arc::clone(&embedding_service)),
            Some(Arc::new(AtomicBool::new(false))),
            None,
            None,
            Some(Arc::clone(&pool)),
        )
        .await
        .expect("create handler");

        handler.set_current_primary_binding(workspace_b_id.clone(), workspace_b_path.clone());

        let db_err = match handler.get_database_for_workspace(&workspace_b_id).await {
            Ok(_) => panic!(
                "daemon helper should fail loudly when rebound workspace is missing from pool"
            ),
            Err(err) => err,
        };
        assert!(
            db_err
                .to_string()
                .contains("not attached in the daemon workspace pool")
        );

        let si_err = match handler
            .get_search_index_for_workspace(&workspace_b_id)
            .await
        {
            Ok(_) => panic!(
                "search helper should fail loudly when rebound workspace is missing from pool"
            ),
            Err(err) => err,
        };
        assert!(
            si_err
                .to_string()
                .contains("not attached in the daemon workspace pool")
        );
    }

    #[tokio::test]
    async fn test_handle_ipc_session_cleanup_disconnects_multi_hop_rebound_primaries() {
        let indexes_dir = tempfile::tempdir().expect("temporary index directory");
        let workspace_a_root = tempfile::tempdir().expect("workspace A root");
        let workspace_b_root = tempfile::tempdir().expect("workspace B root");
        let workspace_c_root = tempfile::tempdir().expect("workspace C root");

        std::fs::create_dir_all(workspace_a_root.path().join(".julie")).expect("create A .julie");
        std::fs::create_dir_all(workspace_b_root.path().join(".julie")).expect("create B .julie");
        std::fs::create_dir_all(workspace_c_root.path().join(".julie")).expect("create C .julie");

        let workspace_a_path = workspace_a_root.path().to_path_buf();
        let workspace_b_path = workspace_b_root.path().to_path_buf();
        let workspace_c_path = workspace_c_root.path().to_path_buf();
        let workspace_a_id =
            crate::workspace::registry::generate_workspace_id(&workspace_a_path.to_string_lossy())
                .expect("generate workspace A id");
        let workspace_b_id =
            crate::workspace::registry::generate_workspace_id(&workspace_b_path.to_string_lossy())
                .expect("generate workspace B id");
        let workspace_c_id =
            crate::workspace::registry::generate_workspace_id(&workspace_c_path.to_string_lossy())
                .expect("generate workspace C id");

        let daemon_db_path = PathBuf::from(indexes_dir.path()).join("daemon.db");
        let daemon_db = Arc::new(
            DaemonDatabase::open(&daemon_db_path)
                .context("open daemon db")
                .expect("open daemon db"),
        );

        for (id, path) in [
            (&workspace_a_id, &workspace_a_path),
            (&workspace_b_id, &workspace_b_path),
            (&workspace_c_id, &workspace_c_path),
        ] {
            daemon_db
                .upsert_workspace(id, &path.to_string_lossy(), "ready")
                .expect("insert workspace row");
        }

        let embedding_service = Arc::new(EmbeddingService::initializing());
        let pool = Arc::new(WorkspacePool::new(
            indexes_dir.path().to_path_buf(),
            Some(Arc::clone(&daemon_db)),
            None,
            Some(Arc::clone(&embedding_service)),
        ));

        let pooled_a = pool
            .get_or_init(&workspace_a_id, workspace_a_path.clone())
            .await
            .expect("attach workspace A");

        let handler = JulieServerHandler::new_with_shared_workspace_startup_hint(
            pooled_a,
            WorkspaceStartupHint {
                path: workspace_a_path.clone(),
                source: Some(WorkspaceStartupSource::Cli),
            },
            Some(Arc::clone(&daemon_db)),
            Some(workspace_a_id.clone()),
            Some(Arc::clone(&embedding_service)),
            Some(Arc::new(AtomicBool::new(false))),
            None,
            None,
            Some(Arc::clone(&pool)),
        )
        .await
        .expect("create handler");

        handler.set_current_primary_binding(workspace_b_id.clone(), workspace_b_path.clone());
        handler
            .initialize_workspace_with_force(
                Some(workspace_b_path.to_string_lossy().to_string()),
                false,
            )
            .await
            .expect("rebind to B");

        handler.set_current_primary_binding(workspace_c_id.clone(), workspace_c_path.clone());
        handler
            .initialize_workspace_with_force(
                Some(workspace_c_path.to_string_lossy().to_string()),
                false,
            )
            .await
            .expect("rebind to C");

        wait_for_session_count(&daemon_db, &workspace_a_id, 1).await;
        wait_for_session_count(&daemon_db, &workspace_b_id, 1).await;
        wait_for_session_count(&daemon_db, &workspace_c_id, 1).await;

        for workspace_id in workspace_ids_to_disconnect(
            &workspace_a_id,
            handler.session_attached_workspace_ids().await,
            true,
        ) {
            pool.disconnect_session(&workspace_id).await;
        }

        wait_for_session_count(&daemon_db, &workspace_a_id, 0).await;
        wait_for_session_count(&daemon_db, &workspace_b_id, 0).await;
        wait_for_session_count(&daemon_db, &workspace_c_id, 0).await;
    }

    #[tokio::test]
    async fn test_handle_ipc_session_cleanup_disconnects_bounce_back_rebinds_without_leaks() {
        let indexes_dir = tempfile::tempdir().expect("temporary index directory");
        let workspace_a_root = tempfile::tempdir().expect("workspace A root");
        let workspace_b_root = tempfile::tempdir().expect("workspace B root");

        std::fs::create_dir_all(workspace_a_root.path().join(".julie")).expect("create A .julie");
        std::fs::create_dir_all(workspace_b_root.path().join(".julie")).expect("create B .julie");

        let workspace_a_path = workspace_a_root.path().to_path_buf();
        let workspace_b_path = workspace_b_root.path().to_path_buf();
        let workspace_a_id =
            crate::workspace::registry::generate_workspace_id(&workspace_a_path.to_string_lossy())
                .expect("generate workspace A id");
        let workspace_b_id =
            crate::workspace::registry::generate_workspace_id(&workspace_b_path.to_string_lossy())
                .expect("generate workspace B id");

        let daemon_db_path = PathBuf::from(indexes_dir.path()).join("daemon.db");
        let daemon_db = Arc::new(
            DaemonDatabase::open(&daemon_db_path)
                .context("open daemon db")
                .expect("open daemon db"),
        );

        daemon_db
            .upsert_workspace(
                &workspace_a_id,
                &workspace_a_path.to_string_lossy(),
                "ready",
            )
            .expect("insert workspace A row");
        daemon_db
            .upsert_workspace(
                &workspace_b_id,
                &workspace_b_path.to_string_lossy(),
                "ready",
            )
            .expect("insert workspace B row");

        let embedding_service = Arc::new(EmbeddingService::initializing());
        let pool = Arc::new(WorkspacePool::new(
            indexes_dir.path().to_path_buf(),
            Some(Arc::clone(&daemon_db)),
            None,
            Some(Arc::clone(&embedding_service)),
        ));

        let pooled_a = pool
            .get_or_init(&workspace_a_id, workspace_a_path.clone())
            .await
            .expect("attach workspace A");

        let handler = JulieServerHandler::new_with_shared_workspace_startup_hint(
            pooled_a,
            WorkspaceStartupHint {
                path: workspace_a_path.clone(),
                source: Some(WorkspaceStartupSource::Cli),
            },
            Some(Arc::clone(&daemon_db)),
            Some(workspace_a_id.clone()),
            Some(Arc::clone(&embedding_service)),
            Some(Arc::new(AtomicBool::new(false))),
            None,
            None,
            Some(Arc::clone(&pool)),
        )
        .await
        .expect("create handler");

        handler.set_current_primary_binding(workspace_b_id.clone(), workspace_b_path.clone());
        handler
            .initialize_workspace_with_force(
                Some(workspace_b_path.to_string_lossy().to_string()),
                false,
            )
            .await
            .expect("rebind to B");

        handler.set_current_primary_binding(workspace_a_id.clone(), workspace_a_path.clone());
        handler
            .initialize_workspace_with_force(
                Some(workspace_a_path.to_string_lossy().to_string()),
                false,
            )
            .await
            .expect("rebind back to A");

        wait_for_session_count(&daemon_db, &workspace_a_id, 1).await;
        wait_for_session_count(&daemon_db, &workspace_b_id, 1).await;

        for workspace_id in workspace_ids_to_disconnect(
            &workspace_a_id,
            handler.session_attached_workspace_ids().await,
            true,
        ) {
            pool.disconnect_session(&workspace_id).await;
        }

        wait_for_session_count(&daemon_db, &workspace_a_id, 0).await;
        wait_for_session_count(&daemon_db, &workspace_b_id, 0).await;
    }

    // Version-gate tests (Finding #1 regression): the accept-loop's adapter↔daemon
    // version compatibility check must reject a mismatched adapter session when
    // there are active sessions, not just flag restart_pending and fall through.
    mod version_gate {
        use crate::daemon::lifecycle::{IncomingSessionAction, RestartReason, version_gate_action};

        #[test]
        fn matching_versions_proceed() {
            let outcome = version_gate_action(Some("1.2.3"), "1.2.3", 0);
            assert_eq!(outcome, IncomingSessionAction::Accept);

            let outcome = version_gate_action(Some("1.2.3"), "1.2.3", 5);
            assert_eq!(outcome, IncomingSessionAction::Accept);
        }

        #[test]
        fn legacy_adapter_without_version_header_proceeds() {
            // Pre-v6.5.3 adapters don't send VERSION. The gate must not reject
            // them — they've been working fine and we keep backwards compat.
            let outcome = version_gate_action(None, "6.7.0", 0);
            assert_eq!(outcome, IncomingSessionAction::Accept);

            let outcome = version_gate_action(None, "6.7.0", 3);
            assert_eq!(outcome, IncomingSessionAction::Accept);
        }

        #[test]
        fn mismatch_with_no_active_sessions_shuts_down_immediately() {
            let outcome = version_gate_action(Some("6.8.0"), "6.7.0", 0);
            assert_eq!(
                outcome,
                IncomingSessionAction::ShutdownForRestart(RestartReason::VersionMismatch)
            );
        }

        #[test]
        fn mismatch_with_active_sessions_rejects_new_session() {
            // THE BUG: before the fix, this branch set restart_pending and fell
            // through to serve the mismatched session. The fix is to reject the
            // new adapter cleanly so it retries once the old daemon drains.
            let outcome = version_gate_action(Some("6.8.0"), "6.7.0", 1);
            assert_eq!(
                outcome,
                IncomingSessionAction::RejectForRestart(RestartReason::VersionMismatch)
            );

            let outcome = version_gate_action(Some("6.8.0"), "6.7.0", 42);
            assert_eq!(
                outcome,
                IncomingSessionAction::RejectForRestart(RestartReason::VersionMismatch)
            );
        }

        #[test]
        fn older_adapter_vs_newer_daemon_is_also_a_mismatch() {
            // Both directions trigger the gate. A newer daemon still shouldn't
            // serve an older adapter because they disagree on the protocol.
            let outcome = version_gate_action(Some("6.6.0"), "6.7.0", 0);
            assert_eq!(
                outcome,
                IncomingSessionAction::ShutdownForRestart(RestartReason::VersionMismatch)
            );

            let outcome = version_gate_action(Some("6.6.0"), "6.7.0", 1);
            assert_eq!(
                outcome,
                IncomingSessionAction::RejectForRestart(RestartReason::VersionMismatch)
            );
        }
    }
}
