//! MCP protocol contract tests for rmcp 3.0.1, date-versioned protocol 2026-07-28,
//! legacy 2025-11-25 interop, error code mapping, and modern no-init lifecycle.

use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ClientCapabilities, ClientInfo,
    ClientJsonRpcMessage, ClientRequest, ContentBlock, ErrorCode, ErrorData, Implementation,
    ProtocolVersion, RequestId, RequestMetaObject, ResultType, ServerCapabilities, ServerInfo,
    ServerJsonRpcMessage, ServerResult,
};
use rmcp::service::{RequestContext, RoleServer};
use rmcp::{ClientHandler, ServerHandler, ServiceExt};
use serde_json::json;
use std::borrow::Cow;

use crate::handler::mcp_adapter::{
    JULIE_PROTOCOL_VERSIONS, adapt_request, julie_protocol_versions, to_mcp_error,
    to_mcp_tool_result,
};
use crate::request_engine::types::{
    RequestFailure, RequestOrigin, RequestReadiness, SemanticMode, ToolReply,
};

// ============================================================================
// 1. TDD Step 1 RED Test & Protocol Version Advertisement
// ============================================================================

#[test]
fn mcp_adapter_advertises_modern_version_explicitly() {
    let versions = julie_protocol_versions();
    assert_eq!(versions[0], ProtocolVersion::V_2026_07_28);
    assert!(versions.contains(&ProtocolVersion::V_2025_11_25));
}

#[test]
fn supported_protocol_versions_order_and_completeness() {
    let versions = julie_protocol_versions();
    assert_eq!(versions[0], ProtocolVersion::V_2026_07_28);
    assert_eq!(versions[1], ProtocolVersion::V_2025_11_25);
    assert!(versions.contains(&ProtocolVersion::V_2025_06_18));
    assert!(versions.contains(&ProtocolVersion::V_2025_03_26));
    assert!(versions.contains(&ProtocolVersion::V_2024_11_05));
    // Verify we do NOT rely on LATEST (which in rmcp 3.0.1 is 2025-11-25)
    assert_ne!(versions[0], ProtocolVersion::LATEST);
}

#[test]
fn static_versions_slice_matches_function_output() {
    assert_eq!(
        JULIE_PROTOCOL_VERSIONS,
        julie_protocol_versions().as_slice()
    );
}

// ============================================================================
// 2. Error Code Mapping Tests (to_mcp_error)
// ============================================================================

#[test]
fn error_mapping_invalid_arguments_to_32602() {
    let failure = RequestFailure::invalid_arguments("parameter 'query' is required");
    let error = to_mcp_error(failure);

    assert_eq!(error.code, ErrorCode::INVALID_PARAMS);
    assert_eq!(error.message, "parameter 'query' is required");
    let data = error.data.expect("error data should be present");
    assert_eq!(data["code"], "INVALID_ARGUMENTS");
    assert_eq!(data["retryable"], false);
    assert_eq!(data["details"], json!({}));
}

#[test]
fn error_mapping_unknown_tool_to_32602_with_structured_details() {
    let failure = RequestFailure::unknown_tool("foo_bar", &["fast_search", "get_symbols"]);
    let error = to_mcp_error(failure);

    assert_eq!(error.code, ErrorCode::INVALID_PARAMS);
    assert!(error.message.contains("Unknown tool 'foo_bar'"));
    let data = error.data.expect("error data should be present");
    assert_eq!(data["code"], "UNKNOWN_TOOL");
    assert_eq!(data["retryable"], false);
    assert_eq!(data["details"]["tool"], "foo_bar");
    assert_eq!(
        data["details"]["available"],
        json!(["fast_search", "get_symbols"])
    );
}

#[test]
fn error_mapping_workspace_errors_to_32602() {
    let req_err = to_mcp_error(RequestFailure::workspace_required("Workspace required"));
    assert_eq!(req_err.code, ErrorCode::INVALID_PARAMS);
    assert_eq!(req_err.data.unwrap()["code"], "WORKSPACE_REQUIRED");

    let conflict_err = to_mcp_error(RequestFailure::workspace_conflict("Workspace conflict"));
    assert_eq!(conflict_err.code, ErrorCode::INVALID_PARAMS);
    assert_eq!(conflict_err.data.unwrap()["code"], "WORKSPACE_CONFLICT");

    let sens_err = to_mcp_error(RequestFailure::sensitive_root("Sensitive root"));
    assert_eq!(sens_err.code, ErrorCode::INVALID_PARAMS);
    assert_eq!(sens_err.data.unwrap()["code"], "SENSITIVE_ROOT");

    let fg_err = to_mcp_error(RequestFailure::foreground_required("Foreground required"));
    assert_eq!(fg_err.code, ErrorCode::INVALID_PARAMS);
    assert_eq!(fg_err.data.unwrap()["code"], "FOREGROUND_REQUIRED");
}

#[test]
fn error_mapping_internal_error_to_32603() {
    let failure = RequestFailure::internal("SQLite disk I/O error");
    let error = to_mcp_error(failure);

    assert_eq!(error.code, ErrorCode::INTERNAL_ERROR);
    assert_eq!(error.message, "SQLite disk I/O error");
    let data = error.data.expect("error data should be present");
    assert_eq!(data["code"], "INTERNAL_ERROR");
    assert_eq!(data["retryable"], false);
}

#[test]
fn error_mapping_deadline_exceeded_to_32603_retryable() {
    let failure = RequestFailure::deadline_exceeded("Operation timed out after 30s");
    let error = to_mcp_error(failure);

    assert_eq!(error.code, ErrorCode::INTERNAL_ERROR);
    let data = error.data.expect("error data should be present");
    assert_eq!(data["code"], "DEADLINE_EXCEEDED");
    assert_eq!(data["retryable"], true);
}

#[test]
fn error_mapping_cancelled_to_32603() {
    let failure = RequestFailure::cancelled("Client cancelled request");
    let error = to_mcp_error(failure);

    assert_eq!(error.code, ErrorCode::INTERNAL_ERROR);
    let data = error.data.expect("error data should be present");
    assert_eq!(data["code"], "CANCELLED");
    assert_eq!(data["retryable"], false);
}

#[test]
fn error_mapping_follower_read_only_to_32603_retryable() {
    let failure = RequestFailure::follower_read_only("Cannot mutate on follower");
    let error = to_mcp_error(failure);

    assert_eq!(error.code, ErrorCode::INTERNAL_ERROR);
    let data = error.data.expect("error data should be present");
    assert_eq!(data["code"], "FOLLOWER_READ_ONLY");
    assert_eq!(data["retryable"], true);
}

#[test]
fn error_mapping_semantics_not_ready_to_32603_with_coverage() {
    let failure = RequestFailure::semantics_not_ready(
        "Embeddings missing",
        json!({ "coverage": "missing", "expected_dim": 384 }),
    );
    let error = to_mcp_error(failure);

    assert_eq!(error.code, ErrorCode::INTERNAL_ERROR);
    let data = error.data.expect("error data should be present");
    assert_eq!(data["code"], "SEMANTICS_NOT_READY");
    assert_eq!(data["retryable"], true);
    assert_eq!(data["details"]["coverage"], "missing");
    assert_eq!(data["details"]["expected_dim"], 384);
}

// ============================================================================
// 3. Tool Result & resultType Discriminator Tests (to_mcp_tool_result)
// ============================================================================

#[test]
fn to_mcp_tool_result_attaches_complete_result_type() {
    let reply = ToolReply::from_result(
        "fast_search",
        Some("ws1".into()),
        json!({
            "content": [{ "type": "text", "text": "found 3 symbols" }],
            "isError": false,
        }),
        RequestReadiness::ready(SemanticMode::Auto),
    );

    let result = to_mcp_tool_result(reply).expect("conversion should succeed");
    assert_eq!(result.result_type, Some(ResultType::COMPLETE));
    assert_eq!(result.is_error, Some(false));
    assert_eq!(result.content.len(), 1);
}

#[test]
fn to_mcp_tool_result_preserves_is_error_true() {
    let reply = ToolReply::from_result(
        "edit_file",
        Some("ws1".into()),
        json!({
            "content": [{ "type": "text", "text": "syntax error on line 4" }],
            "isError": true,
        }),
        RequestReadiness::disabled(),
    );

    let result = to_mcp_tool_result(reply).expect("conversion should succeed");
    assert_eq!(result.result_type, Some(ResultType::COMPLETE));
    assert_eq!(result.is_error, Some(true));
}

#[test]
fn call_tool_result_serialization_contains_result_type_for_modern() {
    let result = CallToolResult::success(vec![ContentBlock::text("hello")]);
    let value = serde_json::to_value(&result).expect("serialize CallToolResult");

    assert_eq!(value["resultType"], "complete");
    assert_eq!(value["isError"], false);
    assert_eq!(value["content"][0]["text"], "hello");
}

#[test]
fn strip_result_type_for_legacy_peer_removes_discriminator() {
    let mut server_result =
        ServerResult::CallToolResult(CallToolResult::success(vec![ContentBlock::text(
            "legacy response",
        )]));

    // rmcp automatically invokes this for peers negotiating < 2026-07-28
    server_result.strip_result_type_for_legacy_peer();

    let value = serde_json::to_value(&server_result).expect("serialize ServerResult");
    assert!(
        value.get("resultType").is_none(),
        "legacy wire payload must NOT contain resultType"
    );
    assert_eq!(value["isError"], false);
    assert_eq!(value["content"][0]["text"], "legacy response");
}

// ============================================================================
// 4. Request Adaptation Tests (adapt_request)
// ============================================================================

#[test]
fn adapt_request_extracts_name_and_arguments() {
    let mut params = CallToolRequestParams::new("fast_search");
    let mut args = serde_json::Map::new();
    args.insert("query".into(), json!("test"));
    params.arguments = Some(args);

    let (req, ctx) = adapt_request(params, None).expect("adapt should succeed");
    assert_eq!(req.name, "fast_search");
    assert_eq!(req.arguments["query"], "test");
    assert_eq!(req.semantics, SemanticMode::Auto);
    assert_eq!(ctx.origin, RequestOrigin::Mcp);
}

#[test]
fn adapt_request_uses_provided_workspace_fallback() {
    let params = CallToolRequestParams::new("get_symbols");
    let ws = std::path::PathBuf::from("/path/to/repo");

    let (req, _ctx) = adapt_request(params, Some(ws.clone())).expect("adapt should succeed");
    assert_eq!(req.workspace, Some(ws));
}

// ============================================================================
// 5. Wire-Level Duplex Lifecycle Tests (In-Memory tokio::io::duplex)
// ============================================================================

#[derive(Clone, Default)]
struct MockTestMcpServer;

impl ServerHandler for MockTestMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("Julie-Test", "1.0.0"))
    }

    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        Cow::Borrowed(JULIE_PROTOCOL_VERSIONS)
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        if request.name == "unknown_tool" {
            return Err(to_mcp_error(RequestFailure::unknown_tool(
                "unknown_tool",
                &["fast_search"],
            )));
        }
        let reply = ToolReply::from_result(
            request.name.as_ref(),
            Some("ws-mock".into()),
            json!({
                "content": [{ "type": "text", "text": "mock tool success" }],
                "isError": false,
            }),
            RequestReadiness::ready(SemanticMode::Auto),
        );
        Ok(to_mcp_tool_result(reply)?.into())
    }
}

#[derive(Clone)]
struct VersionedMockClient {
    version: ProtocolVersion,
}

impl ClientHandler for VersionedMockClient {
    fn get_info(&self) -> ClientInfo {
        let mut info = ClientInfo::default();
        info.protocol_version = self.version.clone();
        info
    }
}

fn make_modern_meta() -> RequestMetaObject {
    let mut meta = RequestMetaObject::new();
    meta.set_protocol_version(ProtocolVersion::V_2026_07_28);
    meta.set_client_info(Implementation::new("direct-client", "1.0.0"));
    meta.set_client_capabilities(ClientCapabilities::default());
    meta
}

#[tokio::test]
async fn modern_direct_call_succeeds_without_prior_handshake() {
    use rmcp::transport::{IntoTransport, Transport};

    let (server_transport, client_transport) = tokio::io::duplex(4096);
    let server_task = tokio::spawn(async move {
        MockTestMcpServer
            .serve(server_transport)
            .await
            .expect("server serve")
            .waiting()
            .await
    });

    let mut client = IntoTransport::<rmcp::RoleClient, _, _>::into_transport(client_transport);

    // Direct first-message tools/call with modern metadata
    let mut call_params = CallToolRequestParams::new("fast_search");
    call_params.meta = Some(make_modern_meta());
    let call_req = rmcp::model::CallToolRequest::new(call_params);

    client
        .send(ClientJsonRpcMessage::request(
            ClientRequest::CallToolRequest(call_req),
            RequestId::Number(1),
        ))
        .await
        .expect("send direct call");

    let response = client.receive().await.expect("receive response");
    match response {
        ServerJsonRpcMessage::Response(res) => {
            assert_eq!(res.id, RequestId::Number(1));
            let val = serde_json::to_value(&res.result).expect("serialize result");
            assert_eq!(val["resultType"], "complete");
            assert_eq!(val["isError"], false);
            assert_eq!(val["content"][0]["text"], "mock tool success");
        }
        other => panic!("expected successful tool call response, got {other:?}"),
    }

    server_task.abort();
}

#[tokio::test]
async fn modern_direct_call_missing_required_meta_rejected_with_32602() {
    use rmcp::transport::{IntoTransport, Transport};

    let (server_transport, client_transport) = tokio::io::duplex(4096);
    let server_task = tokio::spawn(async move {
        MockTestMcpServer
            .serve(server_transport)
            .await
            .expect("server serve")
            .waiting()
            .await
    });

    let mut client = IntoTransport::<rmcp::RoleClient, _, _>::into_transport(client_transport);

    // 1. Initial direct call establishes session without initialize
    let mut initial_params = CallToolRequestParams::new("fast_search");
    initial_params.meta = Some(make_modern_meta());
    client
        .send(ClientJsonRpcMessage::request(
            ClientRequest::CallToolRequest(rmcp::model::CallToolRequest::new(initial_params)),
            RequestId::Number(1),
        ))
        .await
        .expect("send initial direct call");

    let initial_response = client.receive().await.expect("receive initial response");
    assert!(matches!(
        initial_response,
        ServerJsonRpcMessage::Response(_)
    ));

    // 2. Subsequent call in modern inline session omitting required meta keys
    let call_params = CallToolRequestParams::new("fast_search");
    let call_req = rmcp::model::CallToolRequest::new(call_params);

    client
        .send(ClientJsonRpcMessage::request(
            ClientRequest::CallToolRequest(call_req),
            RequestId::Number(2),
        ))
        .await
        .expect("send direct call without meta");

    let response = client.receive().await.expect("receive response");
    match response {
        ServerJsonRpcMessage::Error(err) => {
            assert_eq!(err.id, Some(RequestId::Number(2)));
            assert_eq!(err.error.code, ErrorCode::INVALID_PARAMS);
            assert!(
                err.error
                    .message
                    .contains("missing or has malformed required fields")
            );
        }
        other => panic!("expected invalid params error, got {other:?}"),
    }

    server_task.abort();
}

#[tokio::test]
async fn modern_direct_call_unsupported_version_rejected() {
    use rmcp::transport::{IntoTransport, Transport};

    let (server_transport, client_transport) = tokio::io::duplex(4096);
    let server_task = tokio::spawn(async move {
        MockTestMcpServer
            .serve(server_transport)
            .await
            .expect("server serve")
            .waiting()
            .await
    });

    let mut client = IntoTransport::<rmcp::RoleClient, _, _>::into_transport(client_transport);

    let mut meta = RequestMetaObject::new();
    let unsupported_version: ProtocolVersion = serde_json::from_str(r#""1999-01-01""#).unwrap();
    meta.set_protocol_version(unsupported_version);
    meta.set_client_capabilities(ClientCapabilities::default());

    let mut call_params = CallToolRequestParams::new("fast_search");
    call_params.meta = Some(meta);
    let call_req = rmcp::model::CallToolRequest::new(call_params);

    client
        .send(ClientJsonRpcMessage::request(
            ClientRequest::CallToolRequest(call_req),
            RequestId::Number(1),
        ))
        .await
        .expect("send direct call");

    let response = client.receive().await.expect("receive response");
    match response {
        ServerJsonRpcMessage::Error(err) => {
            assert_eq!(err.id, Some(RequestId::Number(1)));
            assert_eq!(err.error.code, ErrorCode::UNSUPPORTED_PROTOCOL_VERSION);
        }
        other => panic!("expected unsupported protocol version error, got {other:?}"),
    }

    server_task.abort();
}

#[tokio::test]
async fn legacy_session_omits_result_type_on_wire() {
    let (server_transport, client_transport) = tokio::io::duplex(4096);
    let server_task = tokio::spawn(async move {
        MockTestMcpServer
            .serve(server_transport)
            .await
            .expect("server serve")
            .waiting()
            .await
    });

    // Legacy client negotiating 2025-11-25 via handshake
    let client = VersionedMockClient {
        version: ProtocolVersion::V_2025_11_25,
    }
    .serve(client_transport)
    .await
    .expect("legacy client connect");

    let res = client
        .call_tool(CallToolRequestParams::new("fast_search"))
        .await
        .expect("tool call succeed");

    assert_eq!(
        res.result_type, None,
        "legacy 2025-11-25 peer must not receive resultType"
    );

    client.cancel().await.expect("cancel client");
    server_task.abort();
}

#[tokio::test]
async fn modern_session_receives_complete_result_type_on_wire() {
    let (server_transport, client_transport) = tokio::io::duplex(4096);
    let server_task = tokio::spawn(async move {
        MockTestMcpServer
            .serve(server_transport)
            .await
            .expect("server serve")
            .waiting()
            .await
    });

    // Modern client negotiating 2026-07-28 via handshake
    let client = VersionedMockClient {
        version: ProtocolVersion::V_2026_07_28,
    }
    .serve(client_transport)
    .await
    .expect("modern client connect");

    let res = client
        .call_tool(CallToolRequestParams::new("fast_search"))
        .await
        .expect("tool call succeed");

    assert_eq!(
        res.result_type,
        Some(ResultType::COMPLETE),
        "modern 2026-07-28 peer must receive resultType: 'complete'"
    );

    client.cancel().await.expect("cancel client");
    server_task.abort();
}

// ============================================================================
// 6. Empirical Challenger Adversarial Tests (McpAdapter Direct Wire Tests)
// ============================================================================

#[tokio::test]
async fn adversarial_mcp_adapter_supported_protocol_versions_advertises_2026_07_28_primary() {
    let fixture = crate::tests::request_engine::RequestFixture::indexed().await;
    let adapter = crate::handler::mcp_adapter::McpAdapter::new(
        std::sync::Arc::new(fixture.engine),
        Some(fixture.root.clone()),
    );
    let versions =
        <crate::handler::mcp_adapter::McpAdapter as ServerHandler>::supported_protocol_versions(
            &adapter,
        );
    assert_eq!(versions[0], ProtocolVersion::V_2026_07_28);
    assert_eq!(versions[1], ProtocolVersion::V_2025_11_25);
    assert!(versions.contains(&ProtocolVersion::V_2025_06_18));
    assert!(versions.contains(&ProtocolVersion::V_2025_03_26));
    assert!(versions.contains(&ProtocolVersion::V_2024_11_05));
    assert_ne!(versions[0], ProtocolVersion::LATEST);
}

#[tokio::test]
async fn adversarial_mcp_adapter_duplex_modern_session_receives_complete_result_type() {
    let fixture = crate::tests::request_engine::RequestFixture::indexed().await;
    let adapter = crate::handler::mcp_adapter::McpAdapter::new(
        std::sync::Arc::new(fixture.engine),
        Some(fixture.root.clone()),
    );
    let (server_transport, client_transport) = tokio::io::duplex(4096);
    let server_task = tokio::spawn(async move {
        let _keep_dirs = (&fixture.temp_home, &fixture.temp_repo);
        adapter
            .serve(server_transport)
            .await
            .expect("adapter serve")
            .waiting()
            .await
    });

    let client = VersionedMockClient {
        version: ProtocolVersion::V_2026_07_28,
    }
    .serve(client_transport)
    .await
    .expect("modern client connect");

    let mut params = CallToolRequestParams::new("manage_workspace");
    let mut args = serde_json::Map::new();
    args.insert("operation".into(), json!("list"));
    params.arguments = Some(args);

    let res = client
        .call_tool(params)
        .await
        .expect("tool call succeed on McpAdapter");

    assert_eq!(
        res.result_type,
        Some(ResultType::COMPLETE),
        "modern 2026-07-28 peer must receive resultType: 'complete' from McpAdapter"
    );

    client.cancel().await.expect("cancel client");
    server_task.abort();
}

#[tokio::test]
async fn adversarial_mcp_adapter_duplex_legacy_session_omits_result_type() {
    let fixture = crate::tests::request_engine::RequestFixture::indexed().await;
    let adapter = crate::handler::mcp_adapter::McpAdapter::new(
        std::sync::Arc::new(fixture.engine),
        Some(fixture.root.clone()),
    );
    let (server_transport, client_transport) = tokio::io::duplex(4096);
    let server_task = tokio::spawn(async move {
        let _keep_dirs = (&fixture.temp_home, &fixture.temp_repo);
        adapter
            .serve(server_transport)
            .await
            .expect("adapter serve")
            .waiting()
            .await
    });

    let client = VersionedMockClient {
        version: ProtocolVersion::V_2025_11_25,
    }
    .serve(client_transport)
    .await
    .expect("legacy client connect");

    let mut params = CallToolRequestParams::new("manage_workspace");
    let mut args = serde_json::Map::new();
    args.insert("operation".into(), json!("list"));
    params.arguments = Some(args);

    let res = client
        .call_tool(params)
        .await
        .expect("tool call succeed on McpAdapter");

    assert_eq!(
        res.result_type, None,
        "legacy 2025-11-25 peer must not receive resultType from McpAdapter"
    );

    client.cancel().await.expect("cancel client");
    server_task.abort();
}

#[tokio::test]
async fn adversarial_mcp_adapter_duplex_unsupported_version_rejected() {
    use rmcp::transport::{IntoTransport, Transport};

    let fixture = crate::tests::request_engine::RequestFixture::indexed().await;
    let adapter = crate::handler::mcp_adapter::McpAdapter::new(
        std::sync::Arc::new(fixture.engine),
        Some(fixture.root.clone()),
    );
    let (server_transport, client_transport) = tokio::io::duplex(4096);
    let server_task = tokio::spawn(async move {
        let _keep_dirs = (&fixture.temp_home, &fixture.temp_repo);
        adapter
            .serve(server_transport)
            .await
            .expect("adapter serve")
            .waiting()
            .await
    });

    let mut client = IntoTransport::<rmcp::RoleClient, _, _>::into_transport(client_transport);

    let mut meta = RequestMetaObject::new();
    let unsupported_version: ProtocolVersion = serde_json::from_str(r#""1999-01-01""#).unwrap();
    meta.set_protocol_version(unsupported_version);
    meta.set_client_capabilities(ClientCapabilities::default());

    let mut call_params = CallToolRequestParams::new("manage_workspace");
    let mut args = serde_json::Map::new();
    args.insert("operation".into(), json!("list"));
    call_params.arguments = Some(args);
    call_params.meta = Some(meta);
    let call_req = rmcp::model::CallToolRequest::new(call_params);

    client
        .send(ClientJsonRpcMessage::request(
            ClientRequest::CallToolRequest(call_req),
            RequestId::Number(1),
        ))
        .await
        .expect("send direct call");

    let response = client.receive().await.expect("receive response");
    match response {
        ServerJsonRpcMessage::Error(err) => {
            assert_eq!(err.id, Some(RequestId::Number(1)));
            assert_eq!(err.error.code, ErrorCode::UNSUPPORTED_PROTOCOL_VERSION);
        }
        other => panic!("expected unsupported protocol version error, got {other:?}"),
    }

    server_task.abort();
}

#[tokio::test]
async fn adversarial_mcp_adapter_duplex_modern_direct_call_without_handshake() {
    use rmcp::transport::{IntoTransport, Transport};

    let fixture = crate::tests::request_engine::RequestFixture::indexed().await;
    let adapter = crate::handler::mcp_adapter::McpAdapter::new(
        std::sync::Arc::new(fixture.engine),
        Some(fixture.root.clone()),
    );
    let (server_transport, client_transport) = tokio::io::duplex(4096);
    let server_task = tokio::spawn(async move {
        let _keep_dirs = (&fixture.temp_home, &fixture.temp_repo);
        adapter
            .serve(server_transport)
            .await
            .expect("adapter serve")
            .waiting()
            .await
    });

    let mut client = IntoTransport::<rmcp::RoleClient, _, _>::into_transport(client_transport);

    let mut call_params = CallToolRequestParams::new("manage_workspace");
    let mut args = serde_json::Map::new();
    args.insert("operation".into(), json!("list"));
    call_params.arguments = Some(args);
    call_params.meta = Some(make_modern_meta());
    let call_req = rmcp::model::CallToolRequest::new(call_params);

    client
        .send(ClientJsonRpcMessage::request(
            ClientRequest::CallToolRequest(call_req),
            RequestId::Number(1),
        ))
        .await
        .expect("send direct call");

    let response = client.receive().await.expect("receive response");
    match response {
        ServerJsonRpcMessage::Response(res) => {
            assert_eq!(res.id, RequestId::Number(1));
            let val = serde_json::to_value(&res.result).expect("serialize result");
            assert_eq!(val["resultType"], "complete");
            assert_eq!(val["isError"], false);
        }
        other => panic!("expected successful tool call response, got {other:?}"),
    }

    server_task.abort();
}

#[tokio::test]
async fn adversarial_mcp_adapter_duplex_missing_required_meta_rejected_with_32602() {
    use rmcp::transport::{IntoTransport, Transport};

    let fixture = crate::tests::request_engine::RequestFixture::indexed().await;
    let adapter = crate::handler::mcp_adapter::McpAdapter::new(
        std::sync::Arc::new(fixture.engine),
        Some(fixture.root.clone()),
    );
    let (server_transport, client_transport) = tokio::io::duplex(4096);
    let server_task = tokio::spawn(async move {
        let _keep_dirs = (&fixture.temp_home, &fixture.temp_repo);
        adapter
            .serve(server_transport)
            .await
            .expect("adapter serve")
            .waiting()
            .await
    });

    let mut client = IntoTransport::<rmcp::RoleClient, _, _>::into_transport(client_transport);

    // Initial direct call establishes session
    let mut initial_params = CallToolRequestParams::new("manage_workspace");
    let mut args = serde_json::Map::new();
    args.insert("operation".into(), json!("list"));
    initial_params.arguments = Some(args);
    initial_params.meta = Some(make_modern_meta());
    client
        .send(ClientJsonRpcMessage::request(
            ClientRequest::CallToolRequest(rmcp::model::CallToolRequest::new(initial_params)),
            RequestId::Number(1),
        ))
        .await
        .expect("send initial direct call");

    let initial_response = client.receive().await.expect("receive initial response");
    assert!(matches!(
        initial_response,
        ServerJsonRpcMessage::Response(_)
    ));

    // Subsequent call omitting required meta keys
    let call_params = CallToolRequestParams::new("manage_workspace");
    let call_req = rmcp::model::CallToolRequest::new(call_params);

    client
        .send(ClientJsonRpcMessage::request(
            ClientRequest::CallToolRequest(call_req),
            RequestId::Number(2),
        ))
        .await
        .expect("send direct call without meta");

    let response = client.receive().await.expect("receive response");
    match response {
        ServerJsonRpcMessage::Error(err) => {
            assert_eq!(err.id, Some(RequestId::Number(2)));
            assert_eq!(err.error.code, ErrorCode::INVALID_PARAMS);
            assert!(
                err.error
                    .message
                    .contains("missing or has malformed required fields")
            );
        }
        other => panic!("expected invalid params error, got {other:?}"),
    }

    server_task.abort();
}

#[tokio::test]
async fn adversarial_mcp_adapter_duplex_unknown_tool_returns_32602() {
    let fixture = crate::tests::request_engine::RequestFixture::indexed().await;
    let adapter = crate::handler::mcp_adapter::McpAdapter::new(
        std::sync::Arc::new(fixture.engine),
        Some(fixture.root.clone()),
    );
    let (server_transport, client_transport) = tokio::io::duplex(4096);
    let server_task = tokio::spawn(async move {
        let _keep_dirs = (&fixture.temp_home, &fixture.temp_repo);
        adapter
            .serve(server_transport)
            .await
            .expect("adapter serve")
            .waiting()
            .await
    });

    let client = VersionedMockClient {
        version: ProtocolVersion::V_2026_07_28,
    }
    .serve(client_transport)
    .await
    .expect("client connect");

    let err = match client
        .call_tool(CallToolRequestParams::new("totally_unknown_tool"))
        .await
        .expect_err("unknown tool should fail")
    {
        rmcp::service::ServiceError::McpError(mcp_err) => mcp_err,
        other => panic!("expected McpError, got {other:?}"),
    };

    assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
    let data = err.data.expect("error data");
    assert_eq!(data["code"], "UNKNOWN_TOOL");

    client.cancel().await.expect("cancel client");
    server_task.abort();
}

#[tokio::test]
async fn adversarial_mcp_adapter_duplex_invalid_params_returns_32602() {
    let fixture = crate::tests::request_engine::RequestFixture::indexed().await;
    let adapter = crate::handler::mcp_adapter::McpAdapter::new(
        std::sync::Arc::new(fixture.engine),
        Some(fixture.root.clone()),
    );
    let (server_transport, client_transport) = tokio::io::duplex(4096);
    let server_task = tokio::spawn(async move {
        let _keep_dirs = (&fixture.temp_home, &fixture.temp_repo);
        adapter
            .serve(server_transport)
            .await
            .expect("adapter serve")
            .waiting()
            .await
    });

    let client = VersionedMockClient {
        version: ProtocolVersion::V_2026_07_28,
    }
    .serve(client_transport)
    .await
    .expect("client connect");

    // Missing required field 'operation' on manage_workspace
    let err = match client
        .call_tool(CallToolRequestParams::new("manage_workspace"))
        .await
        .expect_err("invalid params should fail")
    {
        rmcp::service::ServiceError::McpError(mcp_err) => mcp_err,
        other => panic!("expected McpError, got {other:?}"),
    };

    assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
    let data = err.data.expect("error data");
    assert_eq!(data["code"], "INVALID_ARGUMENTS");

    client.cancel().await.expect("cancel client");
    server_task.abort();
}
