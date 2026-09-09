use crate::handler::mcp_adapter::McpAdapter;
use crate::request_engine::RequestEngine;
use rmcp::transport::streamable_http_server::session::never::NeverSessionManager;
use rmcp::transport::streamable_http_server::tower::{
    StreamableHttpServerConfig, StreamableHttpService,
};
use std::sync::Arc;

pub fn mcp_service(
    engine: Arc<RequestEngine>,
) -> StreamableHttpService<McpAdapter, NeverSessionManager> {
    let config = StreamableHttpServerConfig::default()
        .with_legacy_session_mode(false)
        .with_json_response(true)
        .with_allowed_hosts(["127.0.0.1", "localhost"]);
    StreamableHttpService::new(
        move || Ok(McpAdapter::new(Arc::clone(&engine), None)),
        Arc::new(NeverSessionManager::default()),
        config,
    )
}
