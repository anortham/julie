//! FastGotoTool - Navigate instantly to symbol definitions
//!
//! This tool uses a multi-strategy approach to find symbol definitions:
//! 1. SQLite FTS5 for O(log n) exact name matching
//! 2. Cross-language naming convention variants
//! 3. HNSW semantic similarity (if available)

use anyhow::Result;
use schemars::JsonSchema;
use crate::mcp_compat::{CallToolResult, Content, CallToolResultExt};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::extractors::Symbol;
use crate::handler::JulieServerHandler;
use crate::tools::shared::create_toonable_result;
use crate::utils::cross_language_intelligence::generate_naming_variants;

use super::formatting::format_lean_goto_results;
use super::reference_workspace;
use super::resolution::{compare_symbols_by_priority_and_context, resolve_workspace_filter};
use super::semantic_matching;
use super::types::DefinitionResult;
use super::types::FastGotoResult;

fn default_workspace() -> Option<String> {
    Some("primary".to_string())
}

fn default_output_format() -> Option<String> {
    None // None = lean format (definition list). Override with "json", "toon", or "auto"
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
    /// Output format: "lean" (default - text list), "json", "toon", or "auto"
    #[serde(default = "default_output_format")]
    pub output_format: Option<String>,
}

impl FastGotoTool {
    /// Helper: Create result with lean format as default, JSON/TOON as alternatives
    fn create_result(
        &self,
        _found: bool,
        definitions: Vec<Symbol>,
        next_actions: Vec<String>,
        _markdown: String,
    ) -> Result<CallToolResult> {
        // Return based on output_format - lean is default
        match self.output_format.as_deref() {
            None | Some("lean") => {
                // Lean mode (DEFAULT): Simple text list of definitions
                let lean_output = format_lean_goto_results(&self.symbol, &definitions);
                debug!(
                    "✅ Returning lean goto results ({} chars, {} definitions)",
                    lean_output.len(),
                    definitions.len()
                );
                Ok(CallToolResult::text_content(vec![Content::text(lean_output)]))
            }
            Some("toon") | Some("auto") | Some("json") => {
                // Structured formats: Build full result object
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
                    found: !definitions.is_empty(),
                    definitions: definition_results,
                    next_actions,
                };

                // Use shared TOON/JSON formatter
                create_toonable_result(
                    &result,
                    &result,
                    self.output_format.as_deref(),
                    10, // Auto threshold: 10+ results use TOON
                    result.definitions.len(),
                    "fast_goto",
                )
            }
            Some(unknown) => {
                // Unknown format - warn and use lean
                warn!(
                    "⚠️ Unknown output_format '{}', using lean format",
                    unknown
                );
                let lean_output = format_lean_goto_results(&self.symbol, &definitions);
                Ok(CallToolResult::text_content(vec![Content::text(lean_output)]))
            }
        }
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
                    let db_lock = match db_arc.lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => {
                            warn!(
                                "Database mutex poisoned in fast_goto (line 184), recovering: {}",
                                poisoned
                            );
                            poisoned.into_inner()
                        }
                    };
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
                        let db_lock = match db_arc.lock() {
                            Ok(guard) => guard,
                            Err(poisoned) => {
                                warn!("Database mutex poisoned in fast_goto (line 218), recovering: {}", poisoned);
                                poisoned.into_inner()
                            }
                        };
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
