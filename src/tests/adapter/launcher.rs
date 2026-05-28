//! Tests for the adapter's DaemonLauncher (auto-start daemon, HTTP readiness).

#[cfg(test)]
mod tests {
    use crate::adapter::launcher::DaemonLauncher;
    use crate::adapter::launcher::DaemonReadiness;
    use crate::daemon::discovery::{DiscoveryFile, DiscoveryRecord};
    use crate::daemon::pid::PidFile;
    use crate::daemon::transport::TransportEndpoint;
    use crate::paths::DaemonPaths;
    use std::fs;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread::{self, JoinHandle};
    use std::time::Duration;

    #[test]
    fn test_daemon_paths_includes_state_file() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        let state_path = paths.daemon_state();
        assert_eq!(state_path, dir.path().join("daemon.state"));
    }

    #[test]
    fn test_daemon_not_running_when_no_pid_file() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        let launcher = DaemonLauncher::new(paths);
        assert_eq!(launcher.daemon_readiness(), DaemonReadiness::Dead);
    }

    #[test]
    fn test_readiness_dead_ignores_legacy_pid_and_state_without_discovery() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        fs::create_dir_all(dir.path()).unwrap();
        let _pid_file = PidFile::create(&paths.daemon_pid()).unwrap();
        fs::write(paths.daemon_state(), "ready").unwrap();

        let launcher = DaemonLauncher::new(paths.clone());

        assert_eq!(
            launcher.daemon_readiness(),
            DaemonReadiness::Dead,
            "adapter readiness must use discovery.json as the new-daemon lifecycle source; \
             legacy daemon.pid + daemon.state alone are not enough"
        );
        assert!(
            paths.daemon_state().exists(),
            "readiness must not mutate legacy state files when discovery.json is absent"
        );
    }

    #[test]
    fn test_readiness_dead_ignores_legacy_pid_without_discovery() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        fs::create_dir_all(dir.path()).unwrap();
        let _pid_file = PidFile::create(&paths.daemon_pid()).unwrap();
        let launcher = DaemonLauncher::new(paths);
        assert_eq!(launcher.daemon_readiness(), DaemonReadiness::Dead);
    }

    #[test]
    fn test_readiness_does_not_clean_legacy_pid_without_discovery() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        fs::create_dir_all(dir.path()).unwrap();
        fs::write(paths.daemon_pid(), "99999999\n").unwrap();
        let launcher = DaemonLauncher::new(paths.clone());
        assert_eq!(launcher.daemon_readiness(), DaemonReadiness::Dead);
        assert!(
            paths.daemon_pid().exists(),
            "new-daemon readiness must not mutate legacy PID files"
        );
    }

    #[test]
    fn test_readiness_dead_ignores_empty_legacy_pid_without_discovery() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        fs::create_dir_all(dir.path()).unwrap();
        fs::write(paths.daemon_pid(), b"").unwrap();
        fs::write(paths.daemon_state(), "starting").unwrap();

        let launcher = DaemonLauncher::new(paths.clone());

        assert_eq!(
            launcher.daemon_readiness(),
            DaemonReadiness::Dead,
            "without discovery.json, even a fresh legacy PID file is not \
             new-daemon liveness"
        );
        assert!(
            paths.daemon_pid().exists(),
            "readiness must not unlink legacy PID files"
        );
        assert!(
            paths.daemon_state().exists(),
            "readiness must not unlink legacy state files"
        );
    }

    #[test]
    fn test_launcher_uses_correct_paths() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        let launcher = DaemonLauncher::new(paths.clone());
        // Verify the launcher's paths match what we gave it
        assert_eq!(launcher.paths().julie_home(), paths.julie_home());
    }

    #[test]
    fn test_readiness_dead_when_no_pid() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        let launcher = DaemonLauncher::new(paths);
        assert_eq!(launcher.daemon_readiness(), DaemonReadiness::Dead);
    }

    #[test]
    fn test_readiness_dead_preserves_legacy_state_without_discovery() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        fs::write(paths.daemon_state(), "ready").unwrap();
        let launcher = DaemonLauncher::new(paths.clone());
        assert_eq!(launcher.daemon_readiness(), DaemonReadiness::Dead);
        assert!(
            paths.daemon_state().exists(),
            "new-daemon readiness must not mutate legacy state files"
        );
    }

    fn spawn_http_readiness_server(listener: TcpListener) -> JoinHandle<()> {
        spawn_http_readiness_server_requests(listener, 1)
    }

    fn spawn_http_readiness_server_requests(
        listener: TcpListener,
        requests: usize,
    ) -> JoinHandle<()> {
        thread::spawn(move || {
            for _ in 0..requests {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = Vec::new();
                loop {
                    let mut chunk = [0u8; 256];
                    let n = stream.read(&mut chunk).unwrap();
                    assert_ne!(n, 0, "client closed before sending full HTTP request");
                    request.extend_from_slice(&chunk[..n]);
                    if request.windows(4).any(|window| window == b"\r\n\r\n") {
                        break;
                    }
                }
                let request = String::from_utf8_lossy(&request);
                assert!(request.starts_with("GET /mcp/ready HTTP/1.1"));
                stream
                    .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")
                    .unwrap();
            }
        })
    }

    fn write_live_discovery(paths: &DaemonPaths, port: u16, phase: &str) -> std::path::PathBuf {
        let token_path = paths.token_file();
        let log_path = paths.julie_home().join("daemon.log");
        let mut record =
            DiscoveryRecord::for_current_process("127.0.0.1", port, token_path.clone(), log_path);
        record.phase = Some(phase.to_string());
        fs::write(&token_path, "test-token\n").unwrap();
        DiscoveryFile::write_atomic(&paths.discovery_file(), &record).unwrap();
        token_path
    }

    #[test]
    fn test_discovery_json_running_is_ready_without_pid_or_state() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        fs::create_dir_all(dir.path()).unwrap();

        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = spawn_http_readiness_server(listener);
        write_live_discovery(&paths, port, "running");

        let launcher = DaemonLauncher::new(paths);
        assert_eq!(launcher.daemon_readiness(), DaemonReadiness::Ready);
        server.join().unwrap();
    }

    #[test]
    fn test_discovery_json_stopping_is_stopping_without_pid_or_state() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        fs::create_dir_all(dir.path()).unwrap();
        write_live_discovery(&paths, 4242, "stopping");

        let launcher = DaemonLauncher::new(paths);
        assert_eq!(launcher.daemon_readiness(), DaemonReadiness::Stopping);
    }

    #[test]
    fn test_discovery_json_draining_is_stopping_without_pid_or_state() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        fs::create_dir_all(dir.path()).unwrap();
        write_live_discovery(&paths, 4242, "draining");

        let launcher = DaemonLauncher::new(paths);
        assert_eq!(launcher.daemon_readiness(), DaemonReadiness::Stopping);
    }

    #[test]
    fn test_transport_endpoint_uses_discovery_json_token_path() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        fs::create_dir_all(dir.path()).unwrap();
        let token_path = write_live_discovery(&paths, 4242, "running");

        let launcher = DaemonLauncher::new(paths);
        let endpoint = launcher.transport_endpoint().expect("transport endpoint");

        assert_eq!(endpoint.token_path(), Some(token_path.as_path()));
        assert_eq!(
            endpoint.mcp_url().as_deref(),
            Some("http://127.0.0.1:4242/mcp")
        );
    }

    #[test]
    fn test_transport_endpoint_falls_back_to_legacy_mcp_transport_discovery() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        fs::create_dir_all(dir.path()).unwrap();
        let _pid_file = PidFile::create(&paths.daemon_pid()).unwrap();
        fs::write(paths.daemon_port(), "4243\n").unwrap();
        TransportEndpoint::streamable_http("127.0.0.1", 4242, "/mcp", "/mcp/ready", None)
            .unwrap()
            .publish_discovery(&paths.daemon_mcp_transport())
            .unwrap();

        let launcher = DaemonLauncher::new(paths);
        let endpoint = launcher.transport_endpoint().expect("legacy endpoint");

        assert_eq!(endpoint.token_path(), None);
        assert_eq!(
            endpoint.mcp_url().as_deref(),
            Some("http://127.0.0.1:4242/mcp")
        );
    }

    #[test]
    fn test_ensure_daemon_ready_attaches_to_legacy_transport_without_discovery_json() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        fs::create_dir_all(dir.path()).unwrap();
        let _pid_file = PidFile::create(&paths.daemon_pid()).unwrap();
        TransportEndpoint::streamable_http("127.0.0.1", 4242, "/mcp", "/mcp/ready", None)
            .unwrap()
            .publish_discovery(&paths.daemon_mcp_transport())
            .unwrap();

        let launcher = DaemonLauncher::new(paths);

        assert!(
            launcher.ensure_daemon_ready().is_ok(),
            "legacy attach remains explicit even though new-daemon readiness ignores PID/state"
        );
    }

    #[test]
    fn test_transport_endpoint_refuses_stale_transport_discovery_without_live_legacy_pid() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        fs::create_dir_all(dir.path()).unwrap();
        TransportEndpoint::streamable_http("127.0.0.1", 4242, "/mcp", "/mcp/ready", None)
            .unwrap()
            .publish_discovery(&paths.daemon_mcp_transport())
            .unwrap();

        let launcher = DaemonLauncher::new(paths);
        let error = launcher
            .transport_endpoint()
            .expect_err("stale daemon-mcp-transport.json must not be used without live legacy PID");

        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn test_readiness_dead_ignores_legacy_transport_discovery_without_discovery_json() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        fs::create_dir_all(dir.path()).unwrap();
        let _pid_file = PidFile::create(&paths.daemon_pid()).unwrap();

        let endpoint =
            TransportEndpoint::streamable_http("127.0.0.1", 4242, "/mcp", "/mcp/ready", None)
                .unwrap();
        endpoint
            .publish_discovery(&paths.daemon_mcp_transport())
            .unwrap();

        let launcher = DaemonLauncher::new(paths);
        assert_eq!(launcher.daemon_readiness(), DaemonReadiness::Dead);
    }

    #[test]
    fn test_ensure_daemon_ready_returns_ok_when_ready() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        fs::create_dir_all(dir.path()).unwrap();

        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = spawn_http_readiness_server(listener);
        write_live_discovery(&paths, port, "running");

        let launcher = DaemonLauncher::new(paths);
        let result = launcher.ensure_daemon_ready();
        server.join().unwrap();
        assert!(result.is_ok());
    }

    #[test]
    fn test_ensure_daemon_ready_waits_for_starting_to_ready() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        fs::create_dir_all(dir.path()).unwrap();

        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = spawn_http_readiness_server_requests(listener, 2);
        write_live_discovery(&paths, port, "starting");

        let paths_clone = paths.clone();
        let handle = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            write_live_discovery(&paths_clone, port, "running");
        });

        let launcher = DaemonLauncher::new(paths);
        let result = launcher.ensure_daemon_ready();
        handle.join().unwrap();
        server.join().unwrap();
        assert!(result.is_ok());
    }

    /// Helper: run N adapters concurrently against
    /// `spawn_under_startup_lock_with`, where each spawn_fn schedules a
    /// "fake daemon" background thread that publishes liveness after
    /// `liveness_delay`. Asserts exactly one spawn_fn call ran.
    fn run_cascade_test_with<F>(n_adapters: usize, liveness_delay: Duration, publish_liveness: F)
    where
        F: Fn(&DaemonPaths) + Send + Sync + 'static + Clone,
    {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::{Arc, Barrier};

        const LIVENESS_WAIT_TIMEOUT: Duration = Duration::from_secs(5);

        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        fs::create_dir_all(dir.path()).unwrap();

        let spawn_count = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(Barrier::new(n_adapters));

        let mut handles = vec![];
        for _ in 0..n_adapters {
            let paths = paths.clone();
            let count = Arc::clone(&spawn_count);
            let barrier = Arc::clone(&barrier);
            let publish = publish_liveness.clone();

            handles.push(thread::spawn(move || {
                let launcher = DaemonLauncher::new(paths.clone());
                barrier.wait();
                launcher
                    .spawn_under_startup_lock_with(
                        || {
                            // Mimic cmd.spawn() returning before the child
                            // has written any liveness file: spin up a
                            // background "daemon" thread that publishes
                            // liveness after a delay.
                            let paths = paths.clone();
                            let publish = publish.clone();
                            std::thread::spawn(move || {
                                std::thread::sleep(liveness_delay);
                                publish(&paths);
                            });
                            count.fetch_add(1, Ordering::SeqCst);
                            Ok(())
                        },
                        LIVENESS_WAIT_TIMEOUT,
                    )
                    .unwrap();
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(
            spawn_count.load(Ordering::SeqCst),
            1,
            "exactly one launcher must spawn; the rest must observe \
             liveness under lock and skip (count was {})",
            spawn_count.load(Ordering::SeqCst)
        );
    }

    /// Regression for the concurrent-adapter spawn cascade observed in
    /// daemon.log at 2026-05-17T09:24:33: six adapters all spawned a
    /// daemon within the same millisecond. Root cause was that
    /// `spawn_under_lock` released `daemon-startup.lock` immediately after
    /// the `cmd.spawn()` syscall returned — before the new daemon could
    /// publish liveness — so the next waiting adapter's re-check of
    /// `daemon_readiness()` still saw `Dead` and spawned another daemon.
    /// The daemon-side singleton lock killed the losers silently, but
    /// each loser burned a fork+exec.
    ///
    /// The fix: hold the startup lock across the spawn AND across a short
    /// wait for the spawned daemon to publish liveness. Subsequent
    /// adapters then see `Starting` on their re-check and skip the spawn
    /// entirely.
    ///
    /// This exercises the production new-daemon signal: `discovery.json`
    /// written with phase=running. This is the path modern daemons take —
    /// `app_test.rs::test_daemon_app_does_not_write_legacy_artifacts` asserts
    /// new daemons do NOT write `daemon.pid`.
    #[test]
    fn test_spawn_under_startup_lock_serializes_concurrent_spawns_via_discovery_json() {
        run_cascade_test_with(6, Duration::from_millis(100), |paths| {
            // Production signal: write a Live DiscoveryRecord. The
            // transport endpoint won't actually respond on this port,
            // so daemon_readiness returns Starting (not Ready) — which
            // is exactly what an adapter sees during the spawn window.
            write_live_discovery(paths, 1, "running");
        });
    }

    /// Regression for the cold-start "zombie spawn" race observed in
    /// adapter.log around 2026-05-17T19:40-19:42:
    ///
    /// ```
    /// 19:40:40.328  Daemon not running, spawning...    ← outer check: Dead
    /// 19:41:09.450  Daemon not running, spawning...    ← no "Spawning daemon:" between
    /// 19:41:09.965  Daemon not running, spawning...
    /// 19:42:03.505  Daemon not running, spawning...
    /// 19:42:03.505  Spawning daemon: ...               ← finally spawned, 83s later
    /// ```
    ///
    /// Multiple "Daemon not running" logs with NO matching "Spawning daemon:"
    /// = `spawn_under_startup_lock_with` skipped `spawn_fn` (re-check inside
    /// the lock saw non-Dead) without telling the caller. The caller then
    /// blindly called `poll_for_readiness_change`, which polled the
    /// dying daemon for a "ready" state it would never reach — burning the
    /// full drain window before the loop could recover.
    ///
    /// Fix: `spawn_under_startup_lock_with` returns `bool` indicating whether
    /// spawn_fn actually ran. Callers that see `false` know to re-evaluate
    /// readiness from scratch instead of polling for "ready".
    #[test]
    fn test_spawn_under_startup_lock_returns_false_when_recheck_sees_stopping() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        fs::create_dir_all(dir.path()).unwrap();

        // Pre-populate: existing daemon mid-drain. Discovery phase=stopping
        // is what the re-check inside the lock will observe.
        write_live_discovery(&paths, 1, "stopping");

        let launcher = DaemonLauncher::new(paths);
        let spawn_called = Arc::new(AtomicBool::new(false));
        let spawn_called_clone = Arc::clone(&spawn_called);

        let result = launcher.spawn_under_startup_lock_with(
            || {
                spawn_called_clone.store(true, Ordering::SeqCst);
                Ok(())
            },
            Duration::from_millis(100),
        );

        let spawn_ran = result.expect("spawn_under_startup_lock_with should succeed");
        assert!(
            !spawn_ran,
            "spawn_fn must NOT be reported as run when re-check found a non-Dead daemon"
        );
        assert!(
            !spawn_called.load(Ordering::SeqCst),
            "spawn_fn closure must not be called when re-check sees Stopping"
        );
    }

    /// Companion to the above: when the re-check inside the lock still sees
    /// Dead (no race), spawn_fn IS called and the return value reports
    /// `true`. Locks in the contract from the happy path.
    #[test]
    fn test_spawn_under_startup_lock_returns_true_when_spawn_ran() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        fs::create_dir_all(dir.path()).unwrap();

        // No PID file, no discovery.json → readiness is Dead.
        let launcher = DaemonLauncher::new(paths);
        let spawn_called = Arc::new(AtomicBool::new(false));
        let spawn_called_clone = Arc::clone(&spawn_called);

        let result = launcher.spawn_under_startup_lock_with(
            || {
                spawn_called_clone.store(true, Ordering::SeqCst);
                Ok(())
            },
            Duration::from_millis(100),
        );

        let spawn_ran = result.expect("spawn_under_startup_lock_with should succeed");
        assert!(
            spawn_ran,
            "spawn_fn must be reported as run when Dead at re-check"
        );
        assert!(
            spawn_called.load(Ordering::SeqCst),
            "spawn_fn closure must be invoked when Dead at re-check"
        );
    }

    /// Regression for Codex 2026-05-27 adversarial review finding #2.
    ///
    /// The cold-start fix in `run_daemon` publishes `discovery.json` with
    /// `phase="starting"` immediately after the listener binds, BEFORE
    /// `DaemonApp::new` runs DB migrations and workspace backfill. During
    /// that window the kernel TCP stack can already complete handshakes on
    /// the bound listener even though the HTTP server is not yet routing
    /// requests — so `endpoint.probe_readiness()` may falsely return
    /// `Ready`. Previously `daemon_readiness()` only short-circuited on
    /// `stopping`/`draining`, leaving `starting` records to fall through
    /// to the endpoint probe; an adapter could open a session against a
    /// daemon that hadn't finished initializing.
    ///
    /// Contract this test pins: a `phase="starting"` discovery record
    /// must classify as `Starting`, even when the HTTP endpoint at the
    /// recorded port is reachable. The flip to `Ready` happens only when
    /// the late publish at the end of `DaemonApp::serve` atomically
    /// overwrites the record with `phase="running"`.
    ///
    /// To prove the endpoint is genuinely reachable (and that the test
    /// would have caught the old behavior of returning `Ready`), we then
    /// flip the phase to `"running"` and assert the launcher transitions
    /// to `Ready` against the same listener.
    #[test]
    fn test_readiness_starting_when_discovery_phase_is_starting_even_with_reachable_endpoint() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        fs::create_dir_all(dir.path()).unwrap();

        // Bind a real TCP listener — between early publish and serve() the
        // kernel can accept connections even without the HTTP layer
        // routing requests. This is the precise condition that lets
        // `probe_readiness` return success against a daemon that is not
        // yet ready for sessions.
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = spawn_http_readiness_server(listener);

        write_live_discovery(&paths, port, "starting");

        let launcher = DaemonLauncher::new(paths.clone());
        assert_eq!(
            launcher.daemon_readiness(),
            DaemonReadiness::Starting,
            "phase=starting must short-circuit to Starting; otherwise adapters \
             open sessions against a daemon that hasn't finished initializing"
        );

        // Flip phase to running and verify the launcher now returns Ready,
        // consuming the spawned HTTP server's single accept. This proves
        // the endpoint was genuinely reachable — without this leg the
        // first assertion could pass for the wrong reason (e.g. probe
        // returning Starting because the endpoint was unreachable).
        write_live_discovery(&paths, port, "running");
        assert_eq!(
            launcher.daemon_readiness(),
            DaemonReadiness::Ready,
            "phase=running against a reachable endpoint must return Ready; \
             this leg proves the Starting verdict above was driven by the \
             phase check, not by a dead endpoint"
        );
        server.join().unwrap();
    }

    // NOTE: An earlier version of this test file asserted that the kernel
    // `daemon.lock` alone (no discovery.json, no daemon.pid) was sufficient
    // to make the launcher classify the daemon as Starting. That contract
    // was withdrawn after Codex's 2026-05-27 adversarial review found that
    // any lock-probe mechanism that briefly acquires `daemon.lock` would
    // race the daemon's own `acquire_or_yield_to_existing_daemon` and
    // cause it to silently exit. The current architecture relies on the
    // early `phase="starting"` discovery publish in `run_daemon`
    // (`publish_starting_discovery`) to close the cold-start race window
    // instead. The starting-phase short-circuit in
    // `test_readiness_starting_when_discovery_phase_is_starting_even_with_reachable_endpoint`
    // is what protects adapters once that record exists.

    /// When a daemon transitions from "starting" to "draining" (e.g. stale
    /// binary detected during startup), readiness should classify it as a
    /// shutdown handoff instead of ready for fresh sessions.
    #[test]
    fn test_readiness_reclassifies_starting_to_draining_as_stopping() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        fs::create_dir_all(dir.path()).unwrap();

        write_live_discovery(&paths, 4242, "starting");

        let launcher = DaemonLauncher::new(paths.clone());
        assert_eq!(launcher.daemon_readiness(), DaemonReadiness::Starting);

        write_live_discovery(&paths, 4242, "draining");
        assert_eq!(launcher.daemon_readiness(), DaemonReadiness::Stopping);
    }
}
