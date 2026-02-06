//! FastGotoTool - Navigate instantly to symbol definitions
//!
//! This tool uses a multi-strategy approach to find symbol definitions:
//! 1. SQLite indexed lookup for O(log n) exact name matching
//! 2. Cross-language naming convention variants

use std::collections::HashMap;

use anyhow::Result;
use schemars::JsonSchema;
use crate::mcp_compat::{CallToolResult, Content, CallToolResultExt};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::extractors::{Symbol, SymbolKind};
use crate::handler::JulieServerHandler;
use tracing::warn;
use crate::utils::cross_language_intelligence::generate_naming_variants;

use super::formatting::format_lean_goto_results;
use super::reference_workspace;
use super::resolution::{compare_symbols_by_priority_and_context, parse_qualified_name, resolve_workspace_filter};

fn default_workspace() -> Option<String> {
    Some("primary".to_string())
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct FastGotoTool {
    /// Symbol name (supports qualified names like "MyClass::method")
    pub symbol: String,
    /// Context file path (relative to workspace root, helps resolve ambiguous symbols)
    #[serde(default)]
    pub context_file: Option<String>,
    /// Line number in context file (helps disambiguate)
    #[serde(default)]
    pub line_number: Option<u32>,
    /// Workspace filter: "primary" (default) or workspace ID
    #[serde(default = "default_workspace")]
    pub workspace: Option<String>,
}

impl FastGotoTool {
    /// Create lean text result for goto definitions
    fn create_result(
        &self,
        definitions: Vec<Symbol>,
        parent_names: &HashMap<String, String>,
    ) -> Result<CallToolResult> {
        let lean_output = format_lean_goto_results(&self.symbol, &definitions, parent_names);
        Ok(CallToolResult::text_content(vec![Content::text(lean_output)]))
    }

    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!("🎯 Finding definition for: {}", self.symbol);

        // Find symbol definitions (workspace resolution happens in find_definitions)
        let definitions = self.find_definitions(handler).await?;
        let empty_parents = HashMap::new();

        if definitions.is_empty() {
            return self.create_result(vec![], &empty_parents);
        }

        // Resolve parent names for enrichment
        let parent_names = self.resolve_parent_names(handler, &definitions).await;

        self.create_result(
            definitions,
            &parent_names,
        )
    }

    /// Batch-fetch parent symbol names for enriching output.
    /// Note: uses primary workspace DB only — reference workspace results will
    /// get empty parent names (graceful degradation, not an error).
    async fn resolve_parent_names(
        &self,
        handler: &JulieServerHandler,
        definitions: &[Symbol],
    ) -> HashMap<String, String> {
        let parent_ids: Vec<String> = definitions
            .iter()
            .filter_map(|s| s.parent_id.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        if parent_ids.is_empty() {
            return HashMap::new();
        }

        let result = async {
            if let Some(workspace) = handler.get_workspace().await? {
                if let Some(db) = workspace.db.as_ref() {
                    let db_arc = db.clone();
                    let ids = parent_ids;
                    let parent_symbols = tokio::task::spawn_blocking(move || {
                        let db_lock = super::lock_db(&db_arc, "fast_goto parent lookup");
                        db_lock.get_symbols_by_ids(&ids)
                    })
                    .await
                    .map_err(|e| anyhow::anyhow!("spawn_blocking join error: {}", e))??;

                    let map: HashMap<String, String> = parent_symbols
                        .into_iter()
                        .map(|s| (s.id.clone(), s.name.clone()))
                        .collect();
                    return Ok::<_, anyhow::Error>(map);
                }
            }
            Ok(HashMap::new())
        }
        .await;

        result.unwrap_or_else(|e| {
            warn!("Failed to resolve parent names: {}", e);
            HashMap::new()
        })
    }

    async fn find_definitions(&self, handler: &JulieServerHandler) -> Result<Vec<Symbol>> {
        debug!("🔍 Finding definitions for: {}", self.symbol);

        // Resolve workspace parameter
        let workspace_filter = resolve_workspace_filter(self.workspace.as_deref(), handler).await?;

        // If reference workspace is specified, open that workspace's DB and search it
        if let Some(ref_workspace_id) = workspace_filter {
            debug!("🎯 Searching reference workspace: {}", ref_workspace_id);
            return self
                .database_find_definitions_in_reference(handler, ref_workspace_id)
                .await;
        }

        // Primary workspace search - use handler.get_workspace().db

        // Strategy 0: Qualified name resolution (e.g. "MyClass::method" or "MyClass.method")
        if let Some((parent_name, child_name)) = parse_qualified_name(&self.symbol) {
            debug!(
                "🔗 Qualified name detected: parent='{}', child='{}'",
                parent_name, child_name
            );

            if let Some(workspace) = handler.get_workspace().await? {
                if let Some(db) = workspace.db.as_ref() {
                    let child = child_name.to_string();
                    let parent = parent_name.to_string();
                    let db_arc = db.clone();

                    let qualified_matches = tokio::task::spawn_blocking(move || {
                        let db_lock = super::lock_db(&db_arc, "fast_goto qualified lookup");

                        // Step 1: Find all symbols with the child name
                        let candidates = db_lock.get_symbols_by_name(&child)?;

                        // Step 2: Collect unique parent IDs
                        let parent_ids: Vec<String> = candidates
                            .iter()
                            .filter_map(|s| s.parent_id.clone())
                            .collect::<std::collections::HashSet<_>>()
                            .into_iter()
                            .collect();

                        if parent_ids.is_empty() {
                            return Ok::<Vec<Symbol>, anyhow::Error>(Vec::new());
                        }

                        // Step 3: Batch fetch parent symbols
                        let parent_symbols = db_lock.get_symbols_by_ids(&parent_ids)?;
                        let parent_name_map: std::collections::HashMap<String, String> =
                            parent_symbols
                                .into_iter()
                                .map(|s| (s.id.clone(), s.name.clone()))
                                .collect();

                        // Step 4: Filter candidates where parent name matches
                        let matches: Vec<Symbol> = candidates
                            .into_iter()
                            .filter(|s| {
                                s.parent_id
                                    .as_ref()
                                    .and_then(|pid| parent_name_map.get(pid))
                                    .map(|name| name == &parent)
                                    .unwrap_or(false)
                            })
                            .collect();

                        Ok(matches)
                    })
                    .await
                    .map_err(|e| anyhow::anyhow!("spawn_blocking join error: {}", e))??;

                    if !qualified_matches.is_empty() {
                        debug!(
                            "🎯 Qualified name resolution found {} matches",
                            qualified_matches.len()
                        );
                        // Skip normal strategies — we have precise matches
                        let mut exact_matches = qualified_matches;
                        self.sort_definitions(&mut exact_matches);
                        return Ok(exact_matches);
                    }

                    debug!("🔗 Qualified name resolution found no matches, falling through to normal resolution");
                }
            }
        }

        // Strategy 1: Use SQLite indexed lookup for O(log n) performance
        let mut exact_matches = Vec::new();

        // Use SQLite for exact name lookup (indexed, fast)
        if let Some(workspace) = handler.get_workspace().await? {
            if let Some(db) = workspace.db.as_ref() {
                // spawn_blocking to avoid blocking tokio runtime during DB I/O
                let symbol = self.symbol.clone();
                let db_arc = db.clone();

                exact_matches = tokio::task::spawn_blocking(move || {
                    let db_lock = super::lock_db(&db_arc, "fast_goto exact lookup");
                    db_lock.get_symbols_by_name(&symbol)
                })
                .await
                .map_err(|e| anyhow::anyhow!("spawn_blocking join error: {}", e))??;

                debug!("⚡ SQLite found {} exact matches", exact_matches.len());
            }
        }

        // Remove duplicates based on symbol id
        exact_matches.sort_by(|a, b| a.id.cmp(&b.id));
        exact_matches.dedup_by(|a, b| a.id == b.id);

        // Filter out imports — for goto-definition, only actual definitions matter.
        // `use crate::search::SearchIndex;` is not where SearchIndex is defined.
        exact_matches.retain(|s| s.kind != SymbolKind::Import);

        // Strategy 2: Cross-language resolution with naming conventions
        // Uses naming convention variants for cross-language search (SQLite indexed)
        if exact_matches.is_empty() {
            debug!(
                "🌍 Attempting cross-language resolution for '{}'",
                self.symbol
            );

            // 2a. Try naming convention variants (fast, works across Python/JS/C#/Rust)
            // Examples: getUserData -> get_user_data (Python), GetUserData (C#)
            // Uses Julie's Intelligence Layer for smart variant generation
            if let Ok(Some(workspace)) = handler.get_workspace().await {
                if let Some(db) = workspace.db.as_ref() {
                    // spawn_blocking to avoid blocking tokio runtime during DB I/O
                    let symbol = self.symbol.clone();
                    let db_arc = db.clone();

                    let variant_matches = tokio::task::spawn_blocking(move || {
                        let db_lock = super::lock_db(&db_arc, "fast_goto variant lookup");
                        let mut matches = Vec::new();

                        // Generate all naming convention variants using shared intelligence module
                        let variants = generate_naming_variants(&symbol);

                        for variant in variants {
                            if variant != symbol {
                                // Avoid duplicate searches
                                if let Ok(variant_symbols) = db_lock.get_symbols_by_name(&variant) {
                                    for s in variant_symbols {
                                        if s.name == variant {
                                            debug!(
                                                "🎯 Found cross-language match: {} -> {}",
                                                symbol, variant
                                            );
                                            matches.push(s);
                                        }
                                    }
                                }
                            }
                        }
                        matches
                    })
                    .await
                    .map_err(|e| anyhow::anyhow!("spawn_blocking join error: {}", e))?;

                    exact_matches.extend(variant_matches);
                }
            }

        }

        // Prioritize results using shared logic
        self.sort_definitions(&mut exact_matches);

        debug!(
            "✅ Found {} definitions for '{}'",
            exact_matches.len(),
            self.symbol
        );
        Ok(exact_matches)
    }

    /// Sort definitions by priority, context file proximity, and line distance
    fn sort_definitions(&self, defs: &mut [Symbol]) {
        defs.sort_by(|a, b| {
            let shared_cmp =
                compare_symbols_by_priority_and_context(a, b, self.context_file.as_deref());
            if shared_cmp != std::cmp::Ordering::Equal {
                return shared_cmp;
            }
            if let Some(line_number) = self.line_number {
                let a_distance = (a.start_line as i32 - line_number as i32).abs();
                let b_distance = (b.start_line as i32 - line_number as i32).abs();
                return a_distance.cmp(&b_distance);
            }
            std::cmp::Ordering::Equal
        });
    }

    /// Format lean text summary for AI agents
    pub fn format_optimized_results(&self, symbols: &[Symbol]) -> String {
        let count = symbols.len();
        let top_results: Vec<String> = symbols.iter().take(5).map(|s| s.name.clone()).collect();

        format!(
            "Found {} definitions for '{}'\n{}",
            count,
            self.symbol,
            top_results.join(", ")
        )
    }

    /// Find definitions in a reference workspace by delegating to the reference_workspace module
    async fn database_find_definitions_in_reference(
        &self,
        handler: &JulieServerHandler,
        ref_workspace_id: String,
    ) -> Result<Vec<Symbol>> {
        reference_workspace::find_definitions_in_reference_workspace(
            handler,
            ref_workspace_id,
            &self.symbol,
            self.context_file.as_deref(),
        )
        .await
    }
}
