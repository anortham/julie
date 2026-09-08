//! Request engine core types, context, reply envelopes, and error representations.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

/// Immutable workspace binding for an active tool execution.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkspaceBinding {
    pub workspace_id: String,
    pub root: PathBuf,
    pub index_root: PathBuf,
}

/// Execution context carrying deadlines, cooperative cancellation, and caller origin.
#[derive(Debug, Clone)]
pub struct RequestContext {
    pub deadline: Instant,
    pub cancellation: CancellationToken,
    pub origin: RequestOrigin,
}

impl RequestContext {
    pub fn new(
        origin: RequestOrigin,
        timeout: Option<Duration>,
        cancellation: CancellationToken,
    ) -> Self {
        let deadline = match timeout {
            Some(d) => Instant::now() + d,
            None => Instant::now() + Duration::from_secs(3600 * 24),
        };
        Self {
            deadline,
            cancellation,
            origin,
        }
    }

    pub fn check_cancelled(&self) -> Result<(), RequestFailure> {
        if self.cancellation.is_cancelled() {
            return Err(RequestFailure::cancelled("Request was cancelled"));
        }
        if Instant::now() >= self.deadline {
            return Err(RequestFailure::deadline_exceeded(
                "Request deadline exceeded",
            ));
        }
        Ok(())
    }

    pub fn to_std_deadline(&self) -> std::time::Instant {
        let now_tokio = Instant::now();
        let now_std = std::time::Instant::now();
        if self.deadline > now_tokio {
            now_std + (self.deadline - now_tokio)
        } else {
            now_std
        }
    }
}

/// Request origin transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RequestOrigin {
    Cli,
    Mcp,
}

/// Tool operation access classification for concurrency and follower safety.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessClass {
    Read,
    Preview,
    SourceEdit,
    IndexMutation,
    Unbound,
}

impl AccessClass {
    pub fn is_mutating(&self) -> bool {
        matches!(self, Self::SourceEdit | Self::IndexMutation)
    }

    pub fn is_index_mutation(&self) -> bool {
        matches!(self, Self::IndexMutation)
    }

    pub fn is_source_edit(&self) -> bool {
        matches!(self, Self::SourceEdit)
    }

    pub fn is_read_only(&self) -> bool {
        !self.is_mutating()
    }
}

/// Semantic retrieval execution mode.
pub use super::semantic::SemanticMode;

/// Operational readiness evidence returned in tool responses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestReadiness {
    pub mode: SemanticMode,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coverage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical_revision: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lexical_revision: Option<u64>,
}

impl RequestReadiness {
    pub fn ready(mode: SemanticMode) -> Self {
        Self {
            mode,
            status: "ready".to_string(),
            coverage: None,
            canonical_revision: None,
            lexical_revision: None,
        }
    }

    pub fn disabled() -> Self {
        Self {
            mode: SemanticMode::Off,
            status: "disabled".to_string(),
            coverage: None,
            canonical_revision: None,
            lexical_revision: None,
        }
    }
}

/// Transport-neutral tool execution request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRequest {
    pub name: String,
    pub arguments: Map<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<PathBuf>,
    #[serde(default)]
    pub semantics: SemanticMode,
}

impl ToolRequest {
    pub fn new(name: impl Into<String>, arguments: Map<String, Value>) -> Self {
        Self {
            name: name.into(),
            arguments,
            workspace: None,
            semantics: SemanticMode::Auto,
        }
    }

    pub fn with_workspace(mut self, workspace: Option<PathBuf>) -> Self {
        self.workspace = workspace;
        self
    }

    pub fn with_semantics(mut self, semantics: SemanticMode) -> Self {
        self.semantics = semantics;
        self
    }
}

/// Transport-neutral tool execution reply.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolReply {
    pub schema_version: u32,
    pub tool: String,
    pub workspace_id: Option<String>,
    pub result: Value,
    pub readiness: RequestReadiness,
}

impl ToolReply {
    pub fn from_result(
        tool: impl Into<String>,
        workspace_id: Option<String>,
        mut result: Value,
        readiness: RequestReadiness,
    ) -> Self {
        if let Some(obj) = result.as_object_mut() {
            obj.remove("resultType");
        }
        Self {
            schema_version: 1,
            tool: tool.into(),
            workspace_id,
            result,
            readiness,
        }
    }

    pub fn is_error(&self) -> bool {
        self.result
            .get("isError")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }
}

pub const WORKSPACE_MISSING: &str = "WORKSPACE_MISSING";

/// Structured application failure mapped to CLI exit codes and MCP errors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestFailure {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub details: Value,
}

impl RequestFailure {
    pub const WORKSPACE_MISSING: &str = WORKSPACE_MISSING;

    pub fn new(
        code: impl Into<String>,
        message: impl Into<String>,
        retryable: bool,
        details: Value,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            retryable,
            details,
        }
    }

    pub fn invalid_arguments(message: impl Into<String>) -> Self {
        Self::new("INVALID_ARGUMENTS", message, false, json!({}))
    }

    pub fn unknown_tool(name: &str, available: &[&str]) -> Self {
        Self::new(
            "UNKNOWN_TOOL",
            format!(
                "Unknown tool '{}'. Available tools: {}",
                name,
                available.join(", ")
            ),
            false,
            json!({ "tool": name, "available": available }),
        )
    }

    pub fn follower_read_only(message: impl Into<String>) -> Self {
        Self::new("FOLLOWER_READ_ONLY", message, true, json!({}))
    }

    pub fn workspace_required(message: impl Into<String>) -> Self {
        Self::new("WORKSPACE_REQUIRED", message, false, json!({}))
    }

    pub fn workspace_conflict(message: impl Into<String>) -> Self {
        Self::new("WORKSPACE_CONFLICT", message, false, json!({}))
    }

    pub fn workspace_missing(message: impl Into<String>) -> Self {
        Self::new(WORKSPACE_MISSING, message, false, json!({}))
    }

    pub fn sensitive_root(message: impl Into<String>) -> Self {
        Self::new("SENSITIVE_ROOT", message, false, json!({}))
    }

    pub fn deadline_exceeded(message: impl Into<String>) -> Self {
        Self::new("DEADLINE_EXCEEDED", message, true, json!({}))
    }

    pub fn cancelled(message: impl Into<String>) -> Self {
        Self::new("CANCELLED", message, false, json!({}))
    }

    pub fn semantics_not_ready(message: impl Into<String>, details: Value) -> Self {
        Self::new("SEMANTICS_NOT_READY", message, true, details)
    }

    pub fn foreground_required(message: impl Into<String>) -> Self {
        Self::new("FOREGROUND_REQUIRED", message, false, json!({}))
    }

    pub fn tool_error(message: impl Into<String>) -> Self {
        Self::new("TOOL_ERROR", message, false, json!({}))
    }

    pub fn stale_edit(message: impl Into<String>) -> Self {
        Self::new("STALE_EDIT", message, false, json!({}))
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new("INTERNAL_ERROR", message, false, json!({}))
    }

    pub fn exit_code(&self) -> i32 {
        match self.code.as_str() {
            "INVALID_ARGUMENTS"
            | "UNKNOWN_TOOL"
            | "WORKSPACE_REQUIRED"
            | "WORKSPACE_CONFLICT"
            | "WORKSPACE_MISSING"
            | "FOREGROUND_REQUIRED"
            | "SENSITIVE_ROOT" => 2,
            "TOOL_ERROR" => 3,
            "FOLLOWER_READ_ONLY"
            | "SEMANTICS_NOT_READY"
            | "UNAVAILABLE"
            | "BUSY"
            | "EDIT_BUSY"
            | "SOURCE_EDIT_UNAVAILABLE" => 4,
            "STALE_EDIT" | "EDIT_CONFLICT" | "EDIT_RECOVERY_CONFLICT" => 5,
            "DEADLINE_EXCEEDED" => 124,
            "CANCELLED" => 130,
            _ => 1,
        }
    }

    pub fn to_mcp_error(&self) -> rmcp::ErrorData {
        let code = match self.code.as_str() {
            "INVALID_ARGUMENTS" | "UNKNOWN_TOOL" | "WORKSPACE_REQUIRED" | "WORKSPACE_CONFLICT"
            | "WORKSPACE_MISSING" => -32602,
            _ => -32603,
        };
        rmcp::ErrorData::new(
            rmcp::model::ErrorCode(code),
            self.message.clone(),
            Some(self.details.clone()),
        )
    }
}

impl std::fmt::Display for RequestFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for RequestFailure {}

impl From<anyhow::Error> for RequestFailure {
    fn from(err: anyhow::Error) -> Self {
        Self::internal(err.to_string())
    }
}
