//! Cross-Language Call Path Tracing - Julie's Killer Feature
//!
//! This tool traces execution flow across multiple programming languages using:
//! 1. Direct relationship analysis from the symbol database
//! 2. Naming convention variants for cross-language bridging
//!
//! This is Julie's unique differentiator - NO other tool can trace calls
//! across language boundaries in polyglot codebases.

pub mod cross_language;
pub mod formatting;
pub mod tracing;
pub mod types;

use anyhow::{Result, anyhow};
use schemars::JsonSchema;
use crate::mcp_compat::{CallToolResult, CallToolResultExt};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use crate::handler::JulieServerHandler;

pub use types::CallPathNode;

/// Lock the database mutex, recovering from poisoning if necessary.
/// Centralizes the lock+recover pattern used throughout trace_call_path.
fn lock_db<'a>(db: &'a Arc<Mutex<crate::database::SymbolDatabase>>, context: &str) -> std::sync::MutexGuard<'a, crate::database::SymbolDatabase> {
    match db.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            ::tracing::warn!("Database mutex poisoned in {}, recovering: {}", context, poisoned);
            poisoned.into_inner()
        }
    }
}
use types::{default_depth, default_upstream, default_workspace};

//***************************//
//   Trace Call Path Tool    //
//***************************//

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct TraceCallPathTool {
    /// Symbol name (supports qualified names)
    pub symbol: String,
    /// Trace direction: "upstream" (default), "downstream", or "both"
    #[serde(default = "default_upstream")]
    pub direction: String,
    /// Maximum trace depth (default: 3, range: 1-10)
    #[serde(default = "default_depth")]
    pub max_depth: u32,
    /// Context file path (helps disambiguate symbols)
    #[serde(default)]
    pub context_file: Option<String>,
    /// Workspace filter: "primary" (default) or workspace ID
    #[serde(default = "default_workspace")]
    pub workspace: Option<String>,
}

impl TraceCallPathTool {
    /// Find cross-language symbols using naming variants (exposed for testing)
    pub async fn find_cross_language_symbols(
        &self,
        db: &Arc<Mutex<crate::database::SymbolDatabase>>,
        symbol: &crate::extractors::Symbol,
    ) -> Result<Vec<crate::extractors::Symbol>> {
        cross_language::find_cross_language_symbols(db, symbol).await
    }

    /// Create result with lean ASCII tree format
    fn create_result(&self, ascii_tree: String) -> Result<CallToolResult> {
        ::tracing::debug!(
            "✅ Returning lean trace results ({} chars)",
            ascii_tree.len()
        );
        Ok(CallToolResult::text_content(vec![
            crate::mcp_compat::Content::text(ascii_tree),
        ]))
    }

    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        ::tracing::info!(
            "🔍 Tracing call path: {} (direction: {}, depth: {}, cross_lang: enabled)",
            self.symbol,
            self.direction,
            self.max_depth
        );

        // Validate parameters
        if self.max_depth > 10 {
            let message = "Error: max_depth cannot exceed 10 (recommended: 5)".to_string();
            return self.create_result(message);
        }

        // Get workspace and database with workspace filtering support
        let primary_workspace = handler.get_workspace().await?.ok_or_else(|| {
            anyhow!("No workspace initialized. Run 'manage_workspace index' first")
        })?;

        // Determine target workspace and load appropriate database
        let db = match self.workspace.as_deref() {
            Some("primary") | None => {
                // Use primary workspace database (default)
                primary_workspace
                    .db
                    .as_ref()
                    .ok_or_else(|| anyhow!("No primary database available"))?
                    .clone()
            }
            Some(workspace_id) => {
                // Load reference workspace database
                let ref_db_path = primary_workspace.workspace_db_path(workspace_id);
                if !ref_db_path.exists() {
                    let message = format!(
                        "Reference workspace database not found: {}\nCheck workspace ID with 'manage_workspace list'",
                        workspace_id
                    );
                    return self.create_result(message);
                }

                ::tracing::debug!("📂 Opening reference workspace DB: {:?}", ref_db_path);

                // Open reference workspace database in blocking task
                let ref_db = tokio::task::spawn_blocking(move || {
                    crate::database::SymbolDatabase::new(&ref_db_path)
                })
                .await
                .map_err(|e| anyhow!("Failed to spawn database task: {}", e))??;

                Arc::new(Mutex::new(ref_db))
            }
        };

        // Find the starting symbol(s) - wrap in block to ensure mutex guard is dropped
        let mut starting_symbols = {
            let db_lock = lock_db(&db, "trace_call_path symbol lookup");
            db_lock.get_symbols_by_name(&self.symbol)?
        }; // Guard dropped here automatically

        if starting_symbols.is_empty() {
            let message = format!(
                "Symbol not found: '{}'\nTry fast_search to find the symbol, or check spelling",
                self.symbol
            );
            return self.create_result(message);
        }

        // If context file provided, filter to symbols in that file
        if let Some(ref context_file) = self.context_file {
            starting_symbols.retain(|s| s.file_path == *context_file);
            if starting_symbols.is_empty() {
                let message = format!(
                    "Symbol '{}' not found in file: {} (try without context_file to search all files)",
                    self.symbol, context_file
                );
                return self.create_result(message);
            }
        }

        // Build call path tree
        let mut visited = HashSet::new();
        let mut all_trees = Vec::new();

        for starting_symbol in &starting_symbols {
            let call_tree = match self.direction.as_str() {
                "upstream" => {
                    tracing::trace_upstream(
                        &db,
                        starting_symbol,
                        0,
                        &mut visited,
                        self.max_depth,
                    )
                    .await?
                }
                "downstream" => {
                    tracing::trace_downstream(
                        &db,
                        starting_symbol,
                        0,
                        &mut visited,
                        self.max_depth,
                    )
                    .await?
                }
                "both" => {
                    // Separate visited sets per direction — a shared set starves
                    // the second direction since the starting symbol is already visited.
                    let mut upstream_visited = visited.clone();
                    let mut downstream_visited = visited.clone();
                    let mut upstream = tracing::trace_upstream(
                        &db,
                        starting_symbol,
                        0,
                        &mut upstream_visited,
                        self.max_depth,
                    )
                    .await?;
                    let downstream = tracing::trace_downstream(
                        &db,
                        starting_symbol,
                        0,
                        &mut downstream_visited,
                        self.max_depth,
                    )
                    .await?;
                    // Merge visited sets back for outer loop (multiple starting symbols)
                    visited.extend(upstream_visited);
                    visited.extend(downstream_visited);
                    upstream.extend(downstream);
                    upstream
                }
                _ => {
                    let message = format!(
                        "Invalid direction: '{}' (valid options: 'upstream', 'downstream', 'both')",
                        self.direction
                    );
                    return self.create_result(message);
                }
            };

            if !call_tree.is_empty() {
                all_trees.push((starting_symbol.clone(), call_tree));
            }
        }

        // Format output as ASCII tree
        let output = formatting::format_call_trees(
            &all_trees,
            &self.symbol,
            &self.direction,
            self.max_depth,
        )?;

        self.create_result(output)
    }
}
