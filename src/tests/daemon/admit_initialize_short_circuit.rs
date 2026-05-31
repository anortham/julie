/// Tests that `admit_initialize` short-circuits on the first failed admission gate.
///
/// When the stale-binary gate returns an error, `admit_initialize` must not invoke
/// the version gate at all. Without the short-circuit, a single admit attempt could
/// call `mark_restart_pending` twice and emit two "rejecting" log lines for the same
/// request.
///
/// The short-circuit is implemented via Rust's `?` operator after the first
/// `apply_admission_action` call. These tests verify the invariant holds by checking
/// the `apply_action_call_count` counter on `HttpSessionAdmission`, which is
/// incremented once per `apply_admission_action` invocation.
#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::{Duration, SystemTime};

    use anyhow::Context;

    use crate::daemon::database::DaemonDatabase;
    use crate::daemon::embedding_service::EmbeddingService;
    use crate::daemon::http_transport::{HttpTransportConfig, HttpTransportServer, MCP_PATH};
    use crate::daemon::lifecycle::DaemonLifecycleController;
    use crate::daemon::mcp_session::{
        DaemonSessionDependencies, HEADER_JULIE_VERSION, HEADER_JULIE_WORKSPACE,
        HEADER_JULIE_WORKSPACE_SOURCE, HttpJulieService, HttpSessionAdmission,
    };
    use crate::daemon::session::SessionTracker;
    use crate::daemon::workspace_pool::WorkspacePool;
    use crate::paths::DaemonPaths;
    use crate::workspace::startup_hint::WorkspaceStartupSource;

    /// Build the standard initialize request body and POST it to the server.
    /// Returns the raw HTTP response as a string.
    fn post_initialize(
        addr: std::net::SocketAddr,
        workspace: &std::path::Path,
        version: &str,
    ) -> String {
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"julie-test","version":"0.0.0"}}}"#;
        let request = format!(
            "POST {MCP_PATH} HTTP/1.1\r\n\
             Host: 127.0.0.1:{port}\r\n\
             Content-Type: application/json\r\n\
             Accept: application/json, text/event-stream\r\n\
             Content-Length: {len}\r\n\
             Connection: close\r\n\
             {HEADER_JULIE_WORKSPACE}: {workspace}\r\n\
             {HEADER_JULIE_WORKSPACE_SOURCE}: {source}\r\n\
             {HEADER_JULIE_VERSION}: {version}\r\n\
             \r\n\
             {body}",
            port = addr.port(),
            len = body.len(),
            workspace = workspace.display(),
            source = WorkspaceStartupSource::Cli.as_header_value(),
        );
        let mut stream = TcpStream::connect(addr).unwrap();
        stream.write_all(request.as_bytes()).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    }

    /// Fixture that wires up the full daemon HTTP stack with a configurable
    /// binary-staleness probe and an observable `apply_action_call_count`.
    struct AdmissionFixture {
        _home: tempfile::TempDir,
        workspace_root: tempfile::TempDir,
        paths: DaemonPaths,
        dependencies: Arc<DaemonSessionDependencies>,
        /// Shared counter: incremented once per `apply_admission_action` call.
        call_count: Arc<AtomicUsize>,
        lifecycle: DaemonLifecycleController,
    }

    impl AdmissionFixture {
        fn new(
            startup_binary_mtime: Option<SystemTime>,
            current_binary_mtime: impl Fn() -> Option<SystemTime> + Send + Sync + 'static,
        ) -> Self {
            let home = tempfile::tempdir().unwrap();
            let workspace_root = tempfile::tempdir().unwrap();
            std::fs::create_dir_all(workspace_root.path().join(".julie"))
                .expect("create workspace .julie dir");
            let paths = DaemonPaths::with_home(home.path().join("julie-home"));
            paths.ensure_dirs().expect("create daemon dirs");
            let daemon_db = Arc::new(
                DaemonDatabase::open(&paths.daemon_db())
                    .context("open daemon db")
                    .expect("open daemon db"),
            );
            let embedding_service = Arc::new(EmbeddingService::initializing());
            let pool = Arc::new(WorkspacePool::new(
                paths.indexes_dir(),
                Some(Arc::clone(&daemon_db)),
            ));
            let sessions = Arc::new(SessionTracker::new());
            let lifecycle = DaemonLifecycleController::new(paths.daemon_state());
            let admission = HttpSessionAdmission::new(
                lifecycle.clone(),
                startup_binary_mtime,
                current_binary_mtime,
            );
            // Clone the counter Arc before moving admission into dependencies.
            let call_count = Arc::clone(&admission.apply_action_call_count);
            let dependencies = Arc::new(
                DaemonSessionDependencies::new(
                    pool,
                    Some(Arc::clone(&daemon_db)),
                    embedding_service,
                    Arc::new(AtomicBool::new(false)),
                    None,
                    None,
                    sessions,
                    Arc::clone(crate::workspace::mutation_gate::Registry::global()),
                )
                .with_http_admission(admission),
            );
            Self {
                _home: home,
                workspace_root,
                paths,
                dependencies,
                call_count,
                lifecycle,
            }
        }
    }

    /// When the stale-binary gate fires (returns `Err` via `ShutdownForRestart`),
    /// `admit_initialize` must short-circuit and never invoke the version gate.
    /// Observable: `apply_action_call_count` is 1, not 2.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_admit_initialize_short_circuits_on_stale_binary_reject() {
        // Binary is stale with 0 active sessions: stale-binary gate produces
        // ShutdownForRestart, which apply_admission_action converts to Err.
        let fixture = AdmissionFixture::new(Some(SystemTime::UNIX_EPOCH), || {
            Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1))
        });
        let dependencies = Arc::clone(&fixture.dependencies);
        let call_count = Arc::clone(&fixture.call_count);
        let server = HttpTransportServer::bind(
            fixture.paths.clone(),
            HttpTransportConfig::default(),
            move || Ok(HttpJulieService::new(Arc::clone(&dependencies))),
        )
        .await
        .unwrap();

        // Send a mismatched version so the version gate would have produced a
        // second error if it ran. If the short-circuit is missing, the count
        // would be 2 (stale gate + version gate). With the short-circuit it
        // must be exactly 1 (stale gate only).
        let response = post_initialize(
            server.local_addr(),
            fixture.workspace_root.path(),
            "0.0.0-mismatched",
        );

        assert!(
            response.starts_with("HTTP/1.1 200 OK"),
            "expect an HTTP 200 wrapping the JSON-RPC error: {response}"
        );
        assert!(
            response.contains(r#""code":-32603"#),
            "stale-binary gate must produce an internal error: {response}"
        );
        assert!(
            response.contains("restart"),
            "error message must mention restart: {response}"
        );

        // The stale-binary gate errored, so admit_initialize returned after the
        // first apply_admission_action call. The version gate was never reached.
        assert_eq!(
            call_count.load(Ordering::Relaxed),
            1,
            "apply_admission_action must be called exactly once when the stale-binary gate rejects"
        );
        assert!(
            fixture.lifecycle.restart_pending(),
            "stale-binary rejection must mark restart pending"
        );

        server.shutdown().await.unwrap();
    }

    /// When the stale-binary gate passes (returns `Ok`), `admit_initialize` must
    /// proceed to run the version gate. Observable: `apply_action_call_count` is 2.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_admit_initialize_runs_version_gate_when_stale_passes() {
        // Binary is NOT stale: stale-binary gate produces Accept (Ok).
        // No sessions active + mismatched version: version gate produces
        // ShutdownForRestart, which is also Err — but crucially the second
        // apply_admission_action must have been called.
        let fixture = AdmissionFixture::new(
            Some(SystemTime::UNIX_EPOCH),
            // current_mtime == startup_mtime => NOT stale (not greater than)
            || Some(SystemTime::UNIX_EPOCH),
        );
        let dependencies = Arc::clone(&fixture.dependencies);
        let call_count = Arc::clone(&fixture.call_count);
        let server = HttpTransportServer::bind(
            fixture.paths.clone(),
            HttpTransportConfig::default(),
            move || Ok(HttpJulieService::new(Arc::clone(&dependencies))),
        )
        .await
        .unwrap();

        // Send a NEWER version so the version gate fires ShutdownForRestart
        // (an older version would be rejected without restart and not exercise
        // the restart-pending path this test asserts).
        let response = post_initialize(
            server.local_addr(),
            fixture.workspace_root.path(),
            "999.999.999",
        );

        assert!(
            response.starts_with("HTTP/1.1 200 OK"),
            "expect HTTP 200 wrapping JSON-RPC error: {response}"
        );
        assert!(
            response.contains(r#""code":-32603"#),
            "version-gate must produce an internal error: {response}"
        );

        // Both gates must have run: stale-binary gate (Accept) + version gate (Err).
        assert_eq!(
            call_count.load(Ordering::Relaxed),
            2,
            "apply_admission_action must be called twice when the stale-binary gate passes"
        );
        assert!(
            fixture.lifecycle.restart_pending(),
            "version-gate rejection must mark restart pending"
        );

        server.shutdown().await.unwrap();
    }
}
