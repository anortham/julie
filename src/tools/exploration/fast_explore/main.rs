use anyhow::Result;
use rust_mcp_sdk::macros::{mcp_tool, JsonSchema};
use rust_mcp_sdk::schema::{CallToolResult, TextContent};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::handler::JulieServerHandler;

use crate::tools::exploration::types::FastExploreResult;

fn default_medium() -> String {
    "medium".to_string()
}

#[mcp_tool(
    name = "fast_explore",
    description = "UNDERSTAND FIRST - Multi-mode codebase exploration (overview/dependencies/trace/hotspots)",
    title = "Fast Codebase Architecture Explorer"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct FastExploreTool {
    /// Exploration mode to run
    /// Valid modes: "overview" (symbol counts and structure), "dependencies" (relationships between symbols), "hotspots" (files with most symbols), "all" (comprehensive analysis)
    /// Example: "overview" for quick codebase summary
    pub mode: String,
    /// Analysis depth level (controls detail amount)
    /// Valid depths: "minimal" (basic info), "medium" (balanced detail), "deep" (comprehensive analysis)
    /// Default: "medium" - good balance of detail vs. readability
    #[serde(default = "default_medium")]
    pub depth: String,
    /// Optional filter to focus analysis on specific areas
    /// Examples: "auth" to focus on authentication code, "user" for user-related symbols, "payment" for payment logic
    /// Leave empty for full codebase analysis
    #[serde(default)]
    pub focus: Option<String>,
}

impl FastExploreTool {
    /// Helper: Create structured result with markdown for dual output
    fn create_result(
        &self,
        success: bool,
        next_actions: Vec<String>,
        markdown: String,
    ) -> Result<CallToolResult> {
        let result = FastExploreResult {
            tool: "fast_explore".to_string(),
            mode: self.mode.clone(),
            depth: self.depth.clone(),
            focus: self.focus.clone(),
            success,
            next_actions,
        };

        // Serialize to JSON
        let structured = serde_json::to_value(&result)?;
        let structured_map = if let serde_json::Value::Object(map) = structured {
            map
        } else {
            return Err(anyhow::anyhow!("Expected JSON object"));
        };

        Ok(
            CallToolResult::text_content(vec![TextContent::from(markdown)])
                .with_structured_content(structured_map),
        )
    }

    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!(
            "🧭 🧠 SUPER GENIUS: Exploring codebase mode={}, focus={:?}",
            self.mode, self.focus
        );

        // 🚀 INTELLIGENT EXPLORATION - No more loading ALL symbols!
        // Each mode uses optimized queries specific to its needs

        let (message, success) = match self.mode.as_str() {
            "overview" => {
                debug!("📊 Intelligent overview mode - using SQL aggregations");
                (self.intelligent_overview(handler).await?, true)
            }
            "dependencies" => {
                debug!("🔗 Intelligent dependencies mode - using targeted queries");
                (self.intelligent_dependencies(handler).await?, true)
            }
            "hotspots" => {
                debug!("🔥 Intelligent hotspots mode - using GROUP BY aggregations");
                (self.intelligent_hotspots(handler).await?, true)
            }
            "trace" => {
                debug!("🔍 Intelligent trace mode - using focused relationship queries");
                (self.intelligent_trace(handler).await?, true)
            }
            "all" => {
                debug!("🌍 Comprehensive analysis mode");
                // For "all" mode, combine insights from all modes
                let mut combined = String::new();
                combined.push_str("## Overview\n");
                combined.push_str(&self.intelligent_overview(handler).await?);
                combined.push_str("\n\n## Dependencies\n");
                combined.push_str(&self.intelligent_dependencies(handler).await?);
                combined.push_str("\n\n## Hotspots\n");
                combined.push_str(&self.intelligent_hotspots(handler).await?);
                combined.push_str("\n\n## Trace Analysis\n");
                combined.push_str(&self.intelligent_trace(handler).await?);
                (combined, true)
            }
            _ => (
                format!(
                    "❌ Unknown exploration mode: '{}'\n\
                    💡 Supported modes: overview, dependencies, hotspots, trace, all",
                    self.mode
                ),
                false,
            ),
        };

        let next_actions = if success {
            vec![
                "Use insights to navigate to important areas".to_string(),
                "Use fast_goto or fast_refs for deeper exploration".to_string(),
            ]
        } else {
            vec!["Check mode parameter spelling".to_string()]
        };

        self.create_result(success, next_actions, message)
    }

    // Analysis methods
    async fn intelligent_overview(&self, handler: &JulieServerHandler) -> Result<String> {
        let workspace = handler
            .get_workspace()
            .await?
            .ok_or_else(|| anyhow::anyhow!("No workspace available"))?;
        let db = workspace
            .db
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No database available"))?;
        let db_lock = db.lock().unwrap();

        let (_kind_counts, mut language_counts) = db_lock.get_symbol_statistics()?;
        let mut file_counts = db_lock.get_file_statistics()?;
        let mut total_symbols = db_lock.get_total_symbol_count()?;

        // Apply focus filter if provided
        if let Some(focus) = &self.focus {
            // Filter files matching focus keyword
            file_counts.retain(|file_path, _| {
                file_path.to_lowercase().contains(&focus.to_lowercase())
            });

            // Recalculate total symbols from filtered files
            total_symbols = file_counts.values().sum();

            // Note: We don't recalculate language_counts since it would require
            // per-symbol queries. This is a limitation of the current aggregation approach.
        }

        let mut sorted_langs: Vec<_> = language_counts.iter().collect();
        sorted_langs.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
        let top_langs: Vec<String> = sorted_langs
            .iter()
            .take(5)
            .map(|(lang, _)| (*lang).clone())
            .collect();

        let focus_msg = if self.focus.is_some() {
            format!(" (filtered by focus: '{}')", self.focus.as_ref().unwrap())
        } else {
            String::new()
        };

        Ok(format!(
            "Codebase overview{}: {} symbols in {} files\nLanguages: {}",
            focus_msg,
            total_symbols,
            file_counts.len(),
            top_langs.join(", ")
        ))
    }

    async fn intelligent_dependencies(&self, handler: &JulieServerHandler) -> Result<String> {
        let workspace = handler
            .get_workspace()
            .await?
            .ok_or_else(|| anyhow::anyhow!("No workspace available"))?;
        let db = workspace
            .db
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No database available"))?;

        let _workspace_id =
            crate::workspace::registry::generate_workspace_id(&workspace.root.to_string_lossy())?;

        let relationship_counts = tokio::task::block_in_place(|| {
            let db_lock = db.lock().unwrap();
            db_lock.get_relationship_type_statistics()
        })?;

        let total_relationships: i64 = relationship_counts.values().sum();
        let mut sorted_rel_types: Vec<_> = relationship_counts.iter().collect();
        sorted_rel_types.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
        let top_types: Vec<String> = sorted_rel_types
            .iter()
            .take(3)
            .map(|(kind, _)| (*kind).clone())
            .collect();

        let focus_note = if let Some(focus) = &self.focus {
            format!("\n\nNote: For focused dependency analysis on '{}', use mode='trace' with focus parameter.", focus)
        } else {
            String::new()
        };

        Ok(format!(
            "Dependencies: {} total relationships\nTop types: {}{}",
            total_relationships,
            top_types.join(", "),
            focus_note
        ))
    }

    async fn intelligent_hotspots(&self, handler: &JulieServerHandler) -> Result<String> {
        let workspace = handler
            .get_workspace()
            .await?
            .ok_or_else(|| anyhow::anyhow!("No workspace available"))?;
        let db = workspace
            .db
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No database available"))?;
        let db_lock = db.lock().unwrap();

        let file_symbol_counts = db_lock.get_file_statistics()?;
        let file_rel_counts = db_lock.get_file_relationship_statistics()?;

        // Calculate complexity using relationship density as a multiplier
        // Density = relationships / symbols (measures interconnectedness)
        // Formula: complexity = symbols * (1.0 + density)
        //
        // Examples:
        // - 100 symbols, 10 relationships → density 0.1 → complexity 110
        // - 100 symbols, 200 relationships → density 2.0 → complexity 300
        // - 50 symbols, 100 relationships → density 2.0 → complexity 150
        let mut complexity_scores: Vec<(String, f64)> = Vec::new();
        for (file, symbol_count) in file_symbol_counts.iter() {
            let symbol_count_f64 = *symbol_count as f64;
            let rel_count = file_rel_counts.get(file).copied().unwrap_or(0) as f64;

            // Calculate relationship density (avoid division by zero)
            let density = if symbol_count_f64 > 0.0 {
                rel_count / symbol_count_f64
            } else {
                0.0
            };

            // Complexity scales with size and interconnectedness
            let complexity = symbol_count_f64 * (1.0 + density);
            complexity_scores.push((file.clone(), complexity));
        }

        // Apply focus filter if provided
        if let Some(focus) = &self.focus {
            complexity_scores.retain(|(file_path, _)| {
                file_path.to_lowercase().contains(&focus.to_lowercase())
            });
        }

        // Sort by complexity (descending) - use partial_cmp for f64
        complexity_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let top_files: Vec<String> = complexity_scores
            .iter()
            .take(5)
            .map(|(path, _)| {
                std::path::Path::new(path)
                    .file_name()
                    .unwrap_or_else(|| std::ffi::OsStr::new(path))
                    .to_string_lossy()
                    .to_string()
            })
            .collect();

        let focus_msg = if self.focus.is_some() {
            format!(" (filtered by focus: '{}')", self.focus.as_ref().unwrap())
        } else {
            String::new()
        };

        Ok(format!(
            "Complexity hotspots{}: {} files analyzed\nTop files: {}",
            focus_msg,
            complexity_scores.len(),
            top_files.join(", ")
        ))
    }

    async fn intelligent_trace(&self, handler: &JulieServerHandler) -> Result<String> {
        if let Some(focus) = &self.focus {
            let workspace = handler
                .get_workspace()
                .await?
                .ok_or_else(|| anyhow::anyhow!("No workspace available"))?;
            let db = workspace
                .db
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("No database available"))?;
            let db_lock = db.lock().unwrap();

            if let Ok(symbols) = db_lock.find_symbols_by_name(focus) {
                if let Some(target) = symbols.first() {
                    let symbol_id = &target.id;
                    let incoming = db_lock
                        .get_relationships_to_symbol(symbol_id)
                        .unwrap_or_default();
                    let outgoing = db_lock
                        .get_relationships_for_symbol(symbol_id)
                        .unwrap_or_default();

                    let mut result = format!(
                        "## Tracing Symbol: '{}' ({}:{})\\n\\n",
                        target.name, target.file_path, target.start_line
                    );

                    result.push_str(&format!(
                        "**Total Relationships**: {} ({} incoming, {} outgoing)\\n\\n",
                        incoming.len() + outgoing.len(),
                        incoming.len(),
                        outgoing.len()
                    ));

                    // Show incoming relationships (what calls/uses this symbol)
                    if !incoming.is_empty() {
                        result.push_str("### Incoming (Used By):\\n");
                        for rel in incoming.iter().take(10) {
                            if let Ok(Some(from_symbol)) = db_lock.get_symbol_by_id(&rel.from_symbol_id) {
                                result.push_str(&format!(
                                    "- **{}** ({}:{}) via {}\\n",
                                    from_symbol.name,
                                    from_symbol.file_path,
                                    from_symbol.start_line,
                                    rel.kind
                                ));
                            }
                        }
                        if incoming.len() > 10 {
                            result.push_str(&format!("... and {} more\\n", incoming.len() - 10));
                        }
                        result.push_str("\\n");
                    }

                    // Show outgoing relationships (what this symbol calls/uses)
                    if !outgoing.is_empty() {
                        result.push_str("### Outgoing (Uses):\\n");
                        for rel in outgoing.iter().take(10) {
                            if let Ok(Some(to_symbol)) = db_lock.get_symbol_by_id(&rel.to_symbol_id) {
                                result.push_str(&format!(
                                    "- **{}** ({}:{}) via {}\\n",
                                    to_symbol.name,
                                    to_symbol.file_path,
                                    to_symbol.start_line,
                                    rel.kind
                                ));
                            }
                        }
                        if outgoing.len() > 10 {
                            result.push_str(&format!("... and {} more\\n", outgoing.len() - 10));
                        }
                    }

                    Ok(result)
                } else {
                    Ok(format!("Symbol '{}' not found", focus))
                }
            } else {
                Ok(format!("Symbol '{}' not found", focus))
            }
        } else {
            Ok(
                "No focus symbol specified\nUse focus parameter to trace a specific symbol"
                    .to_string(),
            )
        }
    }
}
