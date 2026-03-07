//! Deep dive tool — progressive-depth, kind-aware symbol context
//!
//! Given a symbol, returns everything an agent needs to understand it in a single call.
//! Replaces the common 3-4 tool chain of fast_search → get_symbols → fast_refs → Read.

pub mod data;
pub(crate) mod formatting;

use std::sync::Arc;

use anyhow::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::database::SymbolDatabase;
use crate::daemon_state::WorkspaceLoadStatus;
use crate::handler::JulieServerHandler;
use crate::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use crate::tools::navigation::resolution::{
    WorkspaceTarget, compare_symbols_by_priority_and_context, resolve_workspace_filter,
};

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

        // Resolve workspace parameter
        let workspace_target = resolve_workspace_filter(self.workspace.as_deref(), handler).await?;

        let symbol_name = self.symbol.clone();
        let context_file = self.context_file.clone();
        let depth_owned = depth.to_string();
        let (incoming_cap, outgoing_cap) = ref_caps(depth);

        match workspace_target {
            WorkspaceTarget::Reference(ref_workspace_id) => {
                // Reference workspace: open separate database
                let workspace = handler
                    .get_workspace()
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("No workspace initialized"))?;
                let ref_db_path = workspace.workspace_db_path(&ref_workspace_id);

                let result = tokio::task::spawn_blocking(move || -> Result<String> {
                    let db = crate::database::SymbolDatabase::new(ref_db_path)?;
                    deep_dive_query(
                        &db,
                        &symbol_name,
                        context_file.as_deref(),
                        &depth_owned,
                        incoming_cap,
                        outgoing_cap,
                    )
                })
                .await
                .map_err(|e| anyhow::anyhow!("spawn_blocking error: {}", e))??;

                return Ok(CallToolResult::text_content(vec![Content::text(result)]));
            }
            WorkspaceTarget::All => {
                return self
                    .federated_deep_dive(handler, &symbol_name, context_file.as_deref(), &depth_owned, incoming_cap, outgoing_cap)
                    .await;
            }
            WorkspaceTarget::Primary => {
                // Fall through to primary workspace logic below
            }
        }

        // Primary workspace: use shared database via Arc<Mutex>
        let workspace = handler
            .get_workspace()
            .await?
            .ok_or_else(|| anyhow::anyhow!("No workspace initialized"))?;

        let db_arc = workspace
            .db
            .as_ref()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Database not available. Run manage_workspace(operation=\"index\") first."
                )
            })?
            .clone();

        // All database work in spawn_blocking (SQLite is synchronous)
        let result = tokio::task::spawn_blocking(move || -> Result<String> {
            let db = db_arc
                .lock()
                .map_err(|e| anyhow::anyhow!("Database lock error: {}", e))?;
            deep_dive_query(
                &db,
                &symbol_name,
                context_file.as_deref(),
                &depth_owned,
                incoming_cap,
                outgoing_cap,
            )
        })
        .await
        .map_err(|e| anyhow::anyhow!("spawn_blocking error: {}", e))??;

        Ok(CallToolResult::text_content(vec![Content::text(result)]))
    }

    /// Federated deep dive: find symbol across all daemon workspaces, run deep_dive
    /// on the home project, then append cross-project callers from other projects.
    async fn federated_deep_dive(
        &self,
        handler: &JulieServerHandler,
        symbol_name: &str,
        context_file: Option<&str>,
        depth: &str,
        incoming_cap: usize,
        outgoing_cap: usize,
    ) -> Result<CallToolResult> {
        // Step 1: Get daemon state — error in stdio mode
        let daemon_state = handler.daemon_state.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "workspace=\"all\" requires daemon mode. \
                 In stdio mode, use workspace=\"primary\" or a specific workspace ID."
            )
        })?;

        // Step 2: Extract Ready workspace entries (hold read lock briefly, then drop)
        let workspace_entries: Vec<WorkspaceEntry> = {
            let state = daemon_state.read().await;
            state.workspaces.iter()
                .filter(|(_, lw)| lw.status == WorkspaceLoadStatus::Ready)
                .filter_map(|(ws_id, lw)| {
                    let db = lw.workspace.db.as_ref()?.clone();
                    let name = lw.path.file_name().and_then(|n| n.to_str())
                        .unwrap_or("unknown").to_string();
                    Some(WorkspaceEntry { workspace_id: ws_id.clone(), project_name: name, db })
                })
                .collect()
        };
        if workspace_entries.is_empty() {
            return Err(anyhow::anyhow!(
                "No Ready workspaces found in daemon. Register and index projects first."
            ));
        }

        debug!(
            "Federated deep dive: searching {} workspaces for '{}'",
            workspace_entries.len(),
            symbol_name
        );

        // Step 3: Search all workspace DBs for the symbol in parallel
        let sym_owned = symbol_name.to_string();
        let ctx_owned = context_file.map(|s| s.to_string());
        let mut join_set = tokio::task::JoinSet::new();
        for entry in &workspace_entries {
            let (db, ws_id, proj) = (entry.db.clone(), entry.workspace_id.clone(), entry.project_name.clone());
            let (sym, ctx) = (sym_owned.clone(), ctx_owned.clone());
            join_set.spawn(async move {
                tokio::task::spawn_blocking(move || {
                    let db = db.lock().map_err(|e| anyhow::anyhow!("DB lock error for {}: {}", ws_id, e))?;
                    let symbols = data::find_symbol(&db, &sym, ctx.as_deref())?;
                    Ok::<_, anyhow::Error>((ws_id, proj, symbols))
                }).await?
            });
        }
        // Collect all matches with workspace attribution
        let mut all_matches: Vec<(String, String, crate::extractors::Symbol)> = Vec::new();
        while let Some(join_result) = join_set.join_next().await {
            match join_result {
                Ok(Ok((ws_id, proj, symbols))) => {
                    for sym in symbols {
                        all_matches.push((ws_id.clone(), proj.clone(), sym));
                    }
                }
                Ok(Err(e)) => warn!("Federated deep dive: workspace search failed: {e}"),
                Err(e) => warn!("Federated deep dive: task join failed: {e}"),
            }
        }

        if all_matches.is_empty() {
            return Ok(CallToolResult::text_content(vec![Content::text(format!(
                "No symbol found: '{}' across {} project(s)\n\
                 Try fast_search(query=\"{}\", search_target=\"definitions\", workspace=\"all\") for fuzzy matching.",
                symbol_name,
                workspace_entries.len(),
                symbol_name
            ))]));
        }

        // Step 4: Pick the best match
        all_matches.sort_by(|a, b| {
            compare_symbols_by_priority_and_context(&a.2, &b.2, context_file)
        });
        let (home_ws_id, home_project_name, _best_symbol) = &all_matches[0];

        // Step 5: Run deep_dive_query against the home workspace
        let home_entry = workspace_entries
            .iter()
            .find(|e| &e.workspace_id == home_ws_id)
            .expect("home workspace must exist in entries");

        let home_db = home_entry.db.clone();
        let home_project = home_project_name.clone();
        let sym_name = symbol_name.to_string();
        let ctx_file = context_file.map(|s| s.to_string());
        let depth_owned = depth.to_string();

        let main_result = tokio::task::spawn_blocking(move || -> Result<String> {
            let db = home_db
                .lock()
                .map_err(|e| anyhow::anyhow!("DB lock error: {}", e))?;
            deep_dive_query_with_project(
                &db,
                &sym_name,
                ctx_file.as_deref(),
                &depth_owned,
                incoming_cap,
                outgoing_cap,
                &home_project,
            )
        })
        .await
        .map_err(|e| anyhow::anyhow!("spawn_blocking error: {}", e))??;

        // Step 6: Search other workspaces for cross-project callers
        let other_entries: Vec<&WorkspaceEntry> = workspace_entries
            .iter()
            .filter(|e| e.workspace_id != *home_ws_id)
            .collect();

        let cross_callers = if other_entries.is_empty() {
            Vec::new()
        } else {
            find_cross_project_callers(symbol_name, &other_entries).await
        };

        // Step 7: Format final output
        let callers_section = format_cross_project_callers(&cross_callers);
        let output = format!("{}{}", main_result, callers_section);

        Ok(CallToolResult::text_content(vec![Content::text(output)]))
    }
}

/// Pre-extracted workspace info for federated deep_dive (DB handle + metadata).
struct WorkspaceEntry {
    workspace_id: String,
    project_name: String,
    db: Arc<std::sync::Mutex<SymbolDatabase>>,
}

/// A cross-project caller entry for federated deep_dive output.
pub(crate) struct CrossProjectCaller {
    pub project_name: String,
    pub file_path: String,
    pub line_number: u32,
    pub caller_name: Option<String>,
}

/// Format cross-project callers as an appended section (empty if no callers).
pub(crate) fn format_cross_project_callers(callers: &[CrossProjectCaller]) -> String {
    if callers.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    out.push_str(&format!("\n\nCross-project callers ({}):\n", callers.len()));
    for c in callers {
        if let Some(ref name) = c.caller_name {
            out.push_str(&format!(
                "  {}:{}  {}  [project: {}]\n",
                c.file_path, c.line_number, name, c.project_name
            ));
        } else {
            out.push_str(&format!(
                "  {}:{}  [project: {}]\n",
                c.file_path, c.line_number, c.project_name
            ));
        }
    }
    out
}

/// Search other workspaces for identifiers that reference a symbol name.
async fn find_cross_project_callers(
    symbol_name: &str,
    other_entries: &[&WorkspaceEntry],
) -> Vec<CrossProjectCaller> {
    let mut join_set = tokio::task::JoinSet::new();

    for entry in other_entries {
        let db = entry.db.clone();
        let proj_name = entry.project_name.clone();
        let ws_id = entry.workspace_id.clone();
        let sym_name = symbol_name.to_string();

        join_set.spawn(async move {
            let result = tokio::task::spawn_blocking(move || -> Result<Vec<CrossProjectCaller>> {
                let db = db.lock().map_err(|e| {
                    anyhow::anyhow!("DB lock error for {}: {}", ws_id, e)
                })?;

                let names = vec![sym_name];
                let ident_refs = db.get_identifiers_by_names(&names)?;

                let mut callers = Vec::new();
                for ident in ident_refs {
                    // Enrich with containing symbol name if available
                    let caller_name = ident
                        .containing_symbol_id
                        .as_ref()
                        .and_then(|id| db.get_symbol_by_id(id).ok().flatten())
                        .map(|s| s.name);

                    callers.push(CrossProjectCaller {
                        project_name: proj_name.clone(),
                        file_path: ident.file_path,
                        line_number: ident.start_line,
                        caller_name,
                    });
                }
                Ok(callers)
            })
            .await?;
            result
        });
    }

    let mut all_callers = Vec::new();
    while let Some(join_result) = join_set.join_next().await {
        match join_result {
            Ok(Ok(callers)) => {
                all_callers.extend(callers);
            }
            Ok(Err(e)) => {
                warn!("Cross-project caller search failed: {e}");
            }
            Err(e) => {
                warn!("Cross-project caller task join failed: {e}");
            }
        }
    }
    all_callers
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
    deep_dive_query_impl(db, symbol_name, context_file, depth, incoming_cap, outgoing_cap, None)
}

/// Like `deep_dive_query` but tags the header with a project name (for federated mode).
pub(crate) fn deep_dive_query_with_project(
    db: &crate::database::SymbolDatabase,
    symbol_name: &str,
    context_file: Option<&str>,
    depth: &str,
    incoming_cap: usize,
    outgoing_cap: usize,
    project_name: &str,
) -> Result<String> {
    deep_dive_query_impl(
        db,
        symbol_name,
        context_file,
        depth,
        incoming_cap,
        outgoing_cap,
        Some(project_name),
    )
}

fn deep_dive_query_impl(
    db: &crate::database::SymbolDatabase,
    symbol_name: &str,
    context_file: Option<&str>,
    depth: &str,
    incoming_cap: usize,
    outgoing_cap: usize,
    project_name: Option<&str>,
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
        let ctx = data::build_symbol_context(db, symbol, depth, incoming_cap, outgoing_cap)?;

        // Step 3: Format with kind-aware output (optionally tagged with project name)
        let formatted = match project_name {
            Some(name) => formatting::format_symbol_context_with_project(&ctx, depth, name),
            None => formatting::format_symbol_context(&ctx, depth),
        };
        output.push_str(&formatted);

        if symbols.len() > 1 {
            output.push_str("\n---\n\n");
        }
    }

    Ok(output)
}
