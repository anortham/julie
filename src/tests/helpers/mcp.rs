/// Extracts text from MCP `CallToolResult` or `CallToolResponse` content blocks.
pub fn call_tool_result_text(result: &impl crate::mcp_compat::AsCallToolResult) -> String {
    crate::mcp_compat::call_tool_result_text(result)
}
