//! Tests for the transport contract that wraps IPC platform details.

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use crate::daemon::transport::{TransportEndpoint, TransportProbe};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn temp_socket_path() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("transport.sock");
        (dir, path)
    }

    #[tokio::test]
    async fn test_transport_endpoint_round_trip() {
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
    fn test_transport_probe_reports_ready_for_live_socket() {
        let (_dir, path) = temp_socket_path();
        let _listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let endpoint = TransportEndpoint::new(path);
        assert_eq!(endpoint.probe_readiness(), TransportProbe::Ready);
    }

    #[test]
    fn test_transport_probe_rejects_stale_socket_file() {
        let (_dir, path) = temp_socket_path();
        {
            let _listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        }

        let endpoint = TransportEndpoint::new(path);
        assert_eq!(endpoint.probe_readiness(), TransportProbe::NotReady);
    }

    #[test]
    fn test_transport_wait_for_readiness_times_out_when_endpoint_is_unavailable() {
        let (_dir, path) = temp_socket_path();
        let endpoint = TransportEndpoint::new(path);
        let result = endpoint.wait_for_readiness(std::time::Duration::from_millis(200));
        assert!(result.is_err());
    }
}
