use super::*;

/// When the daemon binary goes stale while sessions are still running, the
/// first incoming session after staleness sets `restart_pending` (and is
/// admitted as `AcceptWithRestartPending`). Subsequent incoming sessions
/// must be rejected so adapters reconnect against the rebuilt binary
/// instead of piling onto a daemon that has already decided to restart.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_http_julie_session_rejects_new_sessions_after_restart_pending_with_active() {
    let stale_now = Arc::new(AtomicBool::new(false));
    let stale_now_for_probe = Arc::clone(&stale_now);
    let fixture = RealServiceFixture::new_with_admission(Some(SystemTime::UNIX_EPOCH), move || {
        if stale_now_for_probe.load(Ordering::SeqCst) {
            Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1))
        } else {
            Some(SystemTime::UNIX_EPOCH)
        }
    });
    let workspace_id = fixture.workspace_id();
    let dependencies = Arc::clone(&fixture.dependencies);
    let server = HttpTransportServer::bind(
        fixture.paths.clone(),
        HttpTransportConfig::default(),
        move || Ok(HttpJulieService::new(Arc::clone(&dependencies))),
    )
    .await
    .unwrap();

    // Session 1: binary not stale yet, admit normally.
    let r1 = post_initialize(
        server.local_addr(),
        InitializeRequestOptions {
            workspace: Some(fixture.workspace_root.path()),
            workspace_source: Some(WorkspaceStartupSource::Cli),
            version: Some(env!("CARGO_PKG_VERSION")),
            ..InitializeRequestOptions::default()
        },
    );
    assert!(
        r1.starts_with("HTTP/1.1 200 OK"),
        "session 1 must succeed before staleness flips: {r1}"
    );
    wait_for_session_count(&fixture.daemon_db, &workspace_id, 1).await;
    assert!(
        !fixture.lifecycle.restart_pending(),
        "restart_pending must not be set before staleness is observed"
    );

    // Binary becomes stale.
    stale_now.store(true, Ordering::SeqCst);

    // Session 2: stale=true, active=1, restart_pending=false
    //   -> AcceptWithRestartPending. Session admitted, restart_pending flips.
    let r2 = post_initialize(
        server.local_addr(),
        InitializeRequestOptions {
            workspace: Some(fixture.workspace_root.path()),
            workspace_source: Some(WorkspaceStartupSource::Cli),
            version: Some(env!("CARGO_PKG_VERSION")),
            ..InitializeRequestOptions::default()
        },
    );
    assert!(
        r2.starts_with("HTTP/1.1 200 OK"),
        "session 2 must be accepted with restart pending: {r2}"
    );
    wait_for_session_count(&fixture.daemon_db, &workspace_id, 2).await;
    assert!(
        fixture.lifecycle.restart_pending(),
        "first stale-binary admission must mark daemon for restart"
    );

    // Session 3: stale=true, active>=1, restart_pending=true
    //   -> RejectForRestart (the new behavior under test).
    let r3 = post_initialize(
        server.local_addr(),
        InitializeRequestOptions {
            workspace: Some(fixture.workspace_root.path()),
            workspace_source: Some(WorkspaceStartupSource::Cli),
            version: Some(env!("CARGO_PKG_VERSION")),
            ..InitializeRequestOptions::default()
        },
    );
    assert!(
        r3.contains(r#""code":-32603"#),
        "session 3 must be rejected with a JSON-RPC internal error: {r3}"
    );
    assert!(
        r3.contains("restart"),
        "session 3 rejection must tell the adapter to reconnect after restart: {r3}"
    );

    // The rejected session must not be added to the workspace counter.
    // Allow up to 200ms for any accidental admission to land.
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(10)).await;
        let row = fixture
            .daemon_db
            .get_workspace(&workspace_id)
            .unwrap()
            .expect("workspace row");
        assert!(
            row.session_count <= 2,
            "rejected stale-binary session must not be counted (saw {})",
            row.session_count
        );
    }

    server.shutdown().await.unwrap();
}
