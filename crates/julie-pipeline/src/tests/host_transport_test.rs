//! Tests for the cross-platform IPC transport seam (Phase 3b, Task 2).
//!
//! Unix domain sockets are the proven path. A throwaway tokio echo server
//! exercises the blocking client against the async server end-to-end. The
//! Windows named-pipe arms are compile-guarded and not exercised here.

#[cfg(all(test, unix))]
mod unix {
    use crate::embeddings::host_transport::{HostAddress, HostClientConn, HostListener};
    use julie_core::paths::DaemonPaths;
    use std::path::PathBuf;
    use std::time::Duration;

    fn temp_address() -> (tempfile::TempDir, HostAddress) {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        let addr = HostAddress::from_paths(&paths);
        (dir, addr)
    }

    #[tokio::test]
    async fn client_round_trips_one_line_through_async_server() {
        let (_dir, addr) = temp_address();

        // Async echo server: read a line, echo it back uppercased.
        let listener = HostListener::bind(&addr).await.expect("bind");
        let server = tokio::spawn(async move {
            let mut conn = listener.accept().await.expect("accept");
            let line = conn.read_line().await.expect("read").expect("not eof");
            conn.write_line(&line.to_uppercase()).await.expect("write");
            // Read EOF so the client's response is fully flushed before drop.
            let _ = conn.read_line().await;
        });

        // Blocking client runs on a blocking thread (mirrors real call sites).
        let reply = tokio::task::spawn_blocking(move || {
            let mut client = HostClientConn::connect(&addr).expect("connect");
            client.round_trip("hello host")
        })
        .await
        .expect("join")
        .expect("round_trip");

        assert_eq!(reply, "HELLO HOST");
        server.await.expect("server task");
    }

    #[tokio::test]
    async fn connect_fails_when_no_host_is_listening() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = DaemonPaths::with_home(dir.path().to_path_buf());
        let addr = HostAddress::from_paths(&paths);
        let err = tokio::task::spawn_blocking(move || HostClientConn::connect(&addr))
            .await
            .expect("join");
        assert!(err.is_err(), "connecting to an absent host must error");
    }

    #[tokio::test]
    async fn unix_socket_file_is_removed_when_listener_drops() {
        let (_dir, addr) = temp_address();
        let socket_path: PathBuf = addr.socket_path().to_path_buf();
        {
            let _listener = HostListener::bind(&addr).await.expect("bind");
            assert!(socket_path.exists(), "socket should exist while bound");
        }
        assert!(
            !socket_path.exists(),
            "socket file should be removed on listener drop"
        );
    }

    /// `connect_with_timeout` completes normally when the server responds within
    /// the deadline.
    #[tokio::test]
    async fn connect_with_timeout_succeeds_when_server_responds_promptly() {
        let (_dir, addr) = temp_address();

        let listener = HostListener::bind(&addr).await.expect("bind");
        let server = tokio::spawn(async move {
            let mut conn = listener.accept().await.expect("accept");
            let line = conn.read_line().await.expect("read").expect("not eof");
            conn.write_line(&line.to_uppercase()).await.expect("write");
            let _ = conn.read_line().await;
        });

        // Generous 5 s timeout — server responds immediately.
        let reply = tokio::task::spawn_blocking(move || {
            let mut client =
                HostClientConn::connect_with_timeout(&addr, Duration::from_secs(5))
                    .expect("connect");
            client.round_trip("ping")
        })
        .await
        .expect("join")
        .expect("round_trip");

        assert_eq!(reply, "PING");
        server.await.expect("server task");
    }

    /// `round_trip` returns an error when the server accepts but stalls longer
    /// than the configured read timeout.
    #[tokio::test]
    async fn connect_with_timeout_read_times_out_on_stalled_server() {
        let (_dir, addr) = temp_address();

        let listener = HostListener::bind(&addr).await.expect("bind");
        let server = tokio::spawn(async move {
            let mut conn = listener.accept().await.expect("accept");
            // Consume the request but stall the response beyond the client timeout.
            let _req = conn.read_line().await.expect("read");
            tokio::time::sleep(Duration::from_millis(400)).await;
            let _ = conn.write_line("too late").await;
        });

        // 50 ms read timeout — the server stalls 400 ms, so round_trip must error.
        let result = tokio::task::spawn_blocking(move || {
            let mut client =
                HostClientConn::connect_with_timeout(&addr, Duration::from_millis(50))
                    .expect("connect");
            client.round_trip("hello")
        })
        .await
        .expect("join");

        assert!(
            result.is_err(),
            "expected a timeout error from the stalled server, got Ok"
        );

        server.await.expect("server task");
    }
}
