//! Tests for the daemon Streamable HTTP MCP transport module.

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};
    use std::net::{SocketAddr, TcpStream};
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::time::{Duration, SystemTime};
    use std::{io::Read, io::Write};

    use anyhow::Context;
    use rmcp::ServerHandler;
    use tokio::sync::broadcast;

    use crate::daemon::database::DaemonDatabase;
    use crate::daemon::embedding_service::EmbeddingService;
    use crate::daemon::http_transport::{
        HttpTransportConfig, HttpTransportServer, MCP_PATH, READINESS_PATH,
    };
    use crate::daemon::lifecycle::DaemonLifecycleController;
    use crate::daemon::mcp_session::{
        DaemonSessionDependencies, HEADER_JULIE_VERSION, HEADER_JULIE_WORKSPACE,
        HEADER_JULIE_WORKSPACE_SOURCE, HttpJulieService, HttpSessionAdmission,
    };
    use crate::daemon::session::SessionTracker;
    use crate::daemon::transport::{TransportEndpoint, TransportMode, TransportProbe};
    use crate::daemon::workspace_pool::WorkspacePool;
    use crate::dashboard::state::DashboardEvent;
    use crate::paths::DaemonPaths;
    use crate::workspace::startup_hint::WorkspaceStartupSource;

    #[derive(Clone)]
    struct TestMcpHandler;

    impl ServerHandler for TestMcpHandler {}

    #[derive(Default)]
    struct InitializeRequestOptions<'a> {
        host: Option<String>,
        origin: Option<&'a str>,
        bearer_token: Option<&'a str>,
        workspace: Option<&'a std::path::Path>,
        workspace_source: Option<WorkspaceStartupSource>,
        version: Option<&'a str>,
    }

    fn post_initialize(addr: SocketAddr, options: InitializeRequestOptions<'_>) -> String {
        post_initialize_raw(addr, options, &[])
    }

    fn post_initialize_raw(
        addr: SocketAddr,
        options: InitializeRequestOptions<'_>,
        extra_headers: &[(&str, &str)],
    ) -> String {
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"julie-test","version":"0.0.0"}}}"#;
        let host = options
            .host
            .unwrap_or_else(|| format!("127.0.0.1:{}", addr.port()));
        let mut request = format!(
            "POST {MCP_PATH} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n",
            body.len()
        );
        if let Some(origin) = options.origin {
            request.push_str(&format!("Origin: {origin}\r\n"));
        }
        if let Some(token) = options.bearer_token {
            request.push_str(&format!("Authorization: Bearer {token}\r\n"));
        }
        if let Some(workspace) = options.workspace {
            request.push_str(&format!(
                "{HEADER_JULIE_WORKSPACE}: {}\r\n",
                workspace.display()
            ));
        }
        if let Some(source) = options.workspace_source {
            request.push_str(&format!(
                "{HEADER_JULIE_WORKSPACE_SOURCE}: {}\r\n",
                source.as_header_value()
            ));
        }
        if let Some(version) = options.version {
            request.push_str(&format!("{HEADER_JULIE_VERSION}: {version}\r\n"));
        }
        for (name, value) in extra_headers {
            request.push_str(&format!("{name}: {value}\r\n"));
        }
        request.push_str("\r\n");
        request.push_str(body);

        let mut stream = TcpStream::connect(addr).unwrap();
        stream.write_all(request.as_bytes()).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    }

    fn delete_session(addr: SocketAddr, session_id: &str, bearer_token: Option<&str>) -> String {
        let mut request = format!(
            "DELETE {MCP_PATH} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nMcp-Session-Id: {session_id}\r\nConnection: close\r\n",
            addr.port()
        );
        if let Some(token) = bearer_token {
            request.push_str(&format!("Authorization: Bearer {token}\r\n"));
        }
        request.push_str("\r\n");

        let mut stream = TcpStream::connect(addr).unwrap();
        stream.write_all(request.as_bytes()).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    }

    fn response_header<'a>(response: &'a str, name: &str) -> Option<&'a str> {
        response.lines().find_map(|line| {
            line.split_once(':')
                .and_then(|(header, value)| header.eq_ignore_ascii_case(name).then(|| value.trim()))
        })
    }

    async fn wait_for_session_count(daemon_db: &DaemonDatabase, workspace_id: &str, expected: i64) {
        let mut last = None;
        for _ in 0..100 {
            if let Ok(Some(row)) = daemon_db.get_workspace(workspace_id) {
                if row.session_count == expected {
                    return;
                }
                last = Some(row.session_count);
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }

        panic!(
            "timed out waiting for workspace {workspace_id} session_count={expected}, last observed={:?}",
            last
        );
    }

    struct RealServiceFixture {
        _home: tempfile::TempDir,
        workspace_root: tempfile::TempDir,
        paths: DaemonPaths,
        daemon_db: Arc<DaemonDatabase>,
        dependencies: Arc<DaemonSessionDependencies>,
        sessions: Arc<SessionTracker>,
        lifecycle: DaemonLifecycleController,
    }

    impl RealServiceFixture {
        fn new() -> Self {
            Self::new_with_admission(None, || None)
        }

        fn new_with_admission(
            startup_binary_mtime: Option<SystemTime>,
            current_binary_mtime: impl Fn() -> Option<SystemTime> + Send + Sync + 'static,
        ) -> Self {
            Self::new_with_options(startup_binary_mtime, current_binary_mtime, None)
        }

        fn new_with_dashboard() -> (Self, broadcast::Receiver<DashboardEvent>) {
            let (dashboard_tx, dashboard_rx) = broadcast::channel(8);
            (
                Self::new_with_options(None, || None, Some(dashboard_tx)),
                dashboard_rx,
            )
        }

        fn new_with_options(
            startup_binary_mtime: Option<SystemTime>,
            current_binary_mtime: impl Fn() -> Option<SystemTime> + Send + Sync + 'static,
            dashboard_tx: Option<broadcast::Sender<DashboardEvent>>,
        ) -> Self {
            let home = tempfile::tempdir().unwrap();
            let workspace_root = tempfile::tempdir().unwrap();
            std::fs::create_dir_all(workspace_root.path().join(".julie"))
                .expect("create workspace .julie");
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
                None,
                Some(Arc::clone(&embedding_service)),
            ));
            let sessions = Arc::new(SessionTracker::new());
            let lifecycle = DaemonLifecycleController::new(paths.daemon_state());
            let dependencies = Arc::new(
                DaemonSessionDependencies::new(
                    pool,
                    Some(Arc::clone(&daemon_db)),
                    embedding_service,
                    Arc::new(AtomicBool::new(false)),
                    dashboard_tx,
                    None,
                    Arc::clone(&sessions),
                )
                .with_http_admission(HttpSessionAdmission::new(
                    lifecycle.clone(),
                    startup_binary_mtime,
                    current_binary_mtime,
                )),
            );

            Self {
                _home: home,
                workspace_root,
                paths,
                daemon_db,
                dependencies,
                sessions,
                lifecycle,
            }
        }

        fn workspace_id(&self) -> String {
            crate::workspace::registry::generate_workspace_id(
                &self.workspace_root.path().to_string_lossy(),
            )
            .expect("generate workspace id")
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_http_transport_binds_loopback_publishes_discovery_and_cleans_up() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().join("julie-home"));
        let server =
            HttpTransportServer::bind(paths.clone(), HttpTransportConfig::default(), || {
                Ok(TestMcpHandler)
            })
            .await
            .unwrap();

        let local_addr = server.local_addr();
        assert_eq!(local_addr.ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));

        let discovery_path = paths.daemon_mcp_transport();
        assert!(
            discovery_path.exists(),
            "HTTP transport discovery must be published after the listener binds"
        );

        let endpoint = TransportEndpoint::read_discovery(&discovery_path).unwrap();
        assert_eq!(endpoint.mode(), TransportMode::StreamableHttp);
        assert_eq!(
            endpoint.mcp_url().unwrap(),
            format!("http://127.0.0.1:{}{}", local_addr.port(), MCP_PATH)
        );
        assert_eq!(endpoint.probe_readiness(), TransportProbe::Ready);

        server.shutdown().await.unwrap();
        assert!(
            !discovery_path.exists(),
            "HTTP transport discovery must be removed during shutdown"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_http_transport_rejects_non_loopback_bind_host() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().join("julie-home"));
        let config = HttpTransportConfig {
            bind_host: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            ..HttpTransportConfig::default()
        };

        let result = HttpTransportServer::bind(paths.clone(), config, || Ok(TestMcpHandler)).await;

        assert!(result.is_err());
        assert!(
            !paths.daemon_mcp_transport().exists(),
            "failed HTTP transport binds must not publish discovery"
        );
    }

    #[test]
    fn test_http_transport_config_sets_sdk_session_policy_intentionally() {
        let config = HttpTransportConfig::default();
        let session_config = config.session_config();
        assert_eq!(session_config.init_timeout, Some(Duration::from_secs(60)));
        assert_eq!(session_config.keep_alive, Some(Duration::from_secs(300)));
        assert_eq!(session_config.sse_retry, Some(Duration::from_secs(3)));
        assert_eq!(config.mcp_path, MCP_PATH);
        assert_eq!(config.readiness_path, READINESS_PATH);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_http_transport_accepts_mcp_initialize_request() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().join("julie-home"));
        let server =
            HttpTransportServer::bind(paths, HttpTransportConfig::default(), || Ok(TestMcpHandler))
                .await
                .unwrap();

        let response = post_initialize(server.local_addr(), InitializeRequestOptions::default());

        assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
        assert!(
            response.to_ascii_lowercase().contains("mcp-session-id:"),
            "{response}"
        );
        assert!(response.contains("\"protocolVersion\""), "{response}");

        server.shutdown().await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_http_transport_requires_bearer_token_for_mcp_requests() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().join("julie-home"));
        let server = HttpTransportServer::bind(
            paths,
            HttpTransportConfig {
                bearer_token: Some("secret-token".to_string()),
                ..HttpTransportConfig::default()
            },
            || Ok(TestMcpHandler),
        )
        .await
        .unwrap();

        let response = post_initialize(server.local_addr(), InitializeRequestOptions::default());

        assert!(
            response.starts_with("HTTP/1.1 401 Unauthorized"),
            "{response}"
        );
        assert!(!response.to_ascii_lowercase().contains("mcp-session-id:"));

        server.shutdown().await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_http_transport_accepts_valid_bearer_token() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().join("julie-home"));
        let server = HttpTransportServer::bind(
            paths.clone(),
            HttpTransportConfig {
                bearer_token: Some("secret-token".to_string()),
                ..HttpTransportConfig::default()
            },
            || Ok(TestMcpHandler),
        )
        .await
        .unwrap();

        let endpoint = TransportEndpoint::read_discovery(&paths.daemon_mcp_transport()).unwrap();
        let discovery_body = std::fs::read_to_string(paths.daemon_mcp_transport()).unwrap();
        assert!(
            !discovery_body.contains("secret-token"),
            "discovery must point to the token file, not copy the bearer token"
        );
        let token_path = endpoint
            .token_path()
            .expect("token path should be published");
        assert_eq!(
            std::fs::read_to_string(token_path).unwrap(),
            "secret-token\n"
        );

        let response = post_initialize(
            server.local_addr(),
            InitializeRequestOptions {
                bearer_token: Some("secret-token"),
                ..InitializeRequestOptions::default()
            },
        );

        assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
        assert!(
            response.to_ascii_lowercase().contains("mcp-session-id:"),
            "{response}"
        );

        server.shutdown().await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_http_transport_rejects_invalid_bearer_token() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().join("julie-home"));
        let server = HttpTransportServer::bind(
            paths,
            HttpTransportConfig {
                bearer_token: Some("secret-token".to_string()),
                ..HttpTransportConfig::default()
            },
            || Ok(TestMcpHandler),
        )
        .await
        .unwrap();

        let response = post_initialize(
            server.local_addr(),
            InitializeRequestOptions {
                bearer_token: Some("wrong-token"),
                ..InitializeRequestOptions::default()
            },
        );

        assert!(
            response.starts_with("HTTP/1.1 401 Unauthorized"),
            "{response}"
        );

        server.shutdown().await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_http_transport_rejects_invalid_host_header() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().join("julie-home"));
        let server = HttpTransportServer::bind(
            paths,
            HttpTransportConfig {
                bearer_token: Some("secret-token".to_string()),
                ..HttpTransportConfig::default()
            },
            || Ok(TestMcpHandler),
        )
        .await
        .unwrap();

        let response = post_initialize(
            server.local_addr(),
            InitializeRequestOptions {
                host: Some("evil.example".to_string()),
                bearer_token: Some("secret-token"),
                ..InitializeRequestOptions::default()
            },
        );

        assert!(response.starts_with("HTTP/1.1 403 Forbidden"), "{response}");

        server.shutdown().await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_http_transport_rejects_foreign_origin() {
        let dir = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::with_home(dir.path().join("julie-home"));
        let server = HttpTransportServer::bind(
            paths,
            HttpTransportConfig {
                bearer_token: Some("secret-token".to_string()),
                ..HttpTransportConfig::default()
            },
            || Ok(TestMcpHandler),
        )
        .await
        .unwrap();

        let response = post_initialize(
            server.local_addr(),
            InitializeRequestOptions {
                origin: Some("https://evil.example"),
                bearer_token: Some("secret-token"),
                ..InitializeRequestOptions::default()
            },
        );

        assert!(response.starts_with("HTTP/1.1 403 Forbidden"), "{response}");

        server.shutdown().await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_http_julie_session_requires_workspace_header_before_initialize() {
        let fixture = RealServiceFixture::new();
        let dependencies = Arc::clone(&fixture.dependencies);
        let server = HttpTransportServer::bind(
            fixture.paths.clone(),
            HttpTransportConfig::default(),
            move || Ok(HttpJulieService::new(Arc::clone(&dependencies))),
        )
        .await
        .unwrap();

        let response = post_initialize(server.local_addr(), InitializeRequestOptions::default());

        assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
        assert!(
            response.contains(r#""code":-32602"#),
            "missing workspace header must be reported as JSON-RPC invalid params: {response}"
        );
        for _ in 0..100 {
            if fixture.sessions.active_count() == 0 {
                server.shutdown().await.unwrap();
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!(
            "failed initialize should remove its daemon session tracker entry, active={}",
            fixture.sessions.active_count()
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_http_julie_session_rejects_invalid_workspace_source_header() {
        let fixture = RealServiceFixture::new();
        let dependencies = Arc::clone(&fixture.dependencies);
        let server = HttpTransportServer::bind(
            fixture.paths.clone(),
            HttpTransportConfig::default(),
            move || Ok(HttpJulieService::new(Arc::clone(&dependencies))),
        )
        .await
        .unwrap();

        let response = post_initialize_raw(
            server.local_addr(),
            InitializeRequestOptions {
                workspace: Some(fixture.workspace_root.path()),
                version: Some(env!("CARGO_PKG_VERSION")),
                ..InitializeRequestOptions::default()
            },
            &[(HEADER_JULIE_WORKSPACE_SOURCE, "nope")],
        );

        assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
        assert!(
            response.contains(r#""code":-32602"#),
            "invalid workspace source must be reported as JSON-RPC invalid params: {response}"
        );
        assert!(
            response.contains("Invalid x-julie-workspace-source header: nope"),
            "{response}"
        );
        for _ in 0..100 {
            if fixture.sessions.active_count() == 0 {
                server.shutdown().await.unwrap();
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!(
            "invalid initialize should remove its daemon session tracker entry, active={}",
            fixture.sessions.active_count()
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_http_julie_session_rejects_version_mismatch_before_workspace_start() {
        let fixture = RealServiceFixture::new();
        let workspace_id = fixture.workspace_id();
        let dependencies = Arc::clone(&fixture.dependencies);
        let server = HttpTransportServer::bind(
            fixture.paths.clone(),
            HttpTransportConfig::default(),
            move || Ok(HttpJulieService::new(Arc::clone(&dependencies))),
        )
        .await
        .unwrap();

        let response = post_initialize(
            server.local_addr(),
            InitializeRequestOptions {
                workspace: Some(fixture.workspace_root.path()),
                workspace_source: Some(WorkspaceStartupSource::Cli),
                version: Some("0.0.0-mismatch"),
                ..InitializeRequestOptions::default()
            },
        );

        assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
        assert!(
            response.contains(r#""code":-32603"#),
            "version mismatch must be reported as a JSON-RPC internal error: {response}"
        );
        assert!(
            response.contains("restart"),
            "version mismatch should tell the adapter to reconnect after restart: {response}"
        );
        assert!(
            fixture.lifecycle.restart_pending(),
            "version mismatch should mark the daemon for restart"
        );
        let workspace_row = fixture.daemon_db.get_workspace(&workspace_id).unwrap();
        assert!(
            workspace_row
                .as_ref()
                .map(|row| row.session_count == 0)
                .unwrap_or(true),
            "version mismatch must not start a workspace session, row={workspace_row:?}"
        );
        for _ in 0..100 {
            if fixture.sessions.active_count() == 0 {
                server.shutdown().await.unwrap();
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!(
            "version-mismatched initialize should remove its daemon session tracker entry, active={}",
            fixture.sessions.active_count()
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_http_julie_session_version_mismatch_does_not_emit_dashboard_session_change() {
        let (fixture, mut dashboard_rx) = RealServiceFixture::new_with_dashboard();
        let dependencies = Arc::clone(&fixture.dependencies);
        let server = HttpTransportServer::bind(
            fixture.paths.clone(),
            HttpTransportConfig::default(),
            move || Ok(HttpJulieService::new(Arc::clone(&dependencies))),
        )
        .await
        .unwrap();

        let response = post_initialize(
            server.local_addr(),
            InitializeRequestOptions {
                workspace: Some(fixture.workspace_root.path()),
                workspace_source: Some(WorkspaceStartupSource::Cli),
                version: Some("0.0.0-mismatch"),
                ..InitializeRequestOptions::default()
            },
        );

        assert!(
            response.contains(r#""code":-32603"#),
            "version mismatch must reject initialize before dashboard session state changes: {response}"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
        assert!(
            dashboard_rx.try_recv().is_err(),
            "rejected initialize must not publish dashboard session changes"
        );

        server.shutdown().await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_http_julie_session_rejects_stale_binary_before_workspace_start() {
        let fixture = RealServiceFixture::new_with_admission(Some(SystemTime::UNIX_EPOCH), || {
            Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1))
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

        let response = post_initialize(
            server.local_addr(),
            InitializeRequestOptions {
                workspace: Some(fixture.workspace_root.path()),
                workspace_source: Some(WorkspaceStartupSource::Cli),
                version: Some(env!("CARGO_PKG_VERSION")),
                ..InitializeRequestOptions::default()
            },
        );

        assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
        assert!(
            response.contains(r#""code":-32603"#),
            "stale binary must be reported as a JSON-RPC internal error: {response}"
        );
        assert!(
            response.contains("restart"),
            "stale binary should tell the adapter to reconnect after restart: {response}"
        );
        assert!(
            fixture.lifecycle.restart_pending(),
            "stale binary should mark the daemon for restart"
        );
        let workspace_row = fixture.daemon_db.get_workspace(&workspace_id).unwrap();
        assert!(
            workspace_row
                .as_ref()
                .map(|row| row.session_count == 0)
                .unwrap_or(true),
            "stale binary must not start a workspace session, row={workspace_row:?}"
        );
        for _ in 0..100 {
            if fixture.sessions.active_count() == 0 {
                server.shutdown().await.unwrap();
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!(
            "stale-binary initialize should remove its daemon session tracker entry, active={}",
            fixture.sessions.active_count()
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_http_julie_session_uses_workspace_headers_and_cleans_up_on_delete() {
        let fixture = RealServiceFixture::new();
        let workspace_id = fixture.workspace_id();
        let dependencies = Arc::clone(&fixture.dependencies);
        let server = HttpTransportServer::bind(
            fixture.paths.clone(),
            HttpTransportConfig::default(),
            move || Ok(HttpJulieService::new(Arc::clone(&dependencies))),
        )
        .await
        .unwrap();

        let response = post_initialize(
            server.local_addr(),
            InitializeRequestOptions {
                workspace: Some(fixture.workspace_root.path()),
                workspace_source: Some(WorkspaceStartupSource::Cli),
                version: Some(env!("CARGO_PKG_VERSION")),
                ..InitializeRequestOptions::default()
            },
        );

        assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
        let session_id = response_header(&response, "mcp-session-id")
            .unwrap_or_else(|| panic!("initialize must return a session id: {response}"))
            .to_string();
        wait_for_session_count(&fixture.daemon_db, &workspace_id, 1).await;
        assert_eq!(fixture.sessions.active_count(), 1);

        let delete_response = delete_session(server.local_addr(), &session_id, None);

        assert!(
            delete_response.starts_with("HTTP/1.1 202 Accepted"),
            "{delete_response}"
        );
        wait_for_session_count(&fixture.daemon_db, &workspace_id, 0).await;
        for _ in 0..100 {
            if fixture.sessions.active_count() == 0 {
                server.shutdown().await.unwrap();
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!(
            "DELETE should remove its daemon session tracker entry, active={}",
            fixture.sessions.active_count()
        );
    }
}
