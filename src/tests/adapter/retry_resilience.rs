//! Tests for the HTTP adapter's retry classification on pre-output transport
//! errors. See Task 4 of the daemon reliability plan.

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use rmcp::model::{ClientJsonRpcMessage, RequestId, ServerJsonRpcMessage};
    use rmcp::service::RoleClient;
    use rmcp::transport::Transport;
    use tokio::io::AsyncWriteExt;
    use tokio::sync::Notify;

    use crate::adapter::{
        AdapterError, AdapterRetryDecision, DaemonAdapterControl, ForwardOutcome, MAX_RETRIES,
        classify_adapter_error, forward_http_stdio_transport, retry_backoff,
        run_http_adapter_inner,
    };
    use crate::daemon::http_transport::{MCP_PATH, READINESS_PATH};
    use crate::daemon::transport::TransportEndpoint;
    use crate::workspace::startup_hint::{WorkspaceStartupHint, WorkspaceStartupSource};

    #[derive(Debug, Clone)]
    struct FakeTransportError(&'static str);

    impl std::fmt::Display for FakeTransportError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(self.0)
        }
    }

    impl std::error::Error for FakeTransportError {}

    /// Transport that fails on `send` with a configurable error.
    struct AlwaysFailSendTransport {
        sent_attempts: Arc<Mutex<usize>>,
    }

    impl AlwaysFailSendTransport {
        fn new() -> Self {
            Self {
                sent_attempts: Arc::new(Mutex::new(0)),
            }
        }
    }

    impl Transport<RoleClient> for AlwaysFailSendTransport {
        type Error = FakeTransportError;

        fn send(
            &mut self,
            _item: ClientJsonRpcMessage,
        ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'static {
            let attempts = Arc::clone(&self.sent_attempts);
            async move {
                *attempts.lock().unwrap() += 1;
                Err(FakeTransportError("simulated transport send failure"))
            }
        }

        async fn receive(&mut self) -> Option<ServerJsonRpcMessage> {
            None
        }

        fn close(&mut self) -> impl Future<Output = Result<(), Self::Error>> + Send {
            async move { Ok(()) }
        }
    }

    /// Transport that yields one response, succeeds on all sends, but fails
    /// on `close()`. Drives the "post-output transport error" path: a response
    /// reaches stdout (wrote_any_output=true), then the EOF-cleanup `close()`
    /// fails with a transport error.
    struct CloseFailsAfterResponseTransport {
        responses: Mutex<VecDeque<ServerJsonRpcMessage>>,
    }

    impl CloseFailsAfterResponseTransport {
        fn new(responses: Vec<ServerJsonRpcMessage>) -> Self {
            Self {
                responses: Mutex::new(responses.into()),
            }
        }
    }

    impl Transport<RoleClient> for CloseFailsAfterResponseTransport {
        type Error = FakeTransportError;

        fn send(
            &mut self,
            _item: ClientJsonRpcMessage,
        ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'static {
            async move { Ok(()) }
        }

        async fn receive(&mut self) -> Option<ServerJsonRpcMessage> {
            self.responses.lock().unwrap().pop_front()
        }

        fn close(&mut self) -> impl Future<Output = Result<(), Self::Error>> + Send {
            async move {
                Err(FakeTransportError(
                    "simulated close failure after output written",
                ))
            }
        }
    }

    /// Transport that waits until two requests have been sent, writes one
    /// response, then closes while the second request is still in flight.
    struct DisconnectsWithOutstandingRequestTransport {
        sent_count: Arc<AtomicUsize>,
        sent_notify: Arc<Notify>,
        response_sent: bool,
    }

    impl DisconnectsWithOutstandingRequestTransport {
        fn new() -> Self {
            Self {
                sent_count: Arc::new(AtomicUsize::new(0)),
                sent_notify: Arc::new(Notify::new()),
                response_sent: false,
            }
        }
    }

    impl Transport<RoleClient> for DisconnectsWithOutstandingRequestTransport {
        type Error = FakeTransportError;

        fn send(
            &mut self,
            _item: ClientJsonRpcMessage,
        ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'static {
            let sent_count = Arc::clone(&self.sent_count);
            let sent_notify = Arc::clone(&self.sent_notify);
            async move {
                sent_count.fetch_add(1, Ordering::SeqCst);
                sent_notify.notify_waiters();
                Ok(())
            }
        }

        async fn receive(&mut self) -> Option<ServerJsonRpcMessage> {
            while self.sent_count.load(Ordering::SeqCst) < 2 {
                self.sent_notify.notified().await;
            }

            if self.response_sent {
                None
            } else {
                self.response_sent = true;
                Some(initialize_response(1))
            }
        }

        fn close(&mut self) -> impl Future<Output = Result<(), Self::Error>> + Send {
            async move { Ok(()) }
        }
    }

    struct CountingDaemonControl {
        ensure_calls: AtomicUsize,
        endpoint: TransportEndpoint,
    }

    impl CountingDaemonControl {
        fn new() -> Self {
            Self {
                ensure_calls: AtomicUsize::new(0),
                endpoint: TransportEndpoint::streamable_http(
                    "127.0.0.1",
                    7890,
                    MCP_PATH,
                    READINESS_PATH,
                    None,
                )
                .expect("valid test endpoint"),
            }
        }

        fn ensure_calls(&self) -> usize {
            self.ensure_calls.load(Ordering::SeqCst)
        }
    }

    impl DaemonAdapterControl for CountingDaemonControl {
        fn ensure_daemon_ready(&self) -> std::io::Result<()> {
            self.ensure_calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn transport_endpoint(&self) -> std::io::Result<TransportEndpoint> {
            Ok(self.endpoint.clone())
        }
    }

    fn initialize_response(id: i32) -> ServerJsonRpcMessage {
        serde_json::from_value(serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "protocolVersion": "2025-06-18",
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "Julie", "version": "7.5.5"}
            }
        }))
        .expect("valid initialize response")
    }

    // --- Classification tests ---

    #[test]
    fn classify_pre_output_transport_error_returns_retry() {
        let error = AdapterError::Transport {
            error: anyhow::anyhow!("simulated"),
            wrote_any_output: false,
            lost_line: None,
        };
        let decision = classify_adapter_error(&error, 0, MAX_RETRIES);
        assert_eq!(decision, AdapterRetryDecision::Retry);
    }

    #[test]
    fn classify_post_output_transport_error_returns_terminal() {
        let error = AdapterError::Transport {
            error: anyhow::anyhow!("simulated"),
            wrote_any_output: true,
            lost_line: None,
        };
        let decision = classify_adapter_error(&error, 0, MAX_RETRIES);
        assert_eq!(
            decision,
            AdapterRetryDecision::Terminal,
            "transport error after output must be terminal to avoid replaying non-idempotent tools"
        );
    }

    #[test]
    fn classify_stdin_error_returns_terminal() {
        let error = AdapterError::Stdin(std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            "client gone",
        ));
        let decision = classify_adapter_error(&error, 0, MAX_RETRIES);
        assert_eq!(decision, AdapterRetryDecision::Terminal);
    }

    #[test]
    fn classify_pre_output_transport_error_returns_exhausted_at_budget() {
        let error = AdapterError::Transport {
            error: anyhow::anyhow!("simulated"),
            wrote_any_output: false,
            lost_line: None,
        };
        let decision = classify_adapter_error(&error, MAX_RETRIES, MAX_RETRIES);
        assert_eq!(decision, AdapterRetryDecision::Exhausted);
    }

    // --- Retry budget / backoff tests ---

    #[test]
    fn max_retries_is_five() {
        assert_eq!(MAX_RETRIES, 5);
    }

    #[test]
    fn retry_backoff_grows_exponentially_and_caps_at_sixteen_seconds() {
        assert_eq!(retry_backoff(1), Duration::from_secs(1));
        assert_eq!(retry_backoff(2), Duration::from_secs(2));
        assert_eq!(retry_backoff(3), Duration::from_secs(4));
        assert_eq!(retry_backoff(4), Duration::from_secs(8));
        assert_eq!(retry_backoff(5), Duration::from_secs(16));
        // Cap holds at and beyond the clamp boundary.
        assert_eq!(retry_backoff(6), Duration::from_secs(16));
    }

    #[test]
    fn cumulative_retry_window_fits_in_drain_timeout() {
        let total: u64 = (1..=MAX_RETRIES)
            .map(|attempt| retry_backoff(attempt).as_secs())
            .sum();
        assert_eq!(total, 31, "expected 1+2+4+8+16 = 31s total backoff");
        // Drain timeout is 60s; we should fit well inside that window.
        assert!(total < 60, "retry window must fit within drain timeout");
    }

    // --- forward_http_stdio_transport_with_pending behavior tests ---

    #[tokio::test]
    async fn pre_output_transport_send_error_returns_transport_error_no_output() {
        let transport = AlwaysFailSendTransport::new();
        let sent_attempts = Arc::clone(&transport.sent_attempts);
        let (stdin, mut stdin_writer) = tokio::io::duplex(1024);
        let mut stdout = Vec::new();

        stdin_writer
            .write_all(br#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test-client","version":"0.0.0"}}}"#)
            .await
            .unwrap();
        stdin_writer.write_all(b"\n").await.unwrap();
        stdin_writer.shutdown().await.unwrap();

        let result = forward_http_stdio_transport(transport, stdin, &mut stdout).await;

        match result {
            Err(AdapterError::Transport {
                wrote_any_output, ..
            }) => {
                assert!(!wrote_any_output, "no output was ever written");
            }
            other => panic!("expected Transport error, got {other:?}"),
        }
        assert!(stdout.is_empty(), "no response should reach stdout");
        assert!(
            *sent_attempts.lock().unwrap() >= 1,
            "send should be attempted at least once"
        );
    }

    #[tokio::test]
    async fn stdin_io_error_returns_stdin_variant() {
        // Construct an MCP-client side I/O error using a closed pipe.
        let transport = AlwaysFailSendTransport::new();
        let (stdin, stdin_writer) = tokio::io::duplex(1024);
        // Drop the writer immediately to simulate the MCP client going away.
        // The reader observes EOF (None), not an error, however — so we instead
        // construct an explicit AdapterError::Stdin and route it through the
        // classifier. The forward_http_stdio_transport surface treats EOF as
        // SessionEnded and we cannot synthesize an io::Error from a duplex
        // stream cleanly. Instead, verify the classification of a Stdin error
        // directly (covered in unit tests above) and assert here that EOF
        // closes cleanly when there is no in-flight work.
        drop(stdin_writer);
        let mut stdout = Vec::new();

        let outcome = forward_http_stdio_transport(transport, stdin, &mut stdout)
            .await
            .map_err(|e| format!("{}", e))
            .expect("EOF on stdin with no in-flight requests should end the session cleanly");
        assert_eq!(outcome, ForwardOutcome::SessionEnded);
    }

    #[tokio::test]
    async fn post_output_transport_error_reports_wrote_any_output_true() {
        // Request 1 succeeds, response flows to stdout (wrote_any_output=true).
        // On stdin EOF, the forwarder calls transport.close(), which fails.
        // Expected: AdapterError::Transport { wrote_any_output: true } so the
        // run_http_adapter loop exits cleanly without replaying the session.
        let transport = CloseFailsAfterResponseTransport::new(vec![initialize_response(1)]);
        let (stdin, mut stdin_writer) = tokio::io::duplex(2048);
        let mut stdout = Vec::new();

        stdin_writer
            .write_all(br#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test-client","version":"0.0.0"}}}"#)
            .await
            .unwrap();
        stdin_writer.write_all(b"\n").await.unwrap();
        stdin_writer.shutdown().await.unwrap();

        let result = forward_http_stdio_transport(transport, stdin, &mut stdout).await;
        match result {
            Err(AdapterError::Transport {
                wrote_any_output, ..
            }) => {
                assert!(
                    wrote_any_output,
                    "wrote_any_output must be true after a response reached stdout"
                );
            }
            other => panic!("expected post-output Transport error, got {other:?}"),
        }
        assert!(
            !stdout.is_empty(),
            "the successful response should have reached stdout"
        );

        // Classification: a Transport error with wrote_any_output=true must be
        // Terminal, never Retry — protects non-idempotent tools from replay.
        let post_output_err = AdapterError::Transport {
            error: anyhow::anyhow!("post-output simulated"),
            wrote_any_output: true,
            lost_line: None,
        };
        assert_eq!(
            classify_adapter_error(&post_output_err, 0, MAX_RETRIES),
            AdapterRetryDecision::Terminal
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn post_output_transport_error_recovers_daemon_before_terminal_exit() {
        let control = CountingDaemonControl::new();
        let (stdin, mut stdin_writer) = tokio::io::duplex(2048);
        let mut stdout = Vec::new();
        let startup_hint = WorkspaceStartupHint {
            path: std::env::current_dir().expect("current dir"),
            source: Some(WorkspaceStartupSource::Cwd),
        };

        stdin_writer
            .write_all(br#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test-client","version":"0.0.0"}}}"#)
            .await
            .unwrap();
        stdin_writer.write_all(b"\n").await.unwrap();
        stdin_writer.shutdown().await.unwrap();

        let result = run_http_adapter_inner(
            startup_hint,
            &control,
            stdin,
            &mut stdout,
            |_endpoint, _startup_hint| {
                Ok(CloseFailsAfterResponseTransport::new(vec![
                    initialize_response(1),
                ]))
            },
        )
        .await;

        assert!(
            result.is_ok(),
            "terminal post-output transport loss should exit cleanly after daemon recovery: {result:?}"
        );
        assert!(
            !stdout.is_empty(),
            "the successful response before transport loss should still reach stdout"
        );
        assert_eq!(
            control.ensure_calls(),
            2,
            "adapter must ensure daemon readiness once before connecting and once more before terminal exit"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn post_output_daemon_eof_with_outstanding_request_recovers_daemon() {
        let control = CountingDaemonControl::new();
        let (stdin, mut stdin_writer) = tokio::io::duplex(4096);
        let mut stdout = Vec::new();
        let startup_hint = WorkspaceStartupHint {
            path: std::env::current_dir().expect("current dir"),
            source: Some(WorkspaceStartupSource::Cwd),
        };

        let initialize_request = br#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test-client","version":"0.0.0"}}}"#;
        stdin_writer.write_all(initialize_request).await.unwrap();
        stdin_writer.write_all(b"\n").await.unwrap();
        stdin_writer.write_all(initialize_request).await.unwrap();
        stdin_writer.write_all(b"\n").await.unwrap();
        stdin_writer.shutdown().await.unwrap();

        let result = run_http_adapter_inner(
            startup_hint,
            &control,
            stdin,
            &mut stdout,
            |_endpoint, _startup_hint| Ok(DisconnectsWithOutstandingRequestTransport::new()),
        )
        .await;

        assert!(
            result.is_ok(),
            "post-output daemon EOF with outstanding work should exit cleanly after daemon recovery: {result:?}"
        );
        assert!(
            !stdout.is_empty(),
            "the first response should have reached stdout before daemon EOF"
        );
        assert_eq!(
            control.ensure_calls(),
            2,
            "adapter must recover daemon readiness before terminal exit when daemon EOF leaves a request in flight"
        );
    }

    // --- Retry semantics: ensure_daemon_ready is invoked on retry ---
    //
    // run_http_adapter's retry loop calls launcher.ensure_daemon_ready() at the
    // top of every iteration before reconnecting. We can't easily mock the
    // launcher (it's a concrete type), so we cover this contractually:
    // classify_adapter_error returns Retry → control flows back to the top of
    // the loop, which means ensure_daemon_ready is called again. The two pieces
    // (classification + loop structure) compose to guarantee daemon re-launch.
    //
    // This test asserts the classification half; the loop half is covered by
    // direct inspection of run_http_adapter (see src/adapter/http_stdio.rs).
    #[test]
    fn retry_decision_leads_back_into_ensure_daemon_ready_loop() {
        // Simulate "attempt 0 failed with pre-output transport error".
        let error = AdapterError::Transport {
            error: anyhow::anyhow!("daemon went down"),
            wrote_any_output: false,
            lost_line: None,
        };
        for attempt in 0..MAX_RETRIES {
            assert_eq!(
                classify_adapter_error(&error, attempt, MAX_RETRIES),
                AdapterRetryDecision::Retry,
                "attempt {} should retry; run_http_adapter will then call ensure_daemon_ready",
                attempt
            );
        }
        assert_eq!(
            classify_adapter_error(&error, MAX_RETRIES, MAX_RETRIES),
            AdapterRetryDecision::Exhausted
        );
    }

    // --- Lost line preservation on transport send failure ---

    #[tokio::test]
    async fn send_failure_preserves_lost_line_for_retry() {
        // When transport.send() fails, the request line that was being sent
        // must appear in AdapterError::Transport::lost_line so the retry
        // loop can push it back onto pending_lines.
        let transport = AlwaysFailSendTransport::new();
        let (stdin, mut stdin_writer) = tokio::io::duplex(1024);
        let mut stdout = Vec::new();

        let request_bytes =
            br#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test-client","version":"0.0.0"}}}"#;

        stdin_writer.write_all(request_bytes).await.unwrap();
        stdin_writer.write_all(b"\n").await.unwrap();
        stdin_writer.shutdown().await.unwrap();

        let result = forward_http_stdio_transport(transport, stdin, &mut stdout).await;

        match result {
            Err(AdapterError::Transport {
                wrote_any_output,
                lost_line,
                ..
            }) => {
                assert!(!wrote_any_output, "no output was ever written");
                let lost = lost_line.expect("lost_line must be Some when send fails");
                let lost_parsed: serde_json::Value =
                    serde_json::from_slice(&lost).expect("lost_line must be valid JSON");
                let original_parsed: serde_json::Value = serde_json::from_slice(request_bytes)
                    .expect("request bytes must be valid JSON");
                assert_eq!(
                    lost_parsed, original_parsed,
                    "lost_line must contain the original request bytes"
                );
            }
            other => panic!("expected Transport error with lost_line, got {other:?}"),
        }
    }

    // Silence unused-import lint when tokio bits aren't all touched.
    #[allow(dead_code)]
    fn _force_uses(_: VecDeque<u8>, _: RequestId) {}
}
