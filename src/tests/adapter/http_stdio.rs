#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    use axum::http::HeaderName;
    use rmcp::model::{
        ClientJsonRpcMessage, JsonRpcMessage, RequestId, ServerJsonRpcMessage, ServerRequest,
        ServerResult,
    };
    use rmcp::service::RoleClient;
    use rmcp::transport::Transport;
    use tokio::io::AsyncWriteExt;
    use tokio::sync::mpsc;
    use tokio::time::{Duration, timeout};

    use crate::adapter::{ForwardOutcome, forward_http_stdio_transport};
    use crate::daemon::http_client::http_client_config_for_endpoint;
    use crate::daemon::mcp_session::{
        HEADER_JULIE_VERSION, HEADER_JULIE_WORKSPACE, HEADER_JULIE_WORKSPACE_SOURCE,
    };
    use crate::daemon::transport::TransportEndpoint;
    use crate::workspace::startup_hint::{WorkspaceStartupHint, WorkspaceStartupSource};

    #[derive(Debug, Clone)]
    struct FakeTransportError;

    impl std::fmt::Display for FakeTransportError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("fake transport error")
        }
    }

    impl std::error::Error for FakeTransportError {}

    struct FakeHttpTransport {
        sent: Arc<Mutex<Vec<ClientJsonRpcMessage>>>,
        received: VecDeque<ServerJsonRpcMessage>,
        closed: Arc<Mutex<bool>>,
    }

    impl FakeHttpTransport {
        fn new(received: Vec<ServerJsonRpcMessage>) -> Self {
            Self {
                sent: Arc::new(Mutex::new(Vec::new())),
                received: received.into(),
                closed: Arc::new(Mutex::new(false)),
            }
        }

        fn sent(&self) -> Arc<Mutex<Vec<ClientJsonRpcMessage>>> {
            Arc::clone(&self.sent)
        }

        fn closed(&self) -> Arc<Mutex<bool>> {
            Arc::clone(&self.closed)
        }
    }

    impl Transport<RoleClient> for FakeHttpTransport {
        type Error = FakeTransportError;

        fn send(
            &mut self,
            item: ClientJsonRpcMessage,
        ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'static {
            let sent = Arc::clone(&self.sent);
            async move {
                sent.lock().unwrap().push(item);
                Ok(())
            }
        }

        async fn receive(&mut self) -> Option<ServerJsonRpcMessage> {
            self.received.pop_front()
        }

        fn close(&mut self) -> impl Future<Output = Result<(), Self::Error>> + Send {
            let closed = Arc::clone(&self.closed);
            async move {
                *closed.lock().unwrap() = true;
                Ok(())
            }
        }
    }

    struct RootsRoundTripTransport {
        sent: Arc<Mutex<Vec<ClientJsonRpcMessage>>>,
        received: mpsc::Receiver<ServerJsonRpcMessage>,
        response_tx: mpsc::Sender<ServerJsonRpcMessage>,
        closed: Arc<Mutex<bool>>,
    }

    impl RootsRoundTripTransport {
        fn new() -> Self {
            let (response_tx, received) = mpsc::channel(4);
            response_tx
                .try_send(list_roots_request(99))
                .expect("seed roots request");
            Self {
                sent: Arc::new(Mutex::new(Vec::new())),
                received,
                response_tx,
                closed: Arc::new(Mutex::new(false)),
            }
        }

        fn sent(&self) -> Arc<Mutex<Vec<ClientJsonRpcMessage>>> {
            Arc::clone(&self.sent)
        }
    }

    impl Transport<RoleClient> for RootsRoundTripTransport {
        type Error = FakeTransportError;

        fn send(
            &mut self,
            item: ClientJsonRpcMessage,
        ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'static {
            let sent = Arc::clone(&self.sent);
            let response_tx = self.response_tx.clone();
            async move {
                let releases_initialize = matches!(
                    &item,
                    ClientJsonRpcMessage::Response(response)
                        if response.id == RequestId::Number(99)
                );
                sent.lock().unwrap().push(item);
                if releases_initialize {
                    response_tx
                        .send(initialize_response(1))
                        .await
                        .map_err(|_| FakeTransportError)?;
                }
                Ok(())
            }
        }

        async fn receive(&mut self) -> Option<ServerJsonRpcMessage> {
            self.received.recv().await
        }

        fn close(&mut self) -> impl Future<Output = Result<(), Self::Error>> + Send {
            let closed = Arc::clone(&self.closed);
            async move {
                *closed.lock().unwrap() = true;
                Ok(())
            }
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

    fn list_roots_request(id: i32) -> ServerJsonRpcMessage {
        serde_json::from_value(serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "roots/list",
            "params": {}
        }))
        .expect("valid roots/list request")
    }

    fn tools_list_response(id: i32) -> ServerJsonRpcMessage {
        serde_json::from_value(serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "tools": []
            }
        }))
        .expect("valid tools/list response")
    }

    fn restart_required_error(id: i32) -> ServerJsonRpcMessage {
        serde_json::from_value(serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": -32603,
                "message": "daemon restart required before accepting MCP traffic"
            }
        }))
        .expect("valid restart error response")
    }

    fn stdout_messages(stdout: &[u8]) -> Vec<ServerJsonRpcMessage> {
        stdout
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_slice(line).expect("valid JSON-RPC stdout message"))
            .collect()
    }

    #[tokio::test]
    async fn test_http_stdio_transport_forwards_request_and_writes_json_response() {
        let transport = FakeHttpTransport::new(vec![initialize_response(1)]);
        let sent = transport.sent();
        let closed = transport.closed();
        let (stdin, mut stdin_writer) = tokio::io::duplex(1024);
        let mut stdout = Vec::new();

        stdin_writer
            .write_all(br#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test-client","version":"0.0.0"}}}"#)
            .await
            .unwrap();
        stdin_writer.write_all(b"\n").await.unwrap();
        stdin_writer.shutdown().await.unwrap();

        let outcome = forward_http_stdio_transport(transport, stdin, &mut stdout)
            .await
            .map_err(|e| format!("{}", e))
            .expect("HTTP stdio bridge should complete");

        assert_eq!(outcome, ForwardOutcome::SessionEnded);
        assert!(*closed.lock().unwrap(), "transport should be closed on EOF");
        let sent = sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        match &sent[0] {
            ClientJsonRpcMessage::Request(request) => {
                assert_eq!(request.id, RequestId::Number(1));
                assert_eq!(request.request.method(), "initialize");
            }
            other => panic!("expected initialize request, got {other:?}"),
        }
        let stdout_message: ServerJsonRpcMessage =
            serde_json::from_slice(&stdout).expect("stdout should contain one JSON-RPC response");
        match stdout_message {
            JsonRpcMessage::Response(response) => {
                assert_eq!(response.id, RequestId::Number(1));
                assert!(matches!(response.result, ServerResult::InitializeResult(_)));
            }
            other => panic!("expected response on stdout, got {other:?}"),
        }
        assert!(
            stdout.ends_with(b"\n"),
            "stdio JSON-RPC responses should be newline-delimited"
        );
    }

    #[tokio::test]
    async fn test_http_stdio_transport_forwards_multiple_requests_in_order() {
        let transport =
            FakeHttpTransport::new(vec![initialize_response(1), tools_list_response(2)]);
        let sent = transport.sent();
        let (stdin, mut stdin_writer) = tokio::io::duplex(2048);
        let mut stdout = Vec::new();

        stdin_writer
            .write_all(br#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test-client","version":"0.0.0"}}}"#)
            .await
            .unwrap();
        stdin_writer.write_all(b"\n").await.unwrap();
        stdin_writer
            .write_all(br#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#)
            .await
            .unwrap();
        stdin_writer.write_all(b"\n").await.unwrap();
        stdin_writer.shutdown().await.unwrap();

        let outcome = forward_http_stdio_transport(transport, stdin, &mut stdout)
            .await
            .map_err(|e| format!("{}", e))
            .expect("HTTP stdio bridge should complete");

        assert_eq!(outcome, ForwardOutcome::SessionEnded);
        let sent = sent.lock().unwrap();
        assert_eq!(sent.len(), 2);
        assert_eq!(
            sent.iter()
                .filter_map(|message| match message {
                    ClientJsonRpcMessage::Request(request) => Some(request.id.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            vec![RequestId::Number(1), RequestId::Number(2)]
        );
        let messages = stdout_messages(&stdout);
        assert_eq!(
            messages
                .iter()
                .filter_map(|message| match message {
                    JsonRpcMessage::Response(response) => Some(response.id.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            vec![RequestId::Number(1), RequestId::Number(2)]
        );
    }

    #[tokio::test]
    async fn test_http_stdio_transport_handles_server_roots_request_without_deadlock() {
        let transport = RootsRoundTripTransport::new();
        let sent = transport.sent();
        let (stdin, mut stdin_writer) = tokio::io::duplex(2048);
        let mut stdout = Vec::new();

        stdin_writer
            .write_all(br#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{"roots":{}},"clientInfo":{"name":"test-client","version":"0.0.0"}}}"#)
            .await
            .unwrap();
        stdin_writer.write_all(b"\n").await.unwrap();
        stdin_writer
            .write_all(br#"{"jsonrpc":"2.0","id":99,"result":{"roots":[]}}"#)
            .await
            .unwrap();
        stdin_writer.write_all(b"\n").await.unwrap();
        stdin_writer.shutdown().await.unwrap();

        let outcome = timeout(
            Duration::from_secs(1),
            forward_http_stdio_transport(transport, stdin, &mut stdout),
        )
        .await
        .expect("bridge should read the client roots response while initialize is in flight")
        .map_err(|e| format!("{}", e))
        .expect("HTTP stdio bridge should complete");

        assert_eq!(outcome, ForwardOutcome::SessionEnded);
        assert_eq!(sent.lock().unwrap().len(), 2);
        let messages = stdout_messages(&stdout);
        assert_eq!(messages.len(), 2);
        match &messages[0] {
            JsonRpcMessage::Request(request) => {
                assert_eq!(request.id, RequestId::Number(99));
                assert!(matches!(
                    request.request,
                    ServerRequest::ListRootsRequest(_)
                ));
            }
            other => panic!("expected roots request on stdout, got {other:?}"),
        }
        match &messages[1] {
            JsonRpcMessage::Response(response) => {
                assert_eq!(response.id, RequestId::Number(1));
                assert!(matches!(response.result, ServerResult::InitializeResult(_)));
            }
            other => panic!("expected initialize response on stdout, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_http_stdio_transport_hides_restart_error_from_stdout() {
        let transport = FakeHttpTransport::new(vec![restart_required_error(1)]);
        let sent = transport.sent();
        let (stdin, mut stdin_writer) = tokio::io::duplex(1024);
        let mut stdout = Vec::new();

        stdin_writer
            .write_all(br#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test-client","version":"0.0.0"}}}"#)
            .await
            .unwrap();
        stdin_writer.write_all(b"\n").await.unwrap();
        stdin_writer.shutdown().await.unwrap();

        let outcome = forward_http_stdio_transport(transport, stdin, &mut stdout)
            .await
            .map_err(|e| format!("{}", e))
            .expect("HTTP stdio bridge should surface restart handoff");

        assert_eq!(outcome, ForwardOutcome::ImmediateDaemonDisconnect);
        assert!(
            stdout.is_empty(),
            "restart handoff errors should be retried instead of written to client stdout"
        );
        assert_eq!(sent.lock().unwrap().len(), 1);
    }

    #[test]
    fn test_http_client_config_uses_discovery_token_and_workspace_headers() {
        let dir = tempfile::tempdir().unwrap();
        let token_path = dir.path().join("daemon-mcp.token");
        std::fs::write(&token_path, "secret-token\n").unwrap();
        let endpoint = TransportEndpoint::streamable_http(
            "127.0.0.1".to_string(),
            9123,
            "/mcp",
            "/mcp/ready",
            Some(token_path),
        )
        .unwrap();
        let startup_hint = WorkspaceStartupHint {
            path: dir.path().join("workspace"),
            source: Some(WorkspaceStartupSource::Cli),
        };

        let config = http_client_config_for_endpoint(&endpoint, &startup_hint)
            .expect("HTTP endpoint should produce client config");

        assert_eq!(config.uri.as_ref(), "http://127.0.0.1:9123/mcp");
        assert_eq!(config.auth_header.as_deref(), Some("secret-token"));
        assert_eq!(
            config
                .custom_headers
                .get(&HeaderName::from_static(HEADER_JULIE_WORKSPACE))
                .unwrap()
                .to_str()
                .unwrap(),
            startup_hint.path.to_string_lossy()
        );
        assert_eq!(
            config
                .custom_headers
                .get(&HeaderName::from_static(HEADER_JULIE_WORKSPACE_SOURCE))
                .unwrap()
                .to_str()
                .unwrap(),
            "cli"
        );
        assert_eq!(
            config
                .custom_headers
                .get(&HeaderName::from_static(HEADER_JULIE_VERSION))
                .unwrap()
                .to_str()
                .unwrap(),
            env!("CARGO_PKG_VERSION")
        );
    }

    #[test]
    fn test_http_client_config_rejects_empty_token_file() {
        let dir = tempfile::tempdir().unwrap();
        let token_path = dir.path().join("daemon-mcp.token");
        std::fs::write(&token_path, "\n").unwrap();
        let endpoint = TransportEndpoint::streamable_http(
            "127.0.0.1".to_string(),
            9123,
            "/mcp",
            "/mcp/ready",
            Some(token_path),
        )
        .unwrap();
        let startup_hint = WorkspaceStartupHint {
            path: dir.path().join("workspace"),
            source: Some(WorkspaceStartupSource::Cli),
        };

        let error = http_client_config_for_endpoint(&endpoint, &startup_hint)
            .expect_err("empty token files should be rejected");

        assert!(
            error.to_string().contains("empty or malformed"),
            "unexpected error: {error:#}"
        );
    }
}
