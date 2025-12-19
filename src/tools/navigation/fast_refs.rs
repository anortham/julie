//! FastRefsTool - Find all references to a symbol
//!
//! This tool finds all usages and references across the codebase using:
//! 1. SQLite FTS5 for O(log n) exact name matching
//! 2. Cross-language naming convention variants
//! 3. HNSW semantic similarity (strict threshold 0.75 to prevent false positives)

use anyhow::Result;
use schemars::JsonSchema;
use crate::mcp_compat::{CallToolResult, Content, CallToolResultExt};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use tracing::{debug, warn};

use crate::extractors::{Relationship, Symbol};
use crate::handler::JulieServerHandler;
use crate::tools::shared::create_toonable_result;
use crate::utils::cross_language_intelligence::generate_naming_variants;

use super::formatting::format_lean_refs_results;
use super::reference_workspace;
use super::resolution::resolve_workspace_filter;
use super::semantic_matching;
use super::types::DefinitionResult;
use super::types::FastRefsResult;
use super::types::ReferenceResult;

fn default_true() -> bool {
    true
}

fn default_limit() -> u32 {
    10 // Reduced from 50 for Julie 2.0 token efficiency (80% reduction)
}

fn default_workspace() -> Option<String> {
    Some("primary".to_string())
}

fn default_output_format() -> Option<String> {
    None // None = lean format (reference list). Override with "json", "toon", or "auto"
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct FastRefsTool {
    /// Symbol name (supports qualified names)
    pub symbol: String,
    /// Include definition in results (default: true)
    #[serde(default = "default_true")]
    pub include_definition: bool,
    /// Maximum references (default: 10, range: 1-500)
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// Workspace filter: "primary" (default) or workspace ID
    #[serde(default = "default_workspace")]
    pub workspace: Option<String>,
    /// Reference kind filter: "call", "variable_ref", "type_usage", "member_access", "import"
    #[serde(default)]
    pub reference_kind: Option<String>,
    /// Output format: "lean" (default - text list), "json", "toon", or "auto"
    #[serde(default = "default_output_format")]
    pub output_format: Option<String>,
}

impl FastRefsTool {
    /// Helper: Create result with lean format as default, JSON/TOON as alternatives
    fn create_result(
        &self,
        _found: bool,
        definitions: Vec<Symbol>,
        references: Vec<Relationship>,
        next_actions: Vec<String>,
        _markdown: String,
    ) -> Result<CallToolResult> {
        // Return based on output_format - lean is default
        match self.output_format.as_deref() {
            None | Some("lean") => {
                // Lean mode (DEFAULT): Simple text list
                let lean_output = format_lean_refs_results(&self.symbol, &definitions, &references);
                debug!(
                    "✅ Returning lean refs results ({} chars, {} defs, {} refs)",
                    lean_output.len(),
                    definitions.len(),
                    references.len()
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

                let reference_results: Vec<ReferenceResult> = references
                    .iter()
                    .map(|rel| ReferenceResult {
                        from_symbol_id: rel.from_symbol_id.clone(),
                        to_symbol_id: rel.to_symbol_id.clone(),
                        kind: format!("{:?}", rel.kind),
                        file_path: rel.file_path.clone(),
                        line_number: rel.line_number,
                        confidence: rel.confidence,
                    })
                    .collect();

                let result = FastRefsResult {
                    tool: "fast_refs".to_string(),
                    symbol: self.symbol.clone(),
                    found: !definitions.is_empty() || !references.is_empty(),
                    include_definition: self.include_definition,
                    definition_count: definitions.len(),
                    reference_count: references.len(),
                    definitions: definition_results,
                    references: reference_results,
                    next_actions,
                };

                // Use shared TOON/JSON formatter
                let total_results = result.definition_count + result.reference_count;
                create_toonable_result(
                    &result,
                    &result,
                    self.output_format.as_deref(),
                    10, // Auto threshold: 10+ results use TOON
                    total_results,
                    "fast_refs",
                )
            }
            Some(unknown) => {
                // Unknown format - warn and use lean
                warn!(
                    "⚠️ Unknown output_format '{}', using lean format",
                    unknown
                );
                let lean_output = format_lean_refs_results(&self.symbol, &definitions, &references);
                Ok(CallToolResult::text_content(vec![Content::text(lean_output)]))
            }
        }
    }

    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!("🔗 Finding references for: {}", self.symbol);

        // Find references (workspace resolution happens in find_references_and_definitions)
        let (definitions, references) = self.find_references_and_definitions(handler).await?;

        if definitions.is_empty() && references.is_empty() {
            let message = format!(
                "🔍 No references found for: '{}'\n\
                💡 Check the symbol name and ensure it exists in the indexed files",
                self.symbol
            );
            return self.create_result(
                false,
                vec![],
                vec![],
                vec![
                    "Use fast_search to locate the symbol".to_string(),
                    "Check symbol name spelling".to_string(),
                ],
                message,
            );
        }

        // Use token-optimized formatting
        let message = self.format_optimized_results(&definitions, &references);

        self.create_result(
            true,
            definitions,
            references,
            vec![
                "Navigate to reference locations".to_string(),
                "Use fast_goto to see definitions".to_string(),
            ],
            message,
        )
    }

    async fn find_references_and_definitions(
        &self,
        handler: &JulieServerHandler,
    ) -> Result<(Vec<Symbol>, Vec<Relationship>)> {
        debug!(
            "🔍 Searching for references to '{}' using indexed search",
            self.symbol
        );

        // Resolve workspace parameter
        let workspace_filter = resolve_workspace_filter(self.workspace.as_deref(), handler).await?;

        // If reference workspace is specified, open that workspace's DB and search it
        if let Some(ref_workspace_id) = workspace_filter {
            debug!("🎯 Searching reference workspace: {}", ref_workspace_id);
            return self
                .database_find_references_in_reference(handler, ref_workspace_id)
                .await;
        }

        // Primary workspace search - use handler.get_workspace().db
        // Strategy 1: Use SQLite FTS5 for O(log n) indexed performance
        let mut definitions = Vec::new();

        // Use SQLite FTS5 for exact name lookup (indexed, fast)
        if let Some(workspace) = handler.get_workspace().await? {
            if let Some(db) = workspace.db.as_ref() {
                // 🚨 DEADLOCK FIX: spawn_blocking with std::sync::Mutex (no block_on needed)
                let symbol = self.symbol.clone();
                let db_arc = db.clone();

                definitions = tokio::task::spawn_blocking(move || {
                    let db_lock = match db_arc.lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => {
                            warn!(
                                "Database mutex poisoned in fast_refs (line 217), recovering: {}",
                                poisoned
                            );
                            poisoned.into_inner()
                        }
                    };
                    db_lock.get_symbols_by_name(&symbol)
                })
                .await
                .map_err(|e| anyhow::anyhow!("spawn_blocking join error: {}", e))??;

                debug!("⚡ SQLite FTS5 found {} exact matches", definitions.len());
            }
        }

        // ✨ INTELLIGENCE: Cross-language naming convention matching
        // Use our shared utility to generate variants (snake_case, camelCase, PascalCase)
        let variants = generate_naming_variants(&self.symbol);
        debug!("🔍 Cross-language search variants: {:?}", variants);

        if let Ok(Some(workspace)) = handler.get_workspace().await {
            if let Some(db) = workspace.db.as_ref() {
                // 🚨 DEADLOCK FIX: spawn_blocking with std::sync::Mutex (no block_on needed)
                let symbol = self.symbol.clone();
                let db_arc = db.clone();

                let variant_matches = tokio::task::spawn_blocking(move || {
                    let db_lock = match db_arc.lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => {
                            warn!(
                                "Database mutex poisoned in fast_refs (line 239), recovering: {}",
                                poisoned
                            );
                            poisoned.into_inner()
                        }
                    };
                    let mut matches = Vec::new();

                    for variant in variants {
                        if variant != symbol {
                            // Avoid duplicate searches
                            if let Ok(variant_symbols) = db_lock.get_symbols_by_name(&variant) {
                                for s in variant_symbols {
                                    // Exact match on variant name
                                    if s.name == variant {
                                        debug!(
                                            "✨ Found cross-language match: {} (variant: {})",
                                            s.name, variant
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

                definitions.extend(variant_matches);
            }
        }

        // Remove duplicates
        definitions.sort_by(|a, b| a.id.cmp(&b.id));
        definitions.dedup_by(|a, b| a.id == b.id);

        // Strategy 2: Find direct relationships - REFERENCES TO this symbol (not FROM it)
        // PERFORMANCE FIX: Use targeted queries instead of loading ALL relationships
        // This changes from O(n) linear scan to O(k * log n) indexed queries where k = definitions.len()
        let mut references: Vec<Relationship> = Vec::new();

        if let Ok(Some(workspace)) = handler.get_workspace().await {
            if let Some(db) = workspace.db.as_ref() {
                // 🚨 DEADLOCK FIX: spawn_blocking with std::sync::Mutex (no block_on needed)
                // std::sync::Mutex can be locked directly without async runtime
                // spawn_blocking prevents blocking the tokio runtime during database I/O

                // Collect definition IDs before moving into spawn_blocking
                let definition_ids: Vec<String> =
                    definitions.iter().map(|d| d.id.clone()).collect();
                let db_arc = db.clone();

                let reference_kind_filter = self.reference_kind.clone();
                let symbol_references = tokio::task::spawn_blocking(move || {
                    let db_lock = match db_arc.lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => {
                            warn!(
                                "Database mutex poisoned in fast_refs (line 289), recovering: {}",
                                poisoned
                            );
                            poisoned.into_inner()
                        }
                    };
                    // Single batch query, optionally filtered by identifier kind
                    if let Some(kind) = reference_kind_filter {
                        db_lock.get_relationships_to_symbols_filtered_by_kind(&definition_ids, &kind)
                    } else {
                        db_lock.get_relationships_to_symbols(&definition_ids)
                    }
                })
                .await
                .map_err(|e| anyhow::anyhow!("spawn_blocking join error: {}", e))?;

                if let Ok(refs) = symbol_references {
                    references.extend(refs);
                }
            }
        }

        // ✨ INTELLIGENCE: Strategy 3 - Semantic similarity matching with strict thresholds
        // Only find HIGHLY similar symbols to prevent false positives
        let existing_def_ids: HashSet<_> = definitions.iter().map(|d| d.id.clone()).collect();
        let existing_ref_ids: HashSet<_> =
            references.iter().map(|r| r.to_symbol_id.clone()).collect();

        if let Ok((semantic_symbols, semantic_refs)) = semantic_matching::find_semantic_references(
            handler,
            &self.symbol,
            &existing_def_ids,
            &existing_ref_ids,
        )
        .await
        {
            definitions.extend(semantic_symbols);
            references.extend(semantic_refs);
        }

        // Sort references by confidence and location
        references.sort_by(|a, b| {
            // First by confidence (descending)
            let conf_cmp = b
                .confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal);
            if conf_cmp != std::cmp::Ordering::Equal {
                return conf_cmp;
            }
            // Then by file path
            let file_cmp = a.file_path.cmp(&b.file_path);
            if file_cmp != std::cmp::Ordering::Equal {
                return file_cmp;
            }
            // Finally by line number
            a.line_number.cmp(&b.line_number)
        });

        // Apply user-specified limit to prevent massive responses
        // Truncate AFTER sorting to return the top N most relevant references
        references.truncate(self.limit as usize);

        debug!(
            "✅ Found {} definitions and {} references for '{}'",
            definitions.len(),
            references.len(),
            self.symbol
        );

        Ok((definitions, references))
    }

    /// Format minimal summary for AI agents (structured_content has all data)
    pub fn format_optimized_results(
        &self,
        symbols: &[Symbol],
        relationships: &[Relationship],
    ) -> String {
        let symbol_id_to_name: HashMap<String, String> = symbols
            .iter()
            .map(|s| (s.id.clone(), s.name.clone()))
            .collect();

        let count = relationships.len();
        let top_results: Vec<String> = relationships
            .iter()
            .take(5)
            .map(|rel| {
                symbol_id_to_name
                    .get(&rel.to_symbol_id)
                    .cloned()
                    .unwrap_or_else(|| self.symbol.clone())
            })
            .collect();

        let mut unique_names: Vec<String> = Vec::new();
        for name in top_results {
            if !unique_names.contains(&name) {
                unique_names.push(name);
            }
        }

        format!(
            "Found {} references for '{}'\n{}",
            count,
            self.symbol,
            unique_names.join(", ")
        )
    }

    /// Find references in a reference workspace by delegating to the reference_workspace module
    async fn database_find_references_in_reference(
        &self,
        handler: &JulieServerHandler,
        ref_workspace_id: String,
    ) -> Result<(Vec<Symbol>, Vec<Relationship>)> {
        reference_workspace::find_references_in_reference_workspace(
            handler,
            ref_workspace_id,
            &self.symbol,
        )
        .await
    }
}
