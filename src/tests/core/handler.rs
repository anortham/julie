// Inline tests extracted from src/handler.rs
//
// This module contains all test functions that were originally inline in handler.rs.
// Tests cover tool execution and server handler functionality.

use crate::handler::JulieServerHandler;
use anyhow::Result;
use rust_mcp_sdk::error::SdkResult;
use rust_mcp_sdk::mcp_server::ServerHandler;
use rust_mcp_sdk::schema::schema_utils::{ClientMessage, MessageFromServer, ServerMessage};
use rust_mcp_sdk::schema::{
    CallToolRequest, CallToolRequestParams, InitializeRequestParams, InitializeResult, RequestId,
};
use rust_mcp_sdk::McpServer;
use std::sync::Arc;
use std::time::Duration;

struct NoopServer;

#[async_trait::async_trait]
impl McpServer for NoopServer {
    async fn start(self: Arc<Self>) -> SdkResult<()> {
        Ok(())
    }

    async fn set_client_details(&self, _client_details: InitializeRequestParams) -> SdkResult<()> {
        Ok(())
    }

    fn server_info(&self) -> &InitializeResult {
        panic!("NoopServer::server_info should not be called in tests");
    }

    fn client_info(&self) -> Option<InitializeRequestParams> {
        None
    }

    async fn wait_for_initialization(&self) {}

    async fn send(
        &self,
        _message: MessageFromServer,
        _request_id: Option<RequestId>,
        _request_timeout: Option<Duration>,
    ) -> SdkResult<Option<ClientMessage>> {
        Ok(None)
    }

    async fn send_batch(
        &self,
        _messages: Vec<ServerMessage>,
        _request_timeout: Option<Duration>,
    ) -> SdkResult<Option<Vec<ClientMessage>>> {
        Ok(None)
    }

    async fn stderr_message(&self, _message: String) -> SdkResult<()> {
        Ok(())
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn tool_lock_not_held_during_tool_execution() -> Result<()> {
    let handler = JulieServerHandler::new().await?;

    let mut arguments = serde_json::Map::new();
    arguments.insert(
        "operation".to_string(),
        serde_json::Value::String("list".to_string()),
    );
    let params = CallToolRequestParams {
        name: "manage_workspace".to_string(),
        arguments: Some(arguments),
    };
    let request = CallToolRequest::new(params);

    let result = handler
        .handle_call_tool_request(request, Arc::new(NoopServer))
        .await;

    assert!(
        result.is_ok(),
        "manage_workspace list should succeed without holding tool lock"
    );

    Ok(())
}
