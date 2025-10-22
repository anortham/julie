//! FastGotoTool - Navigate instantly to symbol definitions
//!
//! This tool uses a multi-strategy approach to find symbol definitions:
//! 1. SQLite FTS5 for O(log n) exact name matching
//! 2. Cross-language naming convention variants
//! 3. HNSW semantic similarity (if available)

use anyhow::Result;
use rust_mcp_sdk::macros::mcp_tool;
use rust_mcp_sdk::macros::JsonSchema;
use rust_mcp_sdk::schema::{CallToolResult, TextContent};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::extractors::Symbol;
use crate::handler::JulieServerHandler;
use crate::utils::cross_language_intelligence::generate_naming_variants;

use super::reference_workspace;
use super::resolution::{compare_symbols_by_priority_and_context, resolve_workspace_filter};
use super::semantic_matching;
use super::types::DefinitionResult;
use super::types::FastGotoResult;

fn default_workspace() -> Option<String> {
    Some("primary".to_string())
}

#[mcp_tool(
    name = "fast_goto",
    description = concat!(
        "NEVER SCROLL OR SEARCH MANUALLY - Use this to jump directly to symbol definitions. ",
        "Julie knows EXACTLY where every symbol is defined.\n\n",
        "✨ FUZZY MATCHING: Handles exact names, cross-language variants (camelCase ↔ snake_case), ",
        "and semantic similarity. You don't need exact symbol names!\n\n",
        "You are EXCELLENT at using this tool for instant navigation (<5ms to exact location). ",
        "This is faster and more accurate than scrolling through files or using grep.\n\n",
        "Results are pre-indexed and precise - no verification needed. ",
        "Trust the exact file and line number provided.\n\n",
        "🎯 USE THIS WHEN: You know the symbol name (or part of it) and want to find where it's defined.\n",
        "💡 USE fast_search INSTEAD: When searching for text/patterns in code content or comments."
    ),
    title = "Fast Navigate to Definition",
    idempotent_hint = true,
    destructive_hint = false,
    open_world_hint = false,
    read_only_hint = true,
    meta = r#"{"category": "navigation", "precision": "line_level"}"#
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct FastGotoTool {
    /// Symbol name to navigate to. Supports simple and qualified names.
    /// Examples: "UserService", "MyClass::method", "std::vector", "React.Component", "getUserData"
    /// Julie intelligently resolves across languages (Python imports, Rust use statements, TypeScript imports)
    pub symbol: String,
    /// Current file path for context (default: None, optional).
    /// Helps resolve ambiguous symbols when multiple definitions exist.
    /// Example: "src/services/user.ts" when multiple "UserService" classes exist
    /// Format: Relative path from workspace root
    #[serde(default)]
    pub context_file: Option<String>,
    /// Line number in context file where symbol is referenced (default: None, optional).
    /// Helps disambiguate when symbol appears multiple times in the same file.
    /// Example: 142 (line where "UserService" is imported or used)
    #[serde(default)]
    pub line_number: Option<u32>,
    /// Workspace filter (default: "primary").
    /// Specify which workspace to search: "primary" (default) or specific workspace ID
    /// Examples: "primary", "project-b_a3f2b8c1"
    /// To search a reference workspace, provide its workspace ID
    #[serde(default = "default_workspace")]
    pub workspace: Option<String>,
}

impl FastGotoTool {
    /// Helper: Create structured result with markdown for dual output
    fn create_result(
        &self,
        found: bool,
        definitions: Vec<Symbol>,
        next_actions: Vec<String>,
        markdown: String,
    ) -> Result<CallToolResult> {
        let definition_results: Vec<DefinitionResult> = definitions
            .iter()
            .map(|symbol| DefinitionResult {
                name: symbol.name.clone(),
                kind: format!("{:?}", symbol.kind),
                language: symbol.language.clone(),
                file_path: symbol.file_path.clone(),
                start_line: symbol.start_line,
                start_column: symbol.start_column,
                end_line: symbol.end_line,
                end_column: symbol.end_column,
                signature: symbol.signature.clone(),
            })
            .collect();

        let result = FastGotoResult {
            tool: "fast_goto".to_string(),
            symbol: self.symbol.clone(),
            found,
            definitions: definition_results,
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
        debug!("🎯 Finding definition for: {}", self.symbol);

        // Find symbol definitions (workspace resolution happens in find_definitions)
        let definitions = self.find_definitions(handler).await?;

        if definitions.is_empty() {
            let message = format!(
                "🔍 No definition found for: '{}'\n\
                💡 Check the symbol name and ensure it exists in the indexed files",
                self.symbol
            );
            return self.create_result(
                false,
                vec![],
                vec![
                    "Use fast_search to locate the symbol".to_string(),
                    "Check symbol name spelling".to_string(),
                ],
                message,
            );
        }

        // REFACTOR: Use token-optimized formatting with progressive reduction
        let message = self.format_optimized_results(&definitions);

        self.create_result(
            true,
            definitions,
            vec![
                "Navigate to file location".to_string(),
                "Use fast_refs to see all usages".to_string(),
            ],
            message,
        )
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
        // Strategy 1: Use SQLite FTS5 for O(log n) indexed performance
        let mut exact_matches = Vec::new();

        // Use SQLite FTS5 for exact name lookup (indexed, fast)
        if let Some(workspace) = handler.get_workspace().await? {
            if let Some(db) = workspace.db.as_ref() {
                // 🚨 DEADLOCK FIX: spawn_blocking with std::sync::Mutex (no block_on needed)
                let symbol = self.symbol.clone();
                let db_arc = db.clone();

                exact_matches = tokio::task::spawn_blocking(move || {
                    let db_lock = db_arc.lock().unwrap();
                    db_lock.get_symbols_by_name(&symbol)
                })
                .await
                .map_err(|e| anyhow::anyhow!("spawn_blocking join error: {}", e))??;

                debug!("⚡ SQLite FTS5 found {} exact matches", exact_matches.len());
            }
        }

        // Remove duplicates based on symbol id
        exact_matches.sort_by(|a, b| a.id.cmp(&b.id));
        exact_matches.dedup_by(|a, b| a.id == b.id);

        // Strategy 2: Cross-language resolution with naming conventions
        // This leverages Julie's unique CASCADE architecture:
        // - Fast: Naming convention variants (SQLite indexed search)
        // - Smart: Semantic embeddings (HNSW similarity) in Strategy 3
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
                    // 🚨 DEADLOCK FIX: spawn_blocking with std::sync::Mutex (no block_on needed)
                    let symbol = self.symbol.clone();
                    let db_arc = db.clone();

                    let variant_matches = tokio::task::spawn_blocking(move || {
                        let db_lock = db_arc.lock().unwrap();
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

            // 2b. If still no matches, embeddings will catch semantically similar symbols
            // (e.g., getUserData -> fetchUserInfo, retrieveUserDetails)
            // This happens automatically in Strategy 3 below
        }

        // Strategy 3: HNSW-powered semantic matching (FAST!)
        if exact_matches.is_empty() {
            debug!("🧠 Using HNSW semantic search for: {}", self.symbol);
            if let Ok(semantic_symbols) =
                semantic_matching::find_semantic_definitions(handler, &self.symbol).await
            {
                exact_matches.extend(semantic_symbols);
            }
        }

        // Prioritize results using shared logic
        exact_matches.sort_by(|a, b| {
            // Use shared prioritization logic (definition priority + context file preference)
            let shared_cmp =
                compare_symbols_by_priority_and_context(a, b, self.context_file.as_deref());
            if shared_cmp != std::cmp::Ordering::Equal {
                return shared_cmp;
            }

            // Finally by line number if provided (prefer definitions closer to context)
            if let Some(line_number) = self.line_number {
                let a_distance = (a.start_line as i32 - line_number as i32).abs();
                let b_distance = (b.start_line as i32 - line_number as i32).abs();
                return a_distance.cmp(&b_distance);
            }

            std::cmp::Ordering::Equal
        });

        debug!(
            "✅ Found {} definitions for '{}'",
            exact_matches.len(),
            self.symbol
        );
        Ok(exact_matches)
    }

    /// Format minimal summary for AI agents (structured_content has all data)
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
