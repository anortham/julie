//! Generic tool dispatcher for the `julie-server tool <name>` subcommand.
//!
//! Maps tool names to their struct types, deserializes JSON params via serde,
//! and calls the tool through the shared `.call_tool(&handler)` path. All 12
//! public MCP tools are reachable through this dispatcher.

use anyhow::Result;
use serde_json::Value;

use crate::handler::JulieServerHandler;
use crate::mcp_compat::CallToolResult;

/// All tool names supported by the generic dispatcher, in alphabetical order.
pub const AVAILABLE_TOOLS: &[&str] = &[
    "blast_radius",
    "call_path",
    "deep_dive",
    "edit_file",
    "fast_refs",
    "fast_search",
    "get_context",
    "get_symbols",
    "manage_workspace",
    "rename_symbol",
    "rewrite_symbol",
    "spillover_get",
];

/// Dispatch a tool call by name, deserializing JSON params into the correct
/// tool struct and executing via `.call_tool(handler)`.
///
/// Returns a clear error for unknown tool names (listing all available tools)
/// and for JSON deserialization failures.
pub async fn dispatch_generic_tool(
    name: &str,
    params: Value,
    handler: &JulieServerHandler,
) -> Result<CallToolResult> {
    match name {
        "fast_search" => {
            let tool: crate::tools::FastSearchTool = deserialize_params(name, params)?;
            tool.call_tool(handler).await
        }
        "fast_refs" => {
            let tool: crate::tools::FastRefsTool = deserialize_params(name, params)?;
            tool.call_tool(handler).await
        }
        "get_symbols" => {
            let tool: crate::tools::GetSymbolsTool = deserialize_params(name, params)?;
            tool.call_tool(handler).await
        }
        "deep_dive" => {
            let tool: crate::tools::DeepDiveTool = deserialize_params(name, params)?;
            tool.call_tool(handler).await
        }
        "get_context" => {
            let tool: crate::tools::GetContextTool = deserialize_params(name, params)?;
            tool.call_tool(handler).await
        }
        "blast_radius" => {
            let tool: crate::tools::BlastRadiusTool = deserialize_params(name, params)?;
            tool.call_tool(handler).await
        }
        "call_path" => {
            let tool: crate::tools::CallPathTool = deserialize_params(name, params)?;
            tool.call_tool(handler).await
        }
        "spillover_get" => {
            let tool: crate::tools::SpilloverGetTool = deserialize_params(name, params)?;
            tool.call_tool(handler).await
        }
        "rename_symbol" => {
            let tool: crate::tools::RenameSymbolTool = deserialize_params(name, params)?;
            tool.call_tool(handler).await
        }
        "manage_workspace" => {
            let tool: crate::tools::ManageWorkspaceTool = deserialize_params(name, params)?;
            tool.call_tool(handler).await
        }
        "edit_file" => {
            let tool: crate::tools::editing::edit_file::EditFileTool =
                deserialize_params(name, params)?;
            tool.call_tool(handler).await
        }
        "rewrite_symbol" => {
            let tool: crate::tools::editing::rewrite_symbol::RewriteSymbolTool =
                deserialize_params(name, params)?;
            tool.call_tool(handler).await
        }
        _ => {
            let available = AVAILABLE_TOOLS.join(", ");
            anyhow::bail!("Unknown tool '{}'. Available tools: {}", name, available)
        }
    }
}

/// Deserialize JSON params into a tool struct, producing a clear error on failure.
fn deserialize_params<T: serde::de::DeserializeOwned>(name: &str, params: Value) -> Result<T> {
    serde_json::from_value(params).map_err(|e| {
        anyhow::anyhow!(
            "Failed to parse parameters for tool '{}': {}\n\
             Check field names and types against the tool schema.",
            name,
            e
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_available_tools_count() {
        assert_eq!(AVAILABLE_TOOLS.len(), 12, "All 12 MCP tools must be listed");
    }

    #[test]
    fn test_available_tools_sorted() {
        let mut sorted = AVAILABLE_TOOLS.to_vec();
        sorted.sort();
        assert_eq!(
            AVAILABLE_TOOLS,
            &sorted[..],
            "AVAILABLE_TOOLS should be in alphabetical order"
        );
    }

    #[test]
    fn test_unknown_tool_lists_available() {
        // Verify the error message format for unknown tools by constructing
        // the same message the dispatch function would produce.
        let name = "nonexistent_tool";
        let available = AVAILABLE_TOOLS.join(", ");
        let err_msg = format!("Unknown tool '{}'. Available tools: {}", name, available);

        assert!(err_msg.contains("nonexistent_tool"));
        assert!(err_msg.contains("fast_search"));
        assert!(err_msg.contains("deep_dive"));
        assert!(err_msg.contains("edit_file"));
        assert!(err_msg.contains("rewrite_symbol"));
        assert!(err_msg.contains("spillover_get"));
    }

    #[test]
    fn test_deserialize_params_fast_search() {
        use crate::tools::FastSearchTool;

        let params = serde_json::json!({
            "query": "test_query",
            "search_target": "definitions",
            "limit": 5
        });

        let tool: FastSearchTool = deserialize_params("fast_search", params).unwrap();
        assert_eq!(tool.query, "test_query");
        assert_eq!(tool.search_target, "definitions");
        assert_eq!(tool.limit, 5);
    }

    #[test]
    fn test_deserialize_params_fast_refs() {
        use crate::tools::FastRefsTool;

        let params = serde_json::json!({
            "symbol": "MyStruct",
            "limit": 20,
            "reference_kind": "call"
        });

        let tool: FastRefsTool = deserialize_params("fast_refs", params).unwrap();
        assert_eq!(tool.symbol, "MyStruct");
        assert_eq!(tool.limit, 20);
        assert_eq!(tool.reference_kind, Some("call".to_string()));
    }

    #[test]
    fn test_deserialize_params_get_symbols() {
        use crate::tools::GetSymbolsTool;

        let params = serde_json::json!({
            "file_path": "src/main.rs",
            "mode": "minimal",
            "target": "main"
        });

        let tool: GetSymbolsTool = deserialize_params("get_symbols", params).unwrap();
        assert_eq!(tool.file_path, "src/main.rs");
        assert_eq!(tool.mode, Some("minimal".to_string()));
        assert_eq!(tool.target, Some("main".to_string()));
    }

    #[test]
    fn test_deserialize_params_deep_dive() {
        use crate::tools::{DeepDiveDepth, DeepDiveTool};

        let params = serde_json::json!({
            "symbol": "JulieServerHandler",
            "depth": "full",
            "context_file": "src/handler.rs"
        });

        let tool: DeepDiveTool = deserialize_params("deep_dive", params).unwrap();
        assert_eq!(tool.symbol, "JulieServerHandler");
        assert_eq!(tool.depth, DeepDiveDepth::Full);
        assert_eq!(tool.context_file, Some("src/handler.rs".to_string()));
    }

    #[test]
    fn test_deserialize_params_get_context() {
        use crate::tools::GetContextTool;

        let params = serde_json::json!({
            "query": "auth middleware",
            "max_tokens": 4000,
            "max_hops": 2,
            "prefer_tests": true
        });

        let tool: GetContextTool = deserialize_params("get_context", params).unwrap();
        assert_eq!(tool.query, "auth middleware");
        assert_eq!(tool.max_tokens, Some(4000));
        assert_eq!(tool.max_hops, Some(2));
        assert_eq!(tool.prefer_tests, Some(true));
    }

    #[test]
    fn test_deserialize_params_blast_radius() {
        use crate::tools::BlastRadiusTool;

        let params = serde_json::json!({
            "file_paths": ["src/main.rs"],
            "symbol_ids": ["MyStruct"],
            "max_depth": 3
        });

        let tool: BlastRadiusTool = deserialize_params("blast_radius", params).unwrap();
        assert_eq!(tool.file_paths, vec!["src/main.rs"]);
        assert_eq!(tool.symbol_ids, vec!["MyStruct"]);
        assert_eq!(tool.max_depth, 3);
    }

    #[test]
    fn test_deserialize_params_call_path() {
        use crate::tools::CallPathTool;

        let params = serde_json::json!({
            "from": "main",
            "to": "handle_request"
        });

        let tool: CallPathTool = deserialize_params("call_path", params).unwrap();
        assert_eq!(tool.from, "main");
        assert_eq!(tool.to, "handle_request");
    }

    #[test]
    fn test_deserialize_params_rename_symbol() {
        use crate::tools::RenameSymbolTool;

        let params = serde_json::json!({
            "old_name": "foo",
            "new_name": "bar",
            "dry_run": true
        });

        let tool: RenameSymbolTool = deserialize_params("rename_symbol", params).unwrap();
        assert_eq!(tool.old_name, "foo");
        assert_eq!(tool.new_name, "bar");
        assert!(tool.dry_run);
    }

    #[test]
    fn test_deserialize_params_manage_workspace() {
        use crate::tools::ManageWorkspaceTool;

        let params = serde_json::json!({
            "operation": "stats"
        });

        let tool: ManageWorkspaceTool = deserialize_params("manage_workspace", params).unwrap();
        assert_eq!(tool.operation, "stats");
        assert_eq!(tool.path, None);
        assert_eq!(tool.force, None);
    }

    #[test]
    fn test_deserialize_params_edit_file() {
        use crate::tools::editing::edit_file::EditFileTool;

        let params = serde_json::json!({
            "file_path": "src/main.rs",
            "old_text": "fn old()",
            "new_text": "fn new()"
        });

        let tool: EditFileTool = deserialize_params("edit_file", params).unwrap();
        assert_eq!(tool.file_path, "src/main.rs");
        assert_eq!(tool.old_text, "fn old()");
        assert_eq!(tool.new_text, "fn new()");
        assert!(tool.dry_run); // default is true
    }

    #[test]
    fn test_deserialize_params_rewrite_symbol() {
        use crate::tools::editing::rewrite_symbol::RewriteSymbolTool;

        let params = serde_json::json!({
            "symbol": "MyStruct::method",
            "operation": "replace_body",
            "content": "{ return 42; }"
        });

        let tool: RewriteSymbolTool = deserialize_params("rewrite_symbol", params).unwrap();
        assert_eq!(tool.symbol, "MyStruct::method");
        assert_eq!(tool.operation, "replace_body");
        assert_eq!(tool.content, "{ return 42; }");
        assert!(tool.dry_run); // default is true
    }

    #[test]
    fn test_deserialize_params_spillover_get() {
        use crate::tools::SpilloverGetTool;

        let params = serde_json::json!({
            "spillover_handle": "abc123",
            "limit": 5
        });

        let tool: SpilloverGetTool = deserialize_params("spillover_get", params).unwrap();
        assert_eq!(tool.spillover_handle, "abc123");
        assert_eq!(tool.limit, Some(5));
    }

    #[test]
    fn test_deserialize_params_invalid_json() {
        use crate::tools::FastSearchTool;

        let params = serde_json::json!({
            "wrong_field": "test"
        });

        // FastSearchTool requires "query" field
        let result: Result<FastSearchTool> = deserialize_params("fast_search", params);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("fast_search"),
            "Error should mention tool name"
        );
        assert!(
            err.contains("Check field names"),
            "Error should guide the user"
        );
    }

    #[test]
    fn test_deserialize_params_defaults_applied() {
        use crate::tools::FastSearchTool;

        // Minimal params: only the required field
        let params = serde_json::json!({
            "query": "hello"
        });

        let tool: FastSearchTool = deserialize_params("fast_search", params).unwrap();
        assert_eq!(tool.query, "hello");
        assert_eq!(tool.search_target, "content"); // default
        assert_eq!(tool.limit, 10); // default
        assert_eq!(tool.language, None);
        assert_eq!(tool.file_pattern, None);
        assert_eq!(tool.exclude_tests, None);
    }
}
