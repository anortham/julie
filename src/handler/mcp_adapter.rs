//! MCP protocol adapter for rmcp 3.0.1 and date-versioned protocol 2026-07-28.
//!
//! Bridges rmcp JSON-RPC request structures to Julie's transport-neutral
//! [`RequestEngine`], ensuring identical execution policy, access classes,
//! and semantic readiness between CLI and MCP transports.

use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;

use rmcp::RoleServer;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ErrorCode, ErrorData as McpError,
    Implementation, InitializeRequestParams, ListToolsResult, ProtocolVersion, ResultType,
    ServerCapabilities, ServerInfo, Tool,
};
use rmcp::service::RequestContext;
use serde_json::{Map, Value, json};

use crate::request_engine::RequestEngine;
use crate::request_engine::catalog::ToolCatalog;
use crate::request_engine::types::{
    RequestContext as AppRequestContext, RequestFailure, RequestOrigin, SemanticMode, ToolReply,
    ToolRequest,
};

/// Supported protocol versions in order of preference (modern primary first).
pub static JULIE_PROTOCOL_VERSIONS: &[ProtocolVersion] = &[
    ProtocolVersion::V_2026_07_28,
    ProtocolVersion::V_2025_11_25,
    ProtocolVersion::V_2025_06_18,
    ProtocolVersion::V_2025_03_26,
    ProtocolVersion::V_2024_11_05,
];

/// Return the ordered list of supported MCP protocol versions.
///
/// Advertises modern `2026-07-28` as primary (first entry) and retains
/// legacy `2025-11-25` interoperability.
pub fn julie_protocol_versions() -> Vec<ProtocolVersion> {
    JULIE_PROTOCOL_VERSIONS.to_vec()
}

/// Construct server metadata and capabilities advertised during MCP discovery/initialize.
pub fn get_server_info(instructions: Option<String>) -> ServerInfo {
    let server_info = Implementation::new("Julie", env!("CARGO_PKG_VERSION"))
        .with_title("Julie - Code Intelligence Server");

    let mut info = ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
        .with_server_info(server_info)
        .with_protocol_version(ProtocolVersion::V_2026_07_28);

    if let Some(instructions) = instructions {
        info = info.with_instructions(instructions);
    }

    info
}

/// Map an application [`RequestFailure`] into standardized rmcp [`McpError`].
///
/// Mapping rules:
/// - Invalid arguments, unknown tool, workspace conflict/required -> -32602 (INVALID_PARAMS)
/// - Runtime failure, follower refusal, tool domain error, internal -> -32603 (INTERNAL_ERROR)
/// - Structured `data` envelope carries `code`, `retryable`, and `details`.
pub fn to_mcp_error(failure: RequestFailure) -> McpError {
    let error_code = match failure.code.as_str() {
        "INVALID_ARGUMENTS"
        | "UNKNOWN_TOOL"
        | "WORKSPACE_REQUIRED"
        | "WORKSPACE_CONFLICT"
        | "SENSITIVE_ROOT"
        | "FOREGROUND_REQUIRED" => ErrorCode::INVALID_PARAMS,
        _ => ErrorCode::INTERNAL_ERROR,
    };

    let data = json!({
        "code": failure.code,
        "retryable": failure.retryable,
        "details": failure.details,
    });

    McpError::new(error_code, failure.message, Some(data))
}

/// Convert a transport-neutral [`ToolReply`] into rmcp [`CallToolResult`].
///
/// Preserves `isError`, structured content, and ensures `resultType: "complete"`
/// is set for the modern 2026-07-28 protocol. (rmcp SDK automatically strips
/// `resultType` for legacy peers negotiating < 2026-07-28).
pub fn to_mcp_tool_result(reply: ToolReply) -> Result<CallToolResult, McpError> {
    let is_err = reply.is_error();

    let mut call_result: CallToolResult = match serde_json::from_value(reply.result.clone()) {
        Ok(r) => r,
        Err(_) => {
            if is_err {
                CallToolResult::structured_error(reply.result)
            } else {
                CallToolResult::structured(reply.result)
            }
        }
    };

    // Modern 2026-07-28 explicitly requires resultType: "complete" on the wire
    call_result.result_type = Some(ResultType::COMPLETE);

    if is_err {
        call_result.is_error = Some(true);
    }

    Ok(call_result)
}

/// Convert rmcp [`CallToolRequestParams`] and optional default workspace into
/// application [`ToolRequest`] and [`AppRequestContext`].
pub fn adapt_request(
    request: CallToolRequestParams,
    default_workspace: Option<PathBuf>,
) -> Result<(ToolRequest, AppRequestContext), McpError> {
    adapt_request_with_cancellation(request, default_workspace, None)
}

/// Convert rmcp [`CallToolRequestParams`], [`RequestContext`], and default workspace
/// into application [`ToolRequest`] and [`AppRequestContext`].
pub fn adapt_request_with_context(
    request: CallToolRequestParams,
    context: &RequestContext<RoleServer>,
    default_workspace: Option<PathBuf>,
) -> Result<(ToolRequest, AppRequestContext), McpError> {
    adapt_request_with_cancellation(request, default_workspace, Some(&context.ct))
}

/// Internal helper for request adaptation with optional cancellation token.
pub fn adapt_request_with_cancellation(
    request: CallToolRequestParams,
    default_workspace: Option<PathBuf>,
    cancellation: Option<&tokio_util::sync::CancellationToken>,
) -> Result<(ToolRequest, AppRequestContext), McpError> {
    let tool_name = request.name.to_string();
    if tool_name.trim().is_empty() {
        return Err(to_mcp_error(RequestFailure::invalid_arguments(
            "Tool name cannot be empty",
        )));
    }

    let mut arguments: Map<String, Value> = request.arguments.unwrap_or_default();

    let workspace = arguments
        .get("workspace")
        .and_then(|v| v.as_str())
        .and_then(|s| {
            if s == "primary" || s == "default" {
                None
            } else {
                Some(PathBuf::from(s))
            }
        })
        .or(default_workspace);

    let semantics = arguments
        .remove("semantics")
        .and_then(|v| serde_json::from_value::<SemanticMode>(v).ok())
        .unwrap_or(SemanticMode::Auto);

    let exempt = crate::handler::is_write_exempt(&tool_name, Some(&arguments));
    let timeout = if exempt {
        None
    } else {
        crate::handler::parse_request_timeout(
            std::env::var("JULIE_INPROCESS_REQUEST_TIMEOUT_SECS").ok(),
        )
    };

    let tool_request = ToolRequest {
        name: tool_name,
        arguments,
        workspace,
        semantics,
    };

    let ct = cancellation.cloned().unwrap_or_default();
    let req_context = AppRequestContext::new(RequestOrigin::Mcp, timeout, ct);

    Ok((tool_request, req_context))
}

/// MCP protocol adapter wrapping [`RequestEngine`] and implementing [`rmcp::ServerHandler`].
pub struct McpAdapter {
    engine: Arc<RequestEngine>,
    default_workspace: Option<PathBuf>,
    instructions: Option<String>,
}

impl McpAdapter {
    pub fn new(engine: Arc<RequestEngine>, default_workspace: Option<PathBuf>) -> Self {
        Self {
            engine,
            default_workspace,
            instructions: None,
        }
    }

    pub fn with_instructions(mut self, instructions: Option<String>) -> Self {
        self.instructions = instructions;
        self
    }

    pub fn adapt_request(
        &self,
        request: CallToolRequestParams,
        context: &RequestContext<RoleServer>,
    ) -> Result<(ToolRequest, AppRequestContext), McpError> {
        adapt_request_with_context(request, context, self.default_workspace.clone())
    }

    pub async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let (req, app_ctx) = self.adapt_request(request, &context)?;
        let reply = self
            .engine
            .execute(req, app_ctx)
            .await
            .map_err(to_mcp_error)?;
        let result = to_mcp_tool_result(reply)?;
        Ok(result.into())
    }

    pub fn list_tools(&self) -> ListToolsResult {
        let tools = ToolCatalog::list()
            .into_iter()
            .map(|info| {
                let schema_obj = match info.schema {
                    Value::Object(map) => map,
                    _ => Map::new(),
                };
                Tool::new(info.name, info.description, Arc::new(schema_obj))
            })
            .collect();
        ListToolsResult::with_all_items(tools)
    }

    pub fn get_tool(&self, name: &str) -> Option<Tool> {
        ToolCatalog::list()
            .into_iter()
            .find(|info| info.name == name)
            .map(|info| {
                let schema_obj = match info.schema {
                    Value::Object(map) => map,
                    _ => Map::new(),
                };
                Tool::new(info.name, info.description, Arc::new(schema_obj))
            })
    }

    pub fn get_info(&self) -> ServerInfo {
        get_server_info(self.instructions.clone())
    }
}

impl rmcp::ServerHandler for McpAdapter {
    fn get_info(&self) -> ServerInfo {
        self.get_info()
    }

    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        Cow::Borrowed(JULIE_PROTOCOL_VERSIONS)
    }

    async fn initialize(
        &self,
        request: InitializeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ServerInfo, McpError> {
        if context.peer.peer_info().is_none() {
            context.peer.set_peer_info(request);
        }
        Ok(self.get_info())
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        self.call_tool(request, context).await
    }

    async fn list_tools(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(self.list_tools())
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.get_tool(name)
    }
}
