//! Deep dive tool — progressive-depth, kind-aware symbol context
//!
//! Given a symbol, returns everything an agent needs to understand it in a single call.
//! Replaces the common 3-4 tool chain of fast_search → get_symbols → fast_refs → Read.

pub mod data;
pub(crate) mod formatting;

use anyhow::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::handler::JulieServerHandler;
use crate::mcp_compat::{CallToolResult, Content, CallToolResultExt};
use crate::tools::navigation::resolution::resolve_workspace_filter;

fn default_depth() -> String {
    "overview".to_string()
}

fn default_workspace() -> Option<String> {
    Some("primary".to_string())
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
/// Investigate a symbol with progressive depth. Returns definition, references, children,
/// and type info in a single call — tailored to the symbol's kind.
///
/// **Always use BEFORE modifying or extending a symbol.** Replaces the common chain of
/// fast_search → get_symbols → fast_refs → Read with a single call.
pub struct DeepDiveTool {
    /// Symbol name to investigate (supports qualified names like `Processor::process`)
    pub symbol: String,

    /// Detail level: "overview" (default, ~200 tokens), "context" (~600 tokens), "full" (~1500 tokens)
    #[serde(default = "default_depth")]
    pub depth: String,

    /// Disambiguate when multiple symbols share a name (partial file path match)
    #[serde(default)]
    pub context_file: Option<String>,

    /// Workspace filter: "primary" (default) or workspace ID
    #[serde(default = "default_workspace")]
    pub workspace: Option<String>,
}

/// Reference caps by depth level
fn ref_caps(depth: &str) -> (usize, usize) {
    match depth {
        "context" => (15, 15),
        "full" => (500, 500), // effectively uncapped
        _ => (10, 10),        // overview
    }
}

impl DeepDiveTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!("🔍 Deep dive: {} (depth: {})", self.symbol, self.depth);

        // Validate depth
        let depth = match self.depth.as_str() {
            "overview" | "context" | "full" => self.depth.as_str(),
            _ => {
                return Ok(CallToolResult::text_content(vec![Content::text(format!(
                    "Invalid depth '{}'. Must be 'overview', 'context', or 'full'.",
                    self.depth
                ))]));
            }
        };

        // Resolve workspace parameter: None = primary, Some(id) = reference
        let workspace_filter = resolve_workspace_filter(self.workspace.as_deref(), handler).await?;

        let symbol_name = self.symbol.clone();
        let context_file = self.context_file.clone();
        let depth_owned = depth.to_string();
        let (incoming_cap, outgoing_cap) = ref_caps(depth);

        if let Some(ref_workspace_id) = workspace_filter {
            // Reference workspace: open separate database
            let workspace = handler
                .get_workspace()
                .await?
                .ok_or_else(|| anyhow::anyhow!("No workspace initialized"))?;
            let ref_db_path = workspace.workspace_db_path(&ref_workspace_id);

            let result = tokio::task::spawn_blocking(move || -> Result<String> {
                let db = crate::database::SymbolDatabase::new(ref_db_path)?;
                deep_dive_query(&db, &symbol_name, context_file.as_deref(), &depth_owned, incoming_cap, outgoing_cap)
            })
            .await
            .map_err(|e| anyhow::anyhow!("spawn_blocking error: {}", e))??;

            return Ok(CallToolResult::text_content(vec![Content::text(result)]));
        }

        // Primary workspace: use shared database via Arc<Mutex>
        let workspace = handler
            .get_workspace()
            .await?
            .ok_or_else(|| anyhow::anyhow!("No workspace initialized"))?;

        let db_arc = workspace
            .db
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Database not available. Run manage_workspace(operation=\"index\") first."))?
            .clone();

        // All database work in spawn_blocking (SQLite is synchronous)
        let result = tokio::task::spawn_blocking(move || -> Result<String> {
            let db = db_arc.lock().map_err(|e| anyhow::anyhow!("Database lock error: {}", e))?;
            deep_dive_query(&db, &symbol_name, context_file.as_deref(), &depth_owned, incoming_cap, outgoing_cap)
        })
        .await
        .map_err(|e| anyhow::anyhow!("spawn_blocking error: {}", e))??;

        Ok(CallToolResult::text_content(vec![Content::text(result)]))
    }
}

/// Shared query logic for both primary and reference workspace deep dives
pub(crate) fn deep_dive_query(
    db: &crate::database::SymbolDatabase,
    symbol_name: &str,
    context_file: Option<&str>,
    depth: &str,
    incoming_cap: usize,
    outgoing_cap: usize,
) -> Result<String> {
    // Step 1: Find the symbol
    let symbols = data::find_symbol(db, symbol_name, context_file)?;

    if symbols.is_empty() {
        return Ok(format!(
            "No symbol found: '{}'\nTry fast_search(query=\"{}\", search_target=\"definitions\") for fuzzy matching.",
            symbol_name, symbol_name
        ));
    }

    // Step 2: Build context for each matching symbol
    // (usually 1, but could be multiple if name is ambiguous)
    let mut output = String::new();

    // Guard: too many matches → return compact disambiguation list instead of
    // building full context for every match (which can burn 10k+ tokens).
    const DISAMBIGUATION_THRESHOLD: usize = 5;
    if symbols.len() > DISAMBIGUATION_THRESHOLD {
        output.push_str(&format!(
            "Found {} definitions of '{}'. Use context_file to disambiguate.\n\n",
            symbols.len(),
            symbol_name
        ));
        for symbol in &symbols {
            let kind = format!("{:?}", symbol.kind).to_lowercase();
            let vis = format!("{:?}", symbol.visibility).to_lowercase();
            output.push_str(&format!(
                "  {}:{} ({}, {})\n",
                symbol.file_path, symbol.start_line, kind, vis
            ));
        }
        return Ok(output);
    }

    if symbols.len() > 1 {
        output.push_str(&format!(
            "Found {} definitions of '{}'. Use context_file to disambiguate.\n\n",
            symbols.len(),
            symbol_name
        ));
    }

    for symbol in &symbols {
        let ctx = data::build_symbol_context(
            db,
            symbol,
            depth,
            incoming_cap,
            outgoing_cap,
        )?;

        // Step 3: Format with kind-aware output
        let formatted = formatting::format_symbol_context(&ctx, depth);
        output.push_str(&formatted);

        if symbols.len() > 1 {
            output.push_str("\n---\n\n");
        }
    }

    Ok(output)
}
