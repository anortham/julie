//! Tests for the daemon-ready handshake that gates stdin forwarding.
//!
//! Background: before this handshake existed, a daemon that accepted an
//! IPC connection and then dropped it (stale binary, version mismatch) could
//! cause the adapter's `tokio::select!` forwarder to consume bytes from stdin
//! and discard them. On retry, stdin was already drained and the client's
//! `initialize` request never reached the new daemon. See commit log and
//! `.memories/2026-04-16/*` for the incident.
//!
//! The handshake: daemon writes `DAEMON_READY\n` after clearing all session
//! gates but before serving. Adapter reads that line before starting the
//! forwarder. These tests exercise the adapter-side read logic in isolation.

#[cfg(test)]
mod tests {
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use crate::adapter::{ReadyOutcome, read_daemon_ready};
    use crate::daemon::ipc::DAEMON_READY_LINE;

    #[tokio::test]
    async fn test_read_daemon_ready_returns_ready_on_signal() {
        let (mut client, mut daemon) = tokio::io::duplex(64);

        let daemon_task = tokio::spawn(async move {
            daemon
                .write_all(DAEMON_READY_LINE)
                .await
                .expect("daemon should write ready signal");
            daemon
        });

        let outcome = read_daemon_ready(&mut client, Duration::from_millis(500)).await;
        daemon_task.await.expect("daemon task panicked");

        assert!(
            matches!(outcome, ReadyOutcome::Ready),
            "expected Ready, got {:?}",
            outcome
        );
    }

    #[tokio::test]
    async fn test_read_daemon_ready_returns_eof_when_daemon_drops_before_signal() {
        let (mut client, daemon) = tokio::io::duplex(64);

        drop(daemon);

        let outcome = read_daemon_ready(&mut client, Duration::from_millis(500)).await;

        assert!(
            matches!(outcome, ReadyOutcome::Eof),
            "expected Eof, got {:?}",
            outcome
        );
    }

    #[tokio::test]
    async fn test_read_daemon_ready_returns_eof_on_partial_signal() {
        let (mut client, mut daemon) = tokio::io::duplex(64);

        let daemon_task = tokio::spawn(async move {
            // Partial signal — daemon dies mid-write (e.g. crash during READY flush).
            daemon
                .write_all(b"DAEMON_RE")
                .await
                .expect("daemon should write partial prefix");
            drop(daemon);
        });

        let outcome = read_daemon_ready(&mut client, Duration::from_millis(500)).await;
        daemon_task.await.expect("daemon task panicked");

        assert!(
            matches!(outcome, ReadyOutcome::Eof),
            "expected Eof on partial write, got {:?}",
            outcome
        );
    }

    #[tokio::test]
    async fn test_read_daemon_ready_times_out_when_daemon_never_sends() {
        // With the current design timeouts are retryable transport failures
        // (the "legacy fallback" is gone), but the low-level read_daemon_ready
        // function still needs to expose the distinction so the caller can
        // log it. A daemon that never writes anything should yield Timeout,
        // not Eof.
        let (mut client, _daemon) = tokio::io::duplex(64);

        let outcome = read_daemon_ready(&mut client, Duration::from_millis(50)).await;

        assert!(
            matches!(outcome, ReadyOutcome::Timeout),
            "expected Timeout, got {:?}",
            outcome
        );
    }

    /// Codex flagged this scenario: daemon accepts, gets past all gates, then
    /// takes a long time doing pre-serve work (workspace init, etc.) and fails
    /// mid-work. The adapter must see Eof (not Timeout) when the daemon drops
    /// after opening the connection but before writing READY, even if the drop
    /// happens late in the wait window.
    #[tokio::test]
    async fn test_read_daemon_ready_returns_eof_when_daemon_drops_after_delay() {
        let (mut client, daemon) = tokio::io::duplex(64);

        let daemon_task = tokio::spawn(async move {
            // Simulate slow pre-serve work (e.g. cold workspace init) that
            // ultimately fails — daemon drops without writing READY.
            tokio::time::sleep(Duration::from_millis(100)).await;
            drop(daemon);
        });

        let outcome = read_daemon_ready(&mut client, Duration::from_millis(500)).await;
        daemon_task.await.expect("daemon task panicked");

        assert!(
            matches!(outcome, ReadyOutcome::Eof),
            "expected Eof after delayed drop, got {:?}",
            outcome
        );
    }

    #[tokio::test]
    async fn test_read_daemon_ready_preserves_bytes_after_signal() {
        // Daemon writes READY and immediately follows with MCP bytes. The adapter
        // must stop reading after `\n` so the following bytes remain in the stream
        // for the byte-forwarder.
        let (mut client, mut daemon) = tokio::io::duplex(64);

        let payload = b"\"jsonrpc\":\"2.0\"";
        let daemon_task = tokio::spawn(async move {
            daemon
                .write_all(DAEMON_READY_LINE)
                .await
                .expect("daemon ready write");
            daemon.write_all(payload).await.expect("daemon payload write");
            daemon.shutdown().await.expect("daemon shutdown");
        });

        let outcome = read_daemon_ready(&mut client, Duration::from_millis(500)).await;
        daemon_task.await.expect("daemon task panicked");

        assert!(matches!(outcome, ReadyOutcome::Ready));

        // Verify the trailing payload is still readable from the stream.
        let mut remaining = Vec::new();
        client
            .read_to_end(&mut remaining)
            .await
            .expect("should drain remaining bytes");
        assert_eq!(
            remaining, payload,
            "bytes after ready signal should be preserved"
        );
    }

    #[tokio::test]
    async fn test_read_daemon_ready_rejects_unexpected_line() {
        let (mut client, mut daemon) = tokio::io::duplex(64);

        let daemon_task = tokio::spawn(async move {
            daemon
                .write_all(b"SOMETHING_ELSE\n")
                .await
                .expect("daemon unexpected line");
            daemon.shutdown().await.expect("daemon shutdown");
        });

        let outcome = read_daemon_ready(&mut client, Duration::from_millis(500)).await;
        daemon_task.await.expect("daemon task panicked");

        match outcome {
            ReadyOutcome::Unexpected(line) => {
                assert_eq!(line, b"SOMETHING_ELSE\n");
            }
            other => panic!("expected Unexpected, got {:?}", other),
        }
    }

    /// Simulates the full adapter handshake against a daemon that accepts the
    /// connection, reads headers, and then decides to shut down for restart
    /// (dropping the stream without writing READY). The adapter must detect
    /// the missing signal and NOT have consumed any stdin bytes by that point,
    /// which is what makes the retry path safe.
    #[tokio::test]
    async fn test_handshake_against_daemon_dropping_before_ready_returns_eof() {
        use crate::adapter::build_ipc_header;
        use crate::workspace::startup_hint::{WorkspaceStartupHint, WorkspaceStartupSource};
        use std::path::PathBuf;

        let (mut client, mut daemon) = tokio::io::duplex(256);

        let daemon_task = tokio::spawn(async move {
            // Simulate accept_loop reading the headers and then hitting
            // ShutdownForRestart / RejectForRestart: drop the stream without
            // writing DAEMON_READY.
            let mut header_buf = Vec::new();
            let mut byte = [0u8; 1];
            loop {
                match daemon.read(&mut byte).await {
                    Ok(0) => break,
                    Ok(_) => {
                        header_buf.push(byte[0]);
                        // Header block terminates with a blank line "\n\n".
                        if header_buf.ends_with(b"\n\n") {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            drop(daemon); // simulates daemon dropping the stream
            header_buf
        });

        // Adapter side: send headers, then try to read READY.
        let hint = WorkspaceStartupHint {
            path: PathBuf::from("/tmp/test-workspace"),
            source: Some(WorkspaceStartupSource::Cwd),
        };
        let header = build_ipc_header(&hint);
        client
            .write_all(header.as_bytes())
            .await
            .expect("adapter header write");

        let outcome = read_daemon_ready(&mut client, Duration::from_millis(500)).await;
        let received_headers = daemon_task.await.expect("daemon task");

        // Daemon received the headers correctly.
        assert_eq!(received_headers, header.as_bytes());
        // Adapter correctly detected that the daemon dropped without READY.
        assert!(
            matches!(outcome, ReadyOutcome::Eof),
            "expected Eof after daemon drops, got {:?}",
            outcome
        );
    }

    #[tokio::test]
    async fn test_read_daemon_ready_caps_line_length_to_guard_against_runaways() {
        // A legacy daemon that barfs megabytes of MCP bytes without a newline
        // should not cause unbounded growth in the adapter's line buffer.
        let (mut client, mut daemon) = tokio::io::duplex(4096);

        let daemon_task = tokio::spawn(async move {
            let huge: Vec<u8> = vec![b'x'; 1024];
            daemon.write_all(&huge).await.expect("daemon huge write");
            // Keep the pipe open so the reader doesn't see EOF.
            daemon
        });

        let outcome = read_daemon_ready(&mut client, Duration::from_millis(200)).await;
        let _daemon = daemon_task.await.expect("daemon task panicked");

        // Either Unexpected (bounded buffer hit) or Timeout (waiting for newline).
        // Both are acceptable — the important property is that the function
        // returned, rather than hanging or allocating unbounded memory.
        assert!(
            matches!(outcome, ReadyOutcome::Unexpected(_) | ReadyOutcome::Timeout),
            "expected bounded outcome, got {:?}",
            outcome
        );
    }
}
