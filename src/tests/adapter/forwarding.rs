//! Tests for adapter byte-forwarding edge cases.

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use crate::adapter::{ForwardOutcome, forward_streams};

    #[tokio::test]
    async fn test_forward_streams_reports_immediate_daemon_disconnect() {
        let (client_stream, daemon_stream) = tokio::io::duplex(64);
        let (mut stdin_reader, _stdin_writer) = tokio::io::duplex(64);
        let mut stdout = tokio::io::sink();

        drop(daemon_stream);

        let outcome = forward_streams(client_stream, &mut stdin_reader, &mut stdout)
            .await
            .expect("forwarding should classify clean EOF");

        assert_eq!(outcome, ForwardOutcome::ImmediateDaemonDisconnect);
    }

    #[tokio::test]
    async fn test_forward_streams_reports_normal_session_end_after_io() {
        let (client_stream, mut daemon_stream) = tokio::io::duplex(64);
        let (mut stdin_reader, mut stdin_writer) = tokio::io::duplex(64);
        let mut stdout = tokio::io::sink();

        let daemon = tokio::spawn(async move {
            let mut buf = [0u8; 5];
            daemon_stream
                .read_exact(&mut buf)
                .await
                .expect("daemon should receive request bytes");
            assert_eq!(&buf, b"hello");
            daemon_stream
                .write_all(b"world")
                .await
                .expect("daemon should send response bytes");
            daemon_stream
                .shutdown()
                .await
                .expect("daemon should close cleanly");
        });

        let stdin_feeder = tokio::spawn(async move {
            stdin_writer
                .write_all(b"hello")
                .await
                .expect("stdin writer should send bytes");
            stdin_writer
                .shutdown()
                .await
                .expect("stdin writer should close cleanly");
        });

        let outcome = forward_streams(client_stream, &mut stdin_reader, &mut stdout)
            .await
            .expect("forwarding should complete normally");

        stdin_feeder.await.expect("stdin feeder should finish");
        daemon.await.expect("daemon task should finish");

        assert_eq!(outcome, ForwardOutcome::SessionEnded);
    }
}
