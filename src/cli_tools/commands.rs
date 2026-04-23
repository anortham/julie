//! `CliToolCommand` implementations for each CLI subcommand.
//!
//! These bridge CLI args into the tool execution pipeline. Each named
//! subcommand maps to an MCP tool name and produces the JSON parameters
//! for daemon-mode dispatch.
//!
//! A3 will replace the `call_standalone` stubs with real tool struct
//! construction and execution.

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

    async fn call_standalone(&self, _handler: &JulieServerHandler) -> Result<CallToolResult> {
        // A3 will wire: FastSearchTool { ... }.call_tool(handler).await
        anyhow::bail!(
            "Standalone tool execution not yet wired for '{}'. \
             Use daemon mode (without --standalone) or wait for A3.",
            self.tool_name()
        )
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

    async fn call_standalone(&self, _handler: &JulieServerHandler) -> Result<CallToolResult> {
        anyhow::bail!(
            "Standalone tool execution not yet wired for '{}'. \
             Use daemon mode (without --standalone) or wait for A3.",
            self.tool_name()
        )
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

    async fn call_standalone(&self, _handler: &JulieServerHandler) -> Result<CallToolResult> {
        anyhow::bail!(
            "Standalone tool execution not yet wired for '{}'. \
             Use daemon mode (without --standalone) or wait for A3.",
            self.tool_name()
        )
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
            args["budget"] = Value::Number(budget.into());
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

    async fn call_standalone(&self, _handler: &JulieServerHandler) -> Result<CallToolResult> {
        anyhow::bail!(
            "Standalone tool execution not yet wired for '{}'. \
             Use daemon mode (without --standalone) or wait for A3.",
            self.tool_name()
        )
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
            args["files"] = Value::Array(files.iter().map(|f| Value::String(f.clone())).collect());
        }
        if let Some(ref symbols) = self.symbols {
            args["symbols"] =
                Value::Array(symbols.iter().map(|s| Value::String(s.clone())).collect());
        }
        if let Some(ref fmt) = self.format {
            args["format"] = Value::String(fmt.clone());
        }

        Ok(args)
    }

    async fn call_standalone(&self, _handler: &JulieServerHandler) -> Result<CallToolResult> {
        anyhow::bail!(
            "Standalone tool execution not yet wired for '{}'. \
             Use daemon mode (without --standalone) or wait for A3.",
            self.tool_name()
        )
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

    async fn call_standalone(&self, _handler: &JulieServerHandler) -> Result<CallToolResult> {
        anyhow::bail!(
            "Standalone tool execution not yet wired for '{}'. \
             Use daemon mode (without --standalone) or wait for A3.",
            self.tool_name()
        )
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
        // returns &'static str for the trait. The alternative is changing
        // the trait signature, but this is fine for a CLI binary that
        // exits after one invocation.
        // A3 may refine this with a proper dispatch table.
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

    async fn call_standalone(&self, _handler: &JulieServerHandler) -> Result<CallToolResult> {
        anyhow::bail!(
            "Standalone tool execution not yet wired for generic tool '{}'. \
             Use daemon mode (without --standalone) or wait for A3.",
            self.name
        )
    }
}
