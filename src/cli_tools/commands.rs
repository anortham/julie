//! `CliToolCommand` implementations for each CLI subcommand.
//!
//! These bridge CLI args into the tool execution pipeline. Each named
//! subcommand maps to an MCP tool name and produces the JSON parameters
//! for daemon-mode dispatch. The `call_standalone` methods construct real
//! tool structs and execute them via `.call_tool(&handler)`.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

use crate::handler::JulieServerHandler;
use crate::mcp_compat::CallToolResult;

use super::CliToolCommand;
use super::subcommands::{
    BlastRadiusArgs, ContextArgs, GenericToolArgs, RefsArgs, SearchArgs, SymbolsArgs, WorkspaceArgs,
};

// ---------------------------------------------------------------------------
// search -> fast_search
// ---------------------------------------------------------------------------

#[async_trait]
impl CliToolCommand for SearchArgs {
    fn tool_name(&self) -> &'static str {
        "fast_search"
    }

    fn to_tool_args(&self) -> Result<Value> {
        let mut args = serde_json::json!({
            "query": self.query,
            "search_target": self.target,
            "limit": self.limit,
        });

        if let Some(ref lang) = self.language {
            args["language"] = Value::String(lang.clone());
        }
        if let Some(ref pattern) = self.file_pattern {
            args["file_pattern"] = Value::String(pattern.clone());
        }
        if let Some(lines) = self.context_lines {
            args["context_lines"] = Value::Number(lines.into());
        }
        if self.exclude_tests {
            args["exclude_tests"] = Value::Bool(true);
        }

        Ok(args)
    }

    async fn call_standalone(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        use crate::tools::search::FastSearchTool;

        let tool = FastSearchTool {
            query: self.query.clone(),
            search_target: self.target.clone(),
            limit: self.limit,
            language: self.language.clone(),
            file_pattern: self.file_pattern.clone(),
            context_lines: self.context_lines,
            exclude_tests: if self.exclude_tests { Some(true) } else { None },
            ..Default::default()
        };
        tool.call_tool(handler).await
    }
}

// ---------------------------------------------------------------------------
// refs -> fast_refs
// ---------------------------------------------------------------------------

#[async_trait]
impl CliToolCommand for RefsArgs {
    fn tool_name(&self) -> &'static str {
        "fast_refs"
    }

    fn to_tool_args(&self) -> Result<Value> {
        let mut args = serde_json::json!({
            "symbol": self.symbol,
            "limit": self.limit,
        });

        if let Some(ref kind) = self.kind {
            args["reference_kind"] = Value::String(kind.clone());
        }
        if let Some(ref path) = self.file_path {
            args["file_path"] = Value::String(path.clone());
        }
        if let Some(ref pattern) = self.file_pattern {
            args["file_pattern"] = Value::String(pattern.clone());
        }

        Ok(args)
    }

    async fn call_standalone(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        use crate::tools::FastRefsTool;

        // Note: file_path and file_pattern from CLI args are not supported by
        // FastRefsTool (they exist on RefsArgs for the daemon JSON path but
        // FastRefsTool has no matching fields). Standalone uses the struct as-is.
        let tool = FastRefsTool {
            symbol: self.symbol.clone(),
            include_definition: true,
            limit: self.limit,
            workspace: None,
            reference_kind: self.kind.clone(),
        };
        tool.call_tool(handler).await
    }
}

// ---------------------------------------------------------------------------
// symbols -> get_symbols
// ---------------------------------------------------------------------------

#[async_trait]
impl CliToolCommand for SymbolsArgs {
    fn tool_name(&self) -> &'static str {
        "get_symbols"
    }

    fn to_tool_args(&self) -> Result<Value> {
        let mut args = serde_json::json!({
            "file_path": self.file_path,
            "mode": self.mode,
            "limit": self.limit,
            "max_depth": self.max_depth,
        });

        if let Some(ref target) = self.target {
            args["target"] = Value::String(target.clone());
        }

        Ok(args)
    }

    async fn call_standalone(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        use crate::tools::symbols::GetSymbolsTool;

        let tool = GetSymbolsTool {
            file_path: self.file_path.clone(),
            max_depth: self.max_depth,
            target: self.target.clone(),
            limit: Some(self.limit),
            mode: Some(self.mode.clone()),
            workspace: None,
        };
        tool.call_tool(handler).await
    }
}

// ---------------------------------------------------------------------------
// context -> get_context
// ---------------------------------------------------------------------------

#[async_trait]
impl CliToolCommand for ContextArgs {
    fn tool_name(&self) -> &'static str {
        "get_context"
    }

    fn to_tool_args(&self) -> Result<Value> {
        let mut args = serde_json::json!({
            "query": self.query,
        });

        if let Some(budget) = self.budget {
            args["max_tokens"] = Value::Number(budget.into());
        }
        if let Some(hops) = self.max_hops {
            args["max_hops"] = Value::Number(hops.into());
        }
        if let Some(ref symbols) = self.entry_symbols {
            args["entry_symbols"] =
                Value::Array(symbols.iter().map(|s| Value::String(s.clone())).collect());
        }
        if self.prefer_tests {
            args["prefer_tests"] = Value::Bool(true);
        }

        Ok(args)
    }

    async fn call_standalone(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        use crate::tools::get_context::GetContextTool;

        let tool = GetContextTool {
            query: self.query.clone(),
            max_tokens: self.budget,
            workspace: None,
            language: None,
            file_pattern: None,
            format: None,
            edited_files: None,
            entry_symbols: self.entry_symbols.clone(),
            stack_trace: None,
            failing_test: None,
            max_hops: self.max_hops,
            prefer_tests: if self.prefer_tests { Some(true) } else { None },
        };
        tool.call_tool(handler).await
    }
}

// ---------------------------------------------------------------------------
// blast-radius -> blast_radius
// ---------------------------------------------------------------------------

#[async_trait]
impl CliToolCommand for BlastRadiusArgs {
    fn tool_name(&self) -> &'static str {
        "blast_radius"
    }

    fn to_tool_args(&self) -> Result<Value> {
        let mut args = serde_json::json!({});

        if let Some(ref rev) = self.rev {
            args["rev"] = Value::String(rev.clone());
        }
        if let Some(ref files) = self.files {
            args["file_paths"] =
                Value::Array(files.iter().map(|f| Value::String(f.clone())).collect());
        }
        if let Some(ref symbols) = self.symbols {
            args["symbol_ids"] =
                Value::Array(symbols.iter().map(|s| Value::String(s.clone())).collect());
        }
        if let Some(ref fmt) = self.format {
            args["format"] = Value::String(fmt.clone());
        }

        Ok(args)
    }

    async fn call_standalone(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        use crate::tools::impact::BlastRadiusTool;

        // Note: BlastRadiusTool uses from_revision/to_revision (database
        // revision IDs), not git rev strings. The --rev CLI flag is
        // aspirational for daemon mode; standalone maps files/symbols directly.
        let tool = BlastRadiusTool {
            symbol_ids: self.symbols.clone().unwrap_or_default(),
            file_paths: self.files.clone().unwrap_or_default(),
            from_revision: None,
            to_revision: None,
            max_depth: 2, // matches default_max_depth() in impact/mod.rs
            limit: 12,    // matches default_limit() in impact/mod.rs
            include_tests: true,
            format: self.format.clone(),
            workspace: None,
        };
        tool.call_tool(handler).await
    }
}

// ---------------------------------------------------------------------------
// workspace -> manage_workspace
// ---------------------------------------------------------------------------

#[async_trait]
impl CliToolCommand for WorkspaceArgs {
    fn tool_name(&self) -> &'static str {
        "manage_workspace"
    }

    fn to_tool_args(&self) -> Result<Value> {
        let mut args = serde_json::json!({
            "operation": self.operation,
        });

        if let Some(ref path) = self.path {
            args["path"] = Value::String(path.clone());
        }
        if self.force {
            args["force"] = Value::Bool(true);
        }
        if let Some(ref name) = self.name {
            args["name"] = Value::String(name.clone());
        }

        Ok(args)
    }

    async fn call_standalone(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        use crate::tools::workspace::commands::ManageWorkspaceTool;

        let tool = ManageWorkspaceTool {
            operation: self.operation.clone(),
            path: self.path.clone(),
            force: if self.force { Some(true) } else { None },
            name: self.name.clone(),
            workspace_id: None,
            detailed: None,
        };
        tool.call_tool(handler).await
    }
}

// ---------------------------------------------------------------------------
// tool (generic) -> any tool by name
// ---------------------------------------------------------------------------

#[async_trait]
impl CliToolCommand for GenericToolArgs {
    fn tool_name(&self) -> &'static str {
        // The generic tool command uses the user-provided name.
        // This is a lifetime workaround: we leak the string since tool_name
        // returns &'static str for the trait. Fine for a CLI binary that
        // exits after one invocation.
        Box::leak(self.name.clone().into_boxed_str())
    }

    fn to_tool_args(&self) -> Result<Value> {
        let args: Value = serde_json::from_str(&self.params).map_err(|e| {
            anyhow::anyhow!(
                "Invalid JSON in --params: {}\n\
                 Expected valid JSON object, e.g. '{{\"query\":\"test\"}}'",
                e
            )
        })?;

        if !args.is_object() {
            anyhow::bail!("Tool parameters must be a JSON object, got: {}", args);
        }

        Ok(args)
    }

    async fn call_standalone(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        let params: Value = serde_json::from_str(&self.params).map_err(|e| {
            anyhow::anyhow!(
                "Invalid JSON in --params: {}\n\
                 Expected valid JSON object, e.g. '{{\"query\":\"test\"}}'",
                e
            )
        })?;

        if !params.is_object() {
            anyhow::bail!("Tool parameters must be a JSON object, got: {}", params);
        }

        super::generic::dispatch_generic_tool(&self.name, params, handler).await
    }
}
