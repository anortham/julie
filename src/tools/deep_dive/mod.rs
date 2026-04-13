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
use crate::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use crate::tools::navigation::resolution::{WorkspaceTarget, resolve_workspace_filter};

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
    #[serde(alias = "symbol_name")]
    pub symbol: String,

    /// Investigation depth: "overview" (default, ~200 tokens: signature + caller/callee list), "context" (~600 tokens: adds code body), "full" (~1500 tokens: all refs, test locations, bodies). Use overview for orientation, context when you need implementation details, full for complete investigation
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
        "full" => (50, 50),
        _ => (10, 10), // overview
    }
}

impl DeepDiveTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!("Deep dive: {} (depth: {})", self.symbol, self.depth);

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
                // Reference workspace: use handler helper for DB access
                let db_arc = handler
                    .get_database_for_workspace(&ref_workspace_id)
                    .await?;

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

                return Ok(CallToolResult::text_content(vec![Content::text(result)]));
            }
            WorkspaceTarget::Primary => {
                // Fall through to primary workspace logic below
            }
        }

        // Primary workspace: use the current-primary DB store, not the stale loaded one.
        let db_arc = handler.primary_database().await?;

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
    let mut output = String::new();

    // Guard: too many matches → auto-select or return compact disambiguation list
    const DISAMBIGUATION_THRESHOLD: usize = 5;
    if symbols.len() > DISAMBIGUATION_THRESHOLD {
        // Check if most results are in the same file (e.g. C++ constructor overloads)
        if let Some(selected) = auto_select_same_file_overload(db, &symbols) {
            let ctx = data::build_symbol_context(db, &selected, depth, incoming_cap, outgoing_cap)?;
            let kind = format!("{:?}", selected.kind).to_lowercase();
            output.push_str(&format!(
                "Auto-selected {} from {} definitions in {} (prefer class/struct over overloads)\n\n",
                kind,
                symbols.len(),
                selected.file_path,
            ));
            output.push_str(&formatting::format_symbol_context(&ctx, depth));
            return Ok(output);
        }

        // Results span multiple files — return compact disambiguation list
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
        let formatted = formatting::format_symbol_context(&ctx, depth);
        output.push_str(&formatted);

        if symbols.len() > 1 {
            output.push_str("\n---\n\n");
        }
    }

    Ok(output)
}

/// When the disambiguation threshold is exceeded and most results are in the same file,
/// auto-select the best match instead of asking for disambiguation.
///
/// This handles the C++ pattern where searching for "basic_json" finds the class AND
/// its 10 constructors all in the same header — the class definition is clearly the
/// intended target.
///
/// Selection priority:
/// 1. Class/Struct/Interface definition (the type itself, not constructors/methods)
/// 2. Highest centrality (reference_score) among remaining matches
/// 3. First match (fallback)
///
/// Returns None when results span multiple files (real disambiguation needed).
fn auto_select_same_file_overload(
    db: &crate::database::SymbolDatabase,
    symbols: &[crate::extractors::base::Symbol],
) -> Option<crate::extractors::base::Symbol> {
    use std::collections::HashMap;

    if symbols.is_empty() {
        return None;
    }

    // Count symbols per file to find the dominant file
    let mut file_counts: HashMap<&str, usize> = HashMap::new();
    for s in symbols {
        *file_counts.entry(&s.file_path).or_insert(0) += 1;
    }

    let (most_common_file, most_common_count) = file_counts
        .iter()
        .max_by_key(|(_, count)| **count)
        .map(|(file, count)| (*file, *count))?;

    // Only auto-select when a majority of results are in the same file
    if most_common_count <= symbols.len() / 2 {
        return None;
    }

    let same_file_symbols: Vec<&crate::extractors::base::Symbol> = symbols
        .iter()
        .filter(|s| s.file_path == most_common_file)
        .collect();

    // Priority 1: Prefer class/struct/interface (the type definition itself)
    use crate::extractors::base::SymbolKind;
    let type_defs: Vec<&&crate::extractors::base::Symbol> = same_file_symbols
        .iter()
        .filter(|s| {
            matches!(
                s.kind,
                SymbolKind::Class | SymbolKind::Struct | SymbolKind::Interface
            )
        })
        .collect();

    if type_defs.len() == 1 {
        // Exactly one type definition — that's our target
        return Some((*type_defs[0]).clone());
    }

    if type_defs.len() > 1 {
        // Multiple type defs: pick by highest centrality
        let ids: Vec<&str> = type_defs.iter().map(|s| s.id.as_str()).collect();
        if let Ok(scores) = db.get_reference_scores(&ids) {
            let best = type_defs.iter().max_by(|a, b| {
                let sa = scores.get(&a.id).copied().unwrap_or(0.0);
                let sb = scores.get(&b.id).copied().unwrap_or(0.0);
                sa.partial_cmp(&sb).unwrap_or(std::cmp::Ordering::Equal)
            });
            if let Some(selected) = best {
                return Some((**selected).clone());
            }
        }
        // Fallback: first type def
        return Some((*type_defs[0]).clone());
    }

    // Priority 2: No type definitions — pick by highest centrality among all same-file symbols
    let ids: Vec<&str> = same_file_symbols.iter().map(|s| s.id.as_str()).collect();
    if let Ok(scores) = db.get_reference_scores(&ids) {
        let best = same_file_symbols.iter().max_by(|a, b| {
            let sa = scores.get(&a.id).copied().unwrap_or(0.0);
            let sb = scores.get(&b.id).copied().unwrap_or(0.0);
            sa.partial_cmp(&sb).unwrap_or(std::cmp::Ordering::Equal)
        });
        if let Some(selected) = best {
            return Some((*selected).clone());
        }
    }

    // Priority 3: Fallback to first same-file symbol
    same_file_symbols.first().map(|s| (*s).clone())
}
