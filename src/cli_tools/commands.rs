//! `CliToolCommand` implementations for each CLI subcommand.
//!
//! These bridge CLI args into the tool execution pipeline. Each named
//! subcommand maps to an MCP tool name and produces normalized JSON parameters
//! for `RequestEngine::execute`.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Map, Value};

use crate::request_engine::RequestFailure;

use super::CliToolCommand;
use super::subcommands::{
    BlastRadiusArgs, CallPathArgs, ContextArgs, DeepDiveArgs, EditArgs, GenericToolArgs,
    PatternsArgs, RefsArgs, RenameArgs, RewriteArgs, SearchArgs, SymbolsArgs, WorkspaceArgs,
};

fn resolve_git_diff_file_paths(rev: &str) -> Result<Vec<String>> {
    let output = std::process::Command::new("git")
        .args(["diff", "--name-only", rev])
        .output();

    match output {
        Ok(o) if o.status.success() => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            let rev_files: Vec<String> = stdout
                .lines()
                .filter(|line| !line.is_empty())
                .map(String::from)
                .collect();
            if rev_files.is_empty() {
                anyhow::bail!(
                    "No changed files found for revision '{rev}'. Verify the revision exists and has changes."
                );
            }
            Ok(rev_files)
        }
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            anyhow::bail!("git diff --name-only {rev} failed: {}", stderr.trim());
        }
        Err(e) => anyhow::bail!(
            "Failed to run git to resolve --rev '{rev}': {e}. Use --files to specify file paths directly."
        ),
    }
}

fn validate_blast_radius_symbol_ids(symbols: &[String]) -> Result<()> {
    let looks_like_name = symbols.iter().any(|symbol| {
        symbol.contains("::")
            || symbol.chars().any(|c| c.is_uppercase())
            || symbol.chars().all(|c| c.is_alphabetic() || c == '_')
    });

    if looks_like_name {
        anyhow::bail!(
            "The --symbols flag expects internal symbol IDs, not human-readable names. Received: {}. Use --files to analyze by file path.",
            symbols.join(", ")
        );
    }
    Ok(())
}

fn build_blast_radius_tool_args(args: &BlastRadiusArgs) -> Result<Value> {
    let mut tool_args = serde_json::json!({});
    let mut file_paths = args.files.clone().unwrap_or_default();

    if let Some(ref rev) = args.rev {
        file_paths.extend(resolve_git_diff_file_paths(rev)?);
    }

    if !file_paths.is_empty() {
        tool_args["file_paths"] = Value::Array(file_paths.into_iter().map(Value::String).collect());
    }

    if let Some(ref symbols) = args.symbols {
        validate_blast_radius_symbol_ids(symbols)?;
        tool_args["symbol_ids"] =
            Value::Array(symbols.iter().cloned().map(Value::String).collect());
    }

    if let Some(ref fmt) = args.report_format {
        tool_args["format"] = Value::String(fmt.clone());
    }

    Ok(tool_args)
}

// --- search -> fast_search ---
#[async_trait]
impl CliToolCommand for SearchArgs {
    fn tool_name(&self) -> &'static str {
        "fast_search"
    }

    fn to_tool_args_map(&self) -> Result<Map<String, Value>, RequestFailure> {
        let mut map = Map::new();
        map.insert("query".into(), Value::String(self.query.clone()));
        map.insert("limit".into(), Value::Number(self.limit.into()));
        if let Some(ref lang) = self.language {
            map.insert("language".into(), Value::String(lang.clone()));
        }
        if let Some(ref fp) = self.file_pattern {
            map.insert("file_pattern".into(), Value::String(fp.clone()));
        }
        if let Some(cl) = self.context_lines {
            map.insert("context_lines".into(), Value::Number(cl.into()));
        }
        if self.exclude_tests {
            map.insert("exclude_tests".into(), Value::Bool(true));
        }
        if let Some(ref r) = self.regions {
            map.insert("regions".into(), Value::String(r.clone()));
        }
        Ok(map)
    }
}

// --- patterns ---
#[async_trait]
impl CliToolCommand for PatternsArgs {
    fn tool_name(&self) -> &'static str {
        "patterns"
    }

    fn to_tool_args_map(&self) -> Result<Map<String, Value>, RequestFailure> {
        let mut map = Map::new();
        map.insert("operation".into(), Value::String(self.operation.clone()));
        if let Some(ref id) = self.pattern_id {
            map.insert("pattern_id".into(), Value::String(id.clone()));
        }
        if let Some(ref q) = self.query {
            map.insert("query".into(), Value::String(q.clone()));
        }
        if let Some(ref p) = self.path {
            map.insert("path".into(), Value::String(p.clone()));
        }
        if let Some(ref l) = self.language {
            map.insert("language".into(), Value::String(l.clone()));
        }
        if !self.where_filters.is_empty() {
            map.insert("where".into(), Value::String(self.where_filters.join(";")));
        }
        if let Some(ref f) = self.facet {
            map.insert("facet".into(), Value::String(f.clone()));
        }
        map.insert("group_by".into(), Value::String(self.group_by.clone()));
        map.insert("limit".into(), Value::Number(self.limit.into()));
        Ok(map)
    }
}

// --- refs -> fast_refs ---
#[async_trait]
impl CliToolCommand for RefsArgs {
    fn tool_name(&self) -> &'static str {
        "fast_refs"
    }

    fn to_tool_args_map(&self) -> Result<Map<String, Value>, RequestFailure> {
        let mut map = Map::new();
        map.insert("symbol".into(), Value::String(self.symbol.clone()));
        map.insert(
            "include_definition".into(),
            Value::Bool(self.include_definition),
        );
        map.insert("limit".into(), Value::Number(self.limit.into()));
        if let Some(ref ws) = self.workspace {
            map.insert("workspace".into(), Value::String(ws.clone()));
        }
        if let Some(ref k) = self.kind {
            map.insert("reference_kind".into(), Value::String(k.clone()));
        }
        Ok(map)
    }
}

// --- symbols -> get_symbols ---
#[async_trait]
impl CliToolCommand for SymbolsArgs {
    fn tool_name(&self) -> &'static str {
        "get_symbols"
    }

    fn to_tool_args_map(&self) -> Result<Map<String, Value>, RequestFailure> {
        let mut map = Map::new();
        map.insert("file_path".into(), Value::String(self.file_path.clone()));
        map.insert("mode".into(), Value::String(self.mode.clone()));
        if let Some(ref t) = self.target {
            map.insert("target".into(), Value::String(t.clone()));
        }
        map.insert("limit".into(), Value::Number(self.limit.into()));
        map.insert("max_depth".into(), Value::Number(self.max_depth.into()));
        Ok(map)
    }
}

// --- context -> get_context ---
#[async_trait]
impl CliToolCommand for ContextArgs {
    fn tool_name(&self) -> &'static str {
        "get_context"
    }

    fn to_tool_args_map(&self) -> Result<Map<String, Value>, RequestFailure> {
        let mut map = Map::new();
        map.insert("query".into(), Value::String(self.query.clone()));
        if let Some(b) = self.budget {
            map.insert("max_tokens".into(), Value::Number(b.into()));
        }
        if let Some(h) = self.max_hops {
            map.insert("max_hops".into(), Value::Number(h.into()));
        }
        if let Some(ref es) = self.entry_symbols {
            map.insert(
                "entry_symbols".into(),
                Value::Array(es.iter().cloned().map(Value::String).collect()),
            );
        }
        if self.prefer_tests {
            map.insert("prefer_tests".into(), Value::Bool(true));
        }
        Ok(map)
    }
}

// ---------------------------------------------------------------------------
// call-path -> call_path
// ---------------------------------------------------------------------------

#[async_trait]
impl CliToolCommand for CallPathArgs {
    fn tool_name(&self) -> &'static str {
        "call_path"
    }

    fn to_tool_args_map(&self) -> Result<Map<String, Value>, RequestFailure> {
        let mut map = Map::new();
        map.insert("from".into(), Value::String(self.from.clone()));
        map.insert("to".into(), Value::String(self.to.clone()));
        map.insert("max_hops".into(), Value::Number(self.max_hops.into()));
        if let Some(ref ws) = self.workspace {
            map.insert("workspace".into(), Value::String(ws.clone()));
        }
        if let Some(ref f) = self.from_file_path {
            map.insert("from_file_path".into(), Value::String(f.clone()));
        }
        if let Some(ref t) = self.to_file_path {
            map.insert("to_file_path".into(), Value::String(t.clone()));
        }
        Ok(map)
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

    fn to_tool_args_map(&self) -> Result<Map<String, Value>, RequestFailure> {
        let val = build_blast_radius_tool_args(self)
            .map_err(|e| RequestFailure::invalid_arguments(e.to_string()))?;
        val.as_object()
            .cloned()
            .ok_or_else(|| RequestFailure::invalid_arguments("expected object"))
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

    fn to_tool_args_map(&self) -> Result<Map<String, Value>, RequestFailure> {
        let mut map = Map::new();
        map.insert("operation".into(), Value::String(self.operation.clone()));
        if let Some(ref p) = self.path {
            map.insert("path".into(), Value::String(p.clone()));
        }
        if self.force {
            map.insert("force".into(), Value::Bool(true));
        }
        if self.foreground {
            map.insert("foreground".into(), Value::Bool(true));
        }
        crate::cli_tools::recover_edit::populate_workspace_recover_args(self, &mut map);
        Ok(map)
    }

    fn validate_standalone(&self) -> Result<()> {
        match self.operation.as_str() {
            "open" | "remove" | "refresh" => {
                anyhow::bail!(
                    "Workspace operation '{}' is not available from the standalone CLI.\n\
                     Use the MCP manage_workspace tool instead.",
                    self.operation
                );
            }
            "dashboard" if !self.foreground => {
                anyhow::bail!(
                    "Workspace operation 'dashboard' via manage_workspace is not available from the standalone CLI. Run `julie-server dashboard` instead."
                );
            }
            _ => Ok(()),
        }
    }
}

// --- tool (generic) ---
#[async_trait]
impl CliToolCommand for GenericToolArgs {
    fn tool_name(&self) -> &'static str {
        Box::leak(self.name.clone().into_boxed_str())
    }

    fn to_tool_args_map(&self) -> Result<Map<String, Value>, RequestFailure> {
        let mut map = self.resolve_params()?;
        if self.foreground {
            map.insert("foreground".into(), Value::Bool(true));
        }
        Ok(map)
    }

    fn to_tool_args(&self) -> Result<Value> {
        let raw = self.params.as_deref().unwrap_or("{}");
        let parsed: Value = serde_json::from_str(raw)
            .map_err(|e| anyhow::anyhow!("Invalid JSON in --params: {e}"))?;
        if !parsed.is_object() {
            anyhow::bail!("--params must be a JSON object, got: {parsed}");
        }
        Ok(parsed)
    }

    fn validate_standalone(&self) -> Result<()> {
        if self.name == "manage_workspace" {
            let map = self.resolve_params().map_err(|e| anyhow::anyhow!("{e}"))?;
            if map.get("operation").and_then(|v| v.as_str()) == Some("dashboard")
                && !self.foreground
            {
                anyhow::bail!(
                    "Workspace operation 'dashboard' via manage_workspace is not available without foreground mode. Run `julie-server dashboard` or add --foreground."
                );
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// deep-dive -> deep_dive
// ---------------------------------------------------------------------------

#[async_trait]
impl CliToolCommand for DeepDiveArgs {
    fn tool_name(&self) -> &'static str {
        "deep_dive"
    }

    fn to_tool_args_map(&self) -> Result<Map<String, Value>, RequestFailure> {
        let mut map = Map::new();
        map.insert("symbol".into(), Value::String(self.symbol.clone()));
        if let Some(ref d) = self.depth {
            map.insert("depth".into(), Value::String(d.clone()));
        }
        if let Some(ref c) = self.context_file {
            map.insert("context_file".into(), Value::String(c.clone()));
        }
        if let Some(ref ws) = self.workspace {
            map.insert("workspace".into(), Value::String(ws.clone()));
        }
        Ok(map)
    }
}

// ---------------------------------------------------------------------------
// edit -> edit_file
// ---------------------------------------------------------------------------

#[async_trait]
impl CliToolCommand for EditArgs {
    fn tool_name(&self) -> &'static str {
        "edit_file"
    }

    fn to_tool_args_map(&self) -> Result<Map<String, Value>, RequestFailure> {
        let mut map = Map::new();
        map.insert("file_path".into(), Value::String(self.file_path.clone()));
        map.insert("old_text".into(), Value::String(self.old_text.clone()));
        map.insert("new_text".into(), Value::String(self.new_text.clone()));
        map.insert("dry_run".into(), Value::Bool(self.dry_run));
        if let Some(ref occ) = self.occurrence {
            map.insert("occurrence".into(), Value::String(occ.clone()));
        }
        if let Some(ref ws) = self.workspace {
            map.insert("workspace".into(), Value::String(ws.clone()));
        }
        Ok(map)
    }
}

// ---------------------------------------------------------------------------
// rename -> rename_symbol
// ---------------------------------------------------------------------------

#[async_trait]
impl CliToolCommand for RenameArgs {
    fn tool_name(&self) -> &'static str {
        "rename_symbol"
    }

    fn to_tool_args_map(&self) -> Result<Map<String, Value>, RequestFailure> {
        let mut map = Map::new();
        map.insert("old_name".into(), Value::String(self.old_name.clone()));
        map.insert("new_name".into(), Value::String(self.new_name.clone()));
        if let Some(ref s) = self.scope {
            map.insert("scope".into(), Value::String(s.clone()));
        }
        map.insert("dry_run".into(), Value::Bool(self.dry_run));
        if let Some(ref ws) = self.workspace {
            map.insert("workspace".into(), Value::String(ws.clone()));
        }
        Ok(map)
    }
}

// ---------------------------------------------------------------------------
// rewrite -> rewrite_symbol
// ---------------------------------------------------------------------------

#[async_trait]
impl CliToolCommand for RewriteArgs {
    fn tool_name(&self) -> &'static str {
        "rewrite_symbol"
    }

    fn to_tool_args_map(&self) -> Result<Map<String, Value>, RequestFailure> {
        let mut map = Map::new();
        map.insert("symbol".into(), Value::String(self.symbol.clone()));
        map.insert("operation".into(), Value::String(self.operation.clone()));
        map.insert("content".into(), Value::String(self.content.clone()));
        if let Some(ref f) = self.file_path {
            map.insert("file_path".into(), Value::String(f.clone()));
        }
        map.insert("dry_run".into(), Value::Bool(self.dry_run));
        if let Some(ref ws) = self.workspace {
            map.insert("workspace".into(), Value::String(ws.clone()));
        }
        Ok(map)
    }
}
