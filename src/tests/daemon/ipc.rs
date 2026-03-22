//! Tests for Unix domain socket IPC transport (daemon::ipc module).

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use crate::daemon::ipc::{IpcConnector, IpcListener};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn temp_socket_path() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.sock");
        (dir, path)
    }

    #[tokio::test]
    async fn test_unix_socket_roundtrip() {
        let (_dir, path) = temp_socket_path();

        let listener = IpcListener::bind(&path).await.unwrap();

        // Spawn a task that accepts one connection and echoes back "world"
        let accept_path = path.clone();
        let server = tokio::spawn(async move {
            let _ = accept_path; // keep ownership clear
            let mut stream = listener.accept().await.unwrap();
            let mut buf = [0u8; 5];
            stream.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, b"hello");
            stream.write_all(b"world").await.unwrap();
        });

        // Connect from client side
        let mut client = IpcConnector::connect(&path).await.unwrap();
        client.write_all(b"hello").await.unwrap();

        let mut buf = [0u8; 5];
        client.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"world");

        server.await.unwrap();
    }

    #[tokio::test]
    async fn test_cleanup_removes_socket() {
        let (_dir, path) = temp_socket_path();

        let listener = IpcListener::bind(&path).await.unwrap();
        assert!(path.exists(), "Socket file should exist after bind");

        listener.cleanup();
        assert!(
            !path.exists(),
            "Socket file should be removed after cleanup"
        );
    }

    #[tokio::test]
    async fn test_connect_to_nonexistent_socket_fails() {
        let (_dir, path) = temp_socket_path();
        // Don't bind anything, just try to connect
        let result = IpcConnector::connect(&path).await;
        assert!(
            result.is_err(),
            "Connecting to non-existent socket should fail"
        );
    }

    #[tokio::test]
    async fn test_bind_removes_stale_socket() {
        let (_dir, path) = temp_socket_path();

        // Create a regular file at the socket path (simulating a stale socket)
        std::fs::write(&path, "stale").unwrap();
        assert!(path.exists());

        // bind should succeed by removing the stale file first
        let listener = IpcListener::bind(&path).await.unwrap();
        assert!(path.exists(), "Socket file should exist after bind");

        listener.cleanup();
    }

    #[tokio::test]
    async fn test_multiple_connections() {
        let (_dir, path) = temp_socket_path();

        let listener = IpcListener::bind(&path).await.unwrap();

        // Spawn server that accepts 3 connections, each echoing back a unique response
        let server = tokio::spawn(async move {
            for i in 0u8..3 {
                let mut stream = listener.accept().await.unwrap();
                let mut buf = [0u8; 1];
                stream.read_exact(&mut buf).await.unwrap();
                assert_eq!(buf[0], i, "Server should receive client index");
                stream.write_all(&[i + 10]).await.unwrap();
            }
        });

        // Connect 3 clients sequentially
        for i in 0u8..3 {
            let mut client = IpcConnector::connect(&path).await.unwrap();
            client.write_all(&[i]).await.unwrap();

            let mut buf = [0u8; 1];
            client.read_exact(&mut buf).await.unwrap();
            assert_eq!(buf[0], i + 10, "Client should receive server response");
        }

        server.await.unwrap();
    }

    #[tokio::test]
    async fn test_path_accessor() {
        let (_dir, path) = temp_socket_path();
        let listener = IpcListener::bind(&path).await.unwrap();
        assert_eq!(listener.path(), path.as_path());
        listener.cleanup();
    }
}
