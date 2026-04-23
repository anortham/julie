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

        Ok(args)
    }

    async fn call_standalone(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        use crate::tools::FastRefsTool;

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

        // Resolve --rev to file paths via git, since BlastRadiusTool's
        // from_revision/to_revision are internal database revision IDs,
        // not git rev strings.
        let mut file_paths: Vec<String> = self.files.clone().unwrap_or_default();
        if let Some(ref rev) = self.rev {
            let output = std::process::Command::new("git")
                .args(["diff", "--name-only", rev])
                .output();

            match output {
                Ok(o) if o.status.success() => {
                    let stdout = String::from_utf8_lossy(&o.stdout);
                    let rev_files: Vec<String> = stdout
                        .lines()
                        .filter(|l| !l.is_empty())
                        .map(String::from)
                        .collect();
                    if rev_files.is_empty() {
                        anyhow::bail!(
                            "No changed files found for revision '{}'. \
                             Verify the revision exists and has changes.",
                            rev
                        );
                    }
                    file_paths.extend(rev_files);
                }
                Ok(o) => {
                    let stderr = String::from_utf8_lossy(&o.stderr);
                    anyhow::bail!("git diff --name-only {} failed: {}", rev, stderr.trim());
                }
                Err(e) => {
                    anyhow::bail!(
                        "Failed to run git to resolve --rev '{}': {}. \
                         Use --files to specify file paths directly.",
                        rev,
                        e
                    );
                }
            }
        }

        if !file_paths.is_empty() {
            args["file_paths"] = Value::Array(
                file_paths
                    .iter()
                    .map(|f| Value::String(f.clone()))
                    .collect(),
            );
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

        // Resolve --rev to file paths via `git diff --name-only <rev>`.
        // BlastRadiusTool uses from_revision/to_revision (internal database
        // revision IDs), not git rev strings, so we convert the rev to
        // changed file paths instead.
        let mut file_paths = self.files.clone().unwrap_or_default();
        if let Some(ref rev) = self.rev {
            let output = std::process::Command::new("git")
                .args(["diff", "--name-only", rev])
                .output();

            match output {
                Ok(o) if o.status.success() => {
                    let stdout = String::from_utf8_lossy(&o.stdout);
                    let rev_files: Vec<String> = stdout
                        .lines()
                        .filter(|l| !l.is_empty())
                        .map(String::from)
                        .collect();
                    if rev_files.is_empty() {
                        anyhow::bail!(
                            "No changed files found for revision '{}'. \
                             Verify the revision exists and has changes.",
                            rev
                        );
                    }
                    file_paths.extend(rev_files);
                }
                Ok(o) => {
                    let stderr = String::from_utf8_lossy(&o.stderr);
                    anyhow::bail!("git diff --name-only {} failed: {}", rev, stderr.trim());
                }
                Err(e) => {
                    anyhow::bail!(
                        "Failed to run git to resolve --rev '{}': {}. \
                         Use --files to specify file paths directly.",
                        rev,
                        e
                    );
                }
            }
        }

        // Validate --symbols: BlastRadiusTool expects opaque symbol IDs (from
        // the database), not human-readable names. Provide a clear error
        // message directing users to --files or the tool subcommand.
        if let Some(ref symbols) = self.symbols {
            // Symbol IDs are numeric or UUID-like strings. If the user passed
            // something that looks like a human-readable name (contains
            // uppercase letters, colons, or is a common identifier pattern),
            // warn them and suggest alternatives.
            let looks_like_name = symbols.iter().any(|s| {
                // Real symbol IDs are hex UUIDs; names have mixed case,
                // underscores in identifier positions, etc.
                s.contains("::")
                    || s.chars().any(|c| c.is_uppercase())
                    || s.chars().all(|c| c.is_alphabetic() || c == '_')
            });

            if looks_like_name {
                anyhow::bail!(
                    "The --symbols flag expects internal symbol IDs, not human-readable names.\n\
                     Received: {}\n\n\
                     To analyze by symbol name, use:\n  \
                     julie-server search \"{}\" --target definitions\n\
                     to find the symbol, then use --files with the file path instead.\n\n\
                     To analyze by file path:\n  \
                     julie-server blast-radius --files src/path/to/file.rs",
                    symbols.join(", "),
                    symbols.first().map(|s| s.as_str()).unwrap_or("SymbolName"),
                );
            }
        }

        let tool = BlastRadiusTool {
            symbol_ids: self.symbols.clone().unwrap_or_default(),
            file_paths,
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
