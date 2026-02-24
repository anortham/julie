//! Main pipeline: search → rank → expand → allocate → format

use anyhow::Result;

use super::GetContextTool;
use crate::handler::JulieServerHandler;

pub async fn run(tool: &GetContextTool, _handler: &JulieServerHandler) -> Result<String> {
    // Will be implemented in subsequent tasks
    Ok(format!(
        "get_context not yet implemented for query: {}",
        tool.query
    ))
}
