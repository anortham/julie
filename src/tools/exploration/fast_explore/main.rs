use anyhow::Result;
use rust_mcp_sdk::macros::{mcp_tool, JsonSchema};
use rust_mcp_sdk::schema::{CallToolResult, TextContent};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::extractors::base::Relationship;
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
                // For "all" mode, combine insights from multiple modes
                let mut combined = String::new();
                combined.push_str(&self.intelligent_overview(handler).await?);
                combined.push_str("\n\n");
                combined.push_str(&self.intelligent_hotspots(handler).await?);
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

        let workspace_id =
            crate::workspace::registry::generate_workspace_id(&workspace.root.to_string_lossy())?;
        let workspace_ids = vec![workspace_id];
        let (_kind_counts, language_counts) = db_lock.get_symbol_statistics(&workspace_ids)?;
        let file_counts = db_lock.get_file_statistics(&workspace_ids)?;
        let total_symbols = db_lock.get_total_symbol_count(&workspace_ids)?;

        let mut sorted_langs: Vec<_> = language_counts.iter().collect();
        sorted_langs.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
        let top_langs: Vec<String> = sorted_langs
            .iter()
            .take(5)
            .map(|(lang, _)| (*lang).clone())
            .collect();

        Ok(format!(
            "Codebase overview: {} symbols in {} files\nLanguages: {}",
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

        let workspace_id =
            crate::workspace::registry::generate_workspace_id(&workspace.root.to_string_lossy())?;
        let workspace_ids = vec![workspace_id];

        let relationship_counts = tokio::task::block_in_place(|| {
            let db_lock = db.lock().unwrap();
            db_lock.get_relationship_type_statistics(&workspace_ids)
        })?;

        let total_relationships: i64 = relationship_counts.values().sum();
        let mut sorted_rel_types: Vec<_> = relationship_counts.iter().collect();
        sorted_rel_types.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
        let top_types: Vec<String> = sorted_rel_types
            .iter()
            .take(3)
            .map(|(kind, _)| (*kind).clone())
            .collect();

        Ok(format!(
            "Dependencies: {} total relationships\nTop types: {}",
            total_relationships,
            top_types.join(", ")
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

        let workspace_id =
            crate::workspace::registry::generate_workspace_id(&workspace.root.to_string_lossy())?;
        let workspace_ids = vec![workspace_id];
        let file_symbol_counts = db_lock.get_file_statistics(&workspace_ids)?;
        let file_rel_counts = db_lock.get_file_relationship_statistics(&workspace_ids)?;

        let mut complexity_scores: Vec<(String, i64)> = Vec::new();
        for (file, symbol_count) in file_symbol_counts.iter() {
            let symbol_count_i64 = *symbol_count as i64;
            let rel_count = file_rel_counts
                .get(file)
                .copied()
                .unwrap_or(0) as i64;
            let complexity = symbol_count_i64 * (1 + rel_count);
            complexity_scores.push((file.clone(), complexity));
        }

        complexity_scores.sort_by(|a, b| b.1.cmp(&a.1));

        let top_files: Vec<String> = complexity_scores.iter()
            .take(5)
            .map(|(path, _)| {
                std::path::Path::new(path)
                    .file_name()
                    .unwrap_or_else(|| std::ffi::OsStr::new(path))
                    .to_string_lossy()
                    .to_string()
            })
            .collect();

        Ok(format!(
            "Complexity hotspots: {} files analyzed\nTop files: {}",
            file_symbol_counts.len(),
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

                    Ok(format!(
                        "Tracing '{}': {} relationships found\nIncoming: {}, Outgoing: {}",
                        focus,
                        incoming.len() + outgoing.len(),
                        incoming.len(),
                        outgoing.len()
                    ))
                } else {
                    Ok(format!("Symbol '{}' not found", focus))
                }
            } else {
                Ok(format!("Symbol '{}' not found", focus))
            }
        } else {
            Ok("No focus symbol specified\nUse focus parameter to trace a specific symbol".to_string())
        }
    }
}
