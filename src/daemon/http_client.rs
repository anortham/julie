//! Shared HTTP client configuration for daemon MCP clients.

use std::collections::HashMap;

use anyhow::{Context, Result};
use axum::http::{HeaderName, HeaderValue};
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;

use crate::daemon::mcp_session::{
    HEADER_JULIE_VERSION, HEADER_JULIE_WORKSPACE, HEADER_JULIE_WORKSPACE_SOURCE,
};
use crate::daemon::transport::TransportEndpoint;
use crate::workspace::startup_hint::WorkspaceStartupHint;

pub(crate) fn http_client_config_for_endpoint(
    endpoint: &TransportEndpoint,
    startup_hint: &WorkspaceStartupHint,
) -> Result<StreamableHttpClientTransportConfig> {
    let uri = endpoint
        .mcp_url()
        .context("daemon transport discovery did not contain an HTTP MCP URL")?;
    let mut headers = HashMap::new();
    let workspace_path = startup_hint.path.to_string_lossy();
    headers.insert(
        HeaderName::from_static(HEADER_JULIE_WORKSPACE),
        HeaderValue::from_str(workspace_path.as_ref())
            .context("workspace path is not valid as an HTTP header")?,
    );
    if let Some(source) = startup_hint.source {
        headers.insert(
            HeaderName::from_static(HEADER_JULIE_WORKSPACE_SOURCE),
            HeaderValue::from_static(source.as_header_value()),
        );
    }
    headers.insert(
        HeaderName::from_static(HEADER_JULIE_VERSION),
        HeaderValue::from_static(env!("CARGO_PKG_VERSION")),
    );

    let mut config = StreamableHttpClientTransportConfig::with_uri(uri).custom_headers(headers);
    if let Some(token_path) = endpoint.token_path() {
        let token = std::fs::read_to_string(token_path).with_context(|| {
            format!("Failed to read HTTP MCP token at {}", token_path.display())
        })?;
        let token = token.trim();
        if token.is_empty() || token.contains('\r') || token.contains('\n') {
            anyhow::bail!("HTTP MCP token file is empty or malformed");
        }
        config = config.auth_header(token.to_string());
    }
    Ok(config)
}
