//! Tests for the daemon Streamable HTTP MCP transport module.

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};
    use std::net::{SocketAddr, TcpStream};
    use std::time::Duration;
    use std::{io::Read, io::Write};

    use rmcp::ServerHandler;

    use crate::daemon::http_transport::{
        HttpTransportConfig, HttpTransportServer, MCP_PATH, READINESS_PATH,
    };
    use crate::daemon::transport::{TransportEndpoint, TransportMode, TransportProbe};
    use crate::paths::DaemonPaths;

    #[derive(Clone)]
    struct TestMcpHandler;

    impl ServerHandler for TestMcpHandler {}

    #[derive(Default)]
    struct InitializeRequestOptions<'a> {
        host: Option<String>,
        origin: Option<&'a str>,
        bearer_token: Option<&'a str>,
    }

    fn post_initialize(addr: SocketAddr, options: InitializeRequestOptions<'_>) -> String {
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
        request.push_str("\r\n");
        request.push_str(body);

        let mut stream = TcpStream::connect(addr).unwrap();
        stream.write_all(request.as_bytes()).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
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
}
