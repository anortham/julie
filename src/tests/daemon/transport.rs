//! Tests for the transport contract that wraps daemon transport platform details.

#[cfg(test)]
mod tests {
    use crate::daemon::transport::{TransportEndpoint, TransportMode, TransportProbe};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::Path;
    use std::thread::{self, JoinHandle};

    #[cfg(unix)]
    fn temp_socket_path() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("transport.sock");
        (dir, path)
    }

    fn write_token(path: &Path, token: &str) {
        std::fs::write(path, token).unwrap();
    }

    fn spawn_http_readiness_server(
        listener: TcpListener,
        expected_token: &'static str,
    ) -> JoinHandle<()> {
        thread::spawn(move || {
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
            assert!(request.contains(&format!("Authorization: Bearer {}", expected_token)));
            stream
                .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")
                .unwrap();
        })
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn test_transport_endpoint_round_trip() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let (_dir, path) = temp_socket_path();
        let endpoint = TransportEndpoint::new(path.clone());
        let listener = endpoint.bind_listener().await.unwrap();

        let server = tokio::spawn(async move {
            let mut stream = listener.accept().await.unwrap();
            let mut buf = [0u8; 4];
            stream.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, b"ping");
            stream.write_all(b"pong").await.unwrap();
        });

        let mut client = endpoint.connect().await.unwrap();
        client.write_all(b"ping").await.unwrap();

        let mut buf = [0u8; 4];
        client.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"pong");

        server.await.unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn test_transport_probe_reports_ready_for_live_socket() {
        let (_dir, path) = temp_socket_path();
        let _listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let endpoint = TransportEndpoint::new(path);
        assert_eq!(endpoint.probe_readiness(), TransportProbe::Ready);
    }

    #[test]
    #[cfg(unix)]
    fn test_transport_probe_rejects_stale_socket_file() {
        let (_dir, path) = temp_socket_path();
        {
            let _listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        }

        let endpoint = TransportEndpoint::new(path);
        assert_eq!(endpoint.probe_readiness(), TransportProbe::NotReady);
    }

    #[test]
    #[cfg(unix)]
    fn test_transport_wait_for_readiness_times_out_when_endpoint_is_unavailable() {
        let (_dir, path) = temp_socket_path();
        let endpoint = TransportEndpoint::new(path);
        let result = endpoint.wait_for_readiness(std::time::Duration::from_millis(200));
        assert!(result.is_err());
    }

    #[test]
    fn test_transport_discovery_round_trips_streamable_http_without_token_value() {
        let dir = tempfile::tempdir().unwrap();
        let token_path = dir.path().join("daemon-mcp.token");
        let state_path = dir.path().join("daemon-mcp-transport.json");
        write_token(&token_path, "super-secret-token\n");

        let endpoint = TransportEndpoint::streamable_http(
            "127.0.0.1",
            41337,
            "/mcp",
            "/mcp/ready",
            Some(token_path.clone()),
        )
        .unwrap();

        endpoint.publish_discovery(&state_path).unwrap();

        let serialized = std::fs::read_to_string(&state_path).unwrap();
        assert!(serialized.contains("\"mode\":\"streamable_http\""));
        assert!(serialized.contains("daemon-mcp.token"));
        assert!(
            !serialized.contains("super-secret-token"),
            "transport discovery must point at auth material, not copy bearer token values"
        );

        let discovered = TransportEndpoint::read_discovery(&state_path).unwrap();
        assert_eq!(discovered.mode(), TransportMode::StreamableHttp);
        assert_eq!(discovered.mcp_url().unwrap(), "http://127.0.0.1:41337/mcp");
        assert_eq!(discovered.token_path(), Some(token_path.as_path()));
    }

    #[test]
    fn test_transport_probe_reports_ready_for_live_http_endpoint() {
        let dir = tempfile::tempdir().unwrap();
        let token_path = dir.path().join("daemon-mcp.token");
        write_token(&token_path, "probe-token\n");

        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = spawn_http_readiness_server(listener, "probe-token");

        let endpoint = TransportEndpoint::streamable_http(
            "127.0.0.1",
            port,
            "/mcp",
            "/mcp/ready",
            Some(token_path),
        )
        .unwrap();

        assert_eq!(endpoint.probe_readiness(), TransportProbe::Ready);
        server.join().unwrap();
    }

    #[test]
    fn test_transport_probe_rejects_stale_http_endpoint_state() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let endpoint =
            TransportEndpoint::streamable_http("127.0.0.1", port, "/mcp", "/mcp/ready", None)
                .unwrap();

        assert_eq!(endpoint.probe_readiness(), TransportProbe::NotReady);
    }
}
