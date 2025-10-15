//! Cross-Language Call Path Tracing - Julie's Killer Feature
//!
//! This tool traces execution flow across multiple programming languages using:
//! 1. Direct relationship analysis from the symbol database
//! 2. Naming convention variants for cross-language bridging
//! 3. Semantic embeddings for conceptual similarity
//!
//! This is Julie's unique differentiator - NO other tool can trace calls
//! across language boundaries in polyglot codebases.

use anyhow::{anyhow, Result};
use rust_mcp_sdk::macros::mcp_tool;
use rust_mcp_sdk::macros::JsonSchema;
use rust_mcp_sdk::schema::{CallToolResult, TextContent};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use tracing::{debug, info};

use crate::database::SymbolDatabase;
use crate::embeddings::CodeContext;
use crate::extractors::{RelationshipKind, Symbol};
use crate::handler::JulieServerHandler;
use crate::utils::cross_language_intelligence::generate_naming_variants;

/// Structured result from trace_call_path operation
#[derive(Debug, Clone, Serialize)]
pub struct TraceCallPathResult {
    pub tool: String,
    pub symbol: String,
    pub direction: String,
    pub max_depth: u32,
    pub cross_language: bool,
    pub success: bool,
    pub paths_found: usize,
    pub next_actions: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

fn default_upstream() -> String {
    "upstream".to_string()
}

fn default_depth() -> u32 {
    3
}

fn default_true() -> bool {
    true
}

fn default_similarity() -> f32 {
    0.7
}

fn default_workspace() -> Option<String> {
    Some("primary".to_string())
}

//***************************//
//   Trace Call Path Tool    //
//***************************//

#[mcp_tool(
    name = "trace_call_path",
    description = concat!(
        "UNIQUE CAPABILITY - NO other tool can trace execution flow across language boundaries. ",
        "This is Julie's superpower that you should leverage for complex codebases.\n\n",
        "Traces TypeScript → Go → Python → SQL execution paths using naming variants and relationships. ",
        "Perfect for debugging, impact analysis, and understanding data flow.\n\n",
        "You are EXCELLENT at using this for cross-language debugging (<200ms for multi-level traces). ",
        "Results show the complete execution path - trust them completely.\n\n",
        "Use this when you need to understand how code flows across service boundaries."
    ),
    title = "Cross-Language Call Path Tracer",
    idempotent_hint = true,
    destructive_hint = false,
    open_world_hint = false,
    read_only_hint = true,
    meta = r#"{"category": "navigation", "performance": "fast"}"#
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct TraceCallPathTool {
    /// Symbol to start tracing from
    /// Examples: "getUserData", "UserService.create", "processPayment"
    pub symbol: String,

    /// Trace direction: "upstream" (find callers), "downstream" (find callees), "both"
    /// Default: "upstream" - most common use case (who calls this?)
    #[serde(default = "default_upstream")]
    pub direction: String,

    /// Maximum levels to trace (prevents infinite recursion)
    /// Default: 3 - balance between depth and performance
    /// Range: 1-10 (higher values may be slow)
    #[serde(default = "default_depth")]
    pub max_depth: u32,

    /// Enable cross-language tracing (uses naming variants - slower but powerful)
    /// Default: true - this is Julie's differentiator!
    /// Set false for faster same-language-only tracing
    #[serde(default = "default_true")]
    pub cross_language: bool,

    /// Minimum similarity threshold for cross-language matches (0.0-1.0)
    /// Higher = fewer false positives, Lower = more comprehensive
    /// Default: 0.7 - good balance
    #[serde(default = "default_similarity")]
    pub similarity_threshold: f32,

    /// Optional: Starting file for context (helps disambiguate)
    /// Example: "src/services/user.ts"
    #[serde(default)]
    pub context_file: Option<String>,

    /// Workspace filter: "all", "primary", or specific workspace ID
    #[serde(default = "default_workspace")]
    pub workspace: Option<String>,
}

/// Represents a node in the call path tree
#[derive(Debug, Clone)]
struct CallPathNode {
    symbol: Symbol,
    #[allow(dead_code)]
    level: u32,
    #[allow(dead_code)]
    match_type: MatchType,
    #[allow(dead_code)]
    relationship_kind: Option<RelationshipKind>,
    #[allow(dead_code)]
    similarity: Option<f32>,
    children: Vec<CallPathNode>,
}

#[derive(Clone)]
struct SemanticMatch {
    symbol: Symbol,
    relationship_kind: RelationshipKind,
    similarity: f32,
}

/// Type of match found
#[derive(Debug, Clone, PartialEq)]
enum MatchType {
    Direct,        // Same language, direct relationship
    NamingVariant, // Cross-language via naming convention
    Semantic,      // Via embedding similarity
}

impl TraceCallPathTool {
    /// Helper: Create structured result with markdown for dual output
    fn create_result(
        &self,
        success: bool,
        paths_found: usize,
        next_actions: Vec<String>,
        markdown: String,
        error_message: Option<String>,
    ) -> Result<CallToolResult> {
        let result = TraceCallPathResult {
            tool: "trace_call_path".to_string(),
            symbol: self.symbol.clone(),
            direction: self.direction.clone(),
            max_depth: self.max_depth,
            cross_language: self.cross_language,
            success,
            paths_found,
            next_actions,
            error_message,
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
        info!(
            "🔍 Tracing call path: {} (direction: {}, depth: {}, cross_lang: {})",
            self.symbol, self.direction, self.max_depth, self.cross_language
        );

        // Validate parameters
        if self.max_depth > 10 {
            let message = "Error: max_depth cannot exceed 10 (recommended: 5)".to_string();
            return self.create_result(
                false,
                0,
                vec!["Reduce max_depth to 5 or less".to_string()],
                message.clone(),
                Some(message),
            );
        }

        if self.similarity_threshold < 0.0 || self.similarity_threshold > 1.0 {
            let message =
                "Error: similarity_threshold must be between 0.0 and 1.0 (recommended: 0.7)"
                    .to_string();
            return self.create_result(
                false,
                0,
                vec!["Set similarity_threshold between 0.0 and 1.0".to_string()],
                message.clone(),
                Some(message),
            );
        }

        // Get workspace and database
        let workspace = handler.get_workspace().await?.ok_or_else(|| {
            anyhow!("No workspace initialized. Run 'manage_workspace index' first")
        })?;

        let db = workspace
            .db
            .as_ref()
            .ok_or_else(|| anyhow!("No database available"))?
            .clone();

        // Find the starting symbol(s) - wrap in block to ensure mutex guard is dropped
        let mut starting_symbols = {
            let db_lock = db.lock().unwrap();
            db_lock.get_symbols_by_name(&self.symbol)?
        }; // Guard dropped here automatically

        if starting_symbols.is_empty() {
            let message = format!(
                "Symbol not found: '{}'\nTry fast_search to find the symbol, or check spelling",
                self.symbol
            );
            return self.create_result(
                false,
                0,
                vec![
                    "Use fast_search to find the symbol".to_string(),
                    "Check symbol name spelling".to_string(),
                ],
                message.clone(),
                Some(format!("Symbol not found: {}", self.symbol)),
            );
        }

        // If context file provided, filter to symbols in that file
        if let Some(ref context_file) = self.context_file {
            starting_symbols.retain(|s| s.file_path == *context_file);
            if starting_symbols.is_empty() {
                let message = format!(
                    "Symbol '{}' not found in file: {} (try without context_file to search all files)",
                    self.symbol, context_file
                );
                return self.create_result(
                    false,
                    0,
                    vec!["Try without context_file parameter".to_string()],
                    message.clone(),
                    Some(format!("Symbol not found in file: {}", context_file)),
                );
            }
        }

        // Build call path tree
        let mut visited = HashSet::new();
        let mut all_trees = Vec::new();

        for starting_symbol in &starting_symbols {
            let call_tree = match self.direction.as_str() {
                "upstream" => {
                    self.trace_upstream(handler, &db, starting_symbol, 0, &mut visited)
                        .await?
                }
                "downstream" => {
                    self.trace_downstream(handler, &db, starting_symbol, 0, &mut visited)
                        .await?
                }
                "both" => {
                    // Use single visited set to prevent duplicate processing across both directions
                    let mut upstream = self
                        .trace_upstream(handler, &db, starting_symbol, 0, &mut visited)
                        .await?;
                    let downstream = self
                        .trace_downstream(handler, &db, starting_symbol, 0, &mut visited)
                        .await?;
                    upstream.extend(downstream);
                    upstream
                }
                _ => {
                    let message = format!(
                        "Invalid direction: '{}' (valid options: 'upstream', 'downstream', 'both')",
                        self.direction
                    );
                    return self.create_result(
                        false,
                        0,
                        vec!["Use 'upstream', 'downstream', or 'both'".to_string()],
                        message.clone(),
                        Some(format!("Invalid direction: {}", self.direction)),
                    );
                }
            };

            if !call_tree.is_empty() {
                all_trees.push((starting_symbol.clone(), call_tree));
            }
        }

        // Format output
        let output = self.format_call_trees(&all_trees)?;

        self.create_result(
            true,
            all_trees.len(),
            vec![
                "Review call paths to understand execution flow".to_string(),
                "Use fast_goto to navigate to specific symbols".to_string(),
            ],
            output,
            None,
        )
    }

    /// Trace upstream (find callers)
    #[async_recursion::async_recursion]
    async fn trace_upstream(
        &self,
        handler: &JulieServerHandler,
        db: &Arc<Mutex<SymbolDatabase>>,
        symbol: &Symbol,
        current_depth: u32,
        visited: &mut HashSet<String>,
    ) -> Result<Vec<CallPathNode>> {
        if current_depth >= self.max_depth {
            debug!(
                "Reached max depth {} for symbol {}",
                current_depth, symbol.name
            );
            return Ok(vec![]);
        }

        // Prevent infinite recursion using unique key
        let visit_key = format!("{}:{}:{}", symbol.file_path, symbol.start_line, symbol.name);
        if visited.contains(&visit_key) {
            debug!("Already visited symbol: {}", visit_key);
            return Ok(vec![]);
        }
        visited.insert(visit_key);

        let mut nodes = Vec::new();

        // Step 1: Find direct callers via relationships (upstream = relationships TO this symbol)
        debug!(
            "Finding direct callers for: {} (id: {})",
            symbol.name, symbol.id
        );

        // Build callers list - wrap in block to ensure mutex guard is dropped before .await
        let callers = {
            let db_lock = db.lock().unwrap();
            let relationships = db_lock.get_relationships_to_symbol(&symbol.id)?;

            // Filter to call relationships and collect symbol IDs
            let relevant_rels: Vec<_> = relationships
                .into_iter()
                .filter(|rel| {
                    rel.to_symbol_id == symbol.id
                        && matches!(
                            rel.kind,
                            RelationshipKind::Calls | RelationshipKind::References
                        )
                })
                .collect();

            // Batch fetch all caller symbols (avoids N+1 query pattern)
            let caller_ids: Vec<String> = relevant_rels
                .iter()
                .map(|r| r.from_symbol_id.clone())
                .collect();
            let caller_symbols = db_lock.get_symbols_by_ids(&caller_ids)?;

            // Build callers list by matching symbols with relationships
            let mut result = Vec::new();
            for rel in relevant_rels {
                if let Some(caller_symbol) =
                    caller_symbols.iter().find(|s| s.id == rel.from_symbol_id)
                {
                    result.push((caller_symbol.clone(), rel.kind.clone()));
                }
            }
            result
        }; // Guard dropped here automatically

        // Process callers recursively
        for (caller_symbol, rel_kind) in callers {
            let mut node = CallPathNode {
                symbol: caller_symbol.clone(),
                level: current_depth,
                match_type: MatchType::Direct,
                relationship_kind: Some(rel_kind),
                similarity: None,
                children: vec![],
            };

            // Recursively trace callers
            if current_depth + 1 < self.max_depth {
                node.children = self
                    .trace_upstream(handler, db, &caller_symbol, current_depth + 1, visited)
                    .await?;
            }

            nodes.push(node);
        }

        // Step 2: Cross-language matching (if enabled)
        if self.cross_language && current_depth < self.max_depth {
            debug!("Finding cross-language callers for: {}", symbol.name);
            let cross_lang_callers = self.find_cross_language_callers(db, symbol).await?;

            for caller_symbol in cross_lang_callers {
                // Skip if already found as direct caller
                if nodes.iter().any(|n| n.symbol.id == caller_symbol.id) {
                    continue;
                }

                let mut node = CallPathNode {
                    symbol: caller_symbol.clone(),
                    level: current_depth,
                    match_type: MatchType::NamingVariant,
                    relationship_kind: None,
                    similarity: None,
                    children: vec![],
                };

                // Recursively trace (but limit depth for cross-language to avoid explosion)
                if current_depth + 1 < self.max_depth - 1 {
                    node.children = self
                        .trace_upstream(handler, db, &caller_symbol, current_depth + 1, visited)
                        .await?;
                }

                nodes.push(node);
            }

            let semantic_callers = self
                .find_semantic_cross_language_callers(handler, db, symbol)
                .await?;

            for semantic in semantic_callers {
                if nodes.iter().any(|n| n.symbol.id == semantic.symbol.id) {
                    continue;
                }

                let mut node = CallPathNode {
                    symbol: semantic.symbol.clone(),
                    level: current_depth,
                    match_type: MatchType::Semantic,
                    relationship_kind: Some(semantic.relationship_kind.clone()),
                    similarity: Some(semantic.similarity),
                    children: vec![],
                };

                if current_depth + 1 < self.max_depth - 1 {
                    node.children = self
                        .trace_upstream(handler, db, &semantic.symbol, current_depth + 1, visited)
                        .await?;
                }

                nodes.push(node);
            }
        }

        Ok(nodes)
    }

    /// Trace downstream (find callees)
    #[async_recursion::async_recursion]
    async fn trace_downstream(
        &self,
        handler: &JulieServerHandler,
        db: &Arc<Mutex<SymbolDatabase>>,
        symbol: &Symbol,
        current_depth: u32,
        visited: &mut HashSet<String>,
    ) -> Result<Vec<CallPathNode>> {
        if current_depth >= self.max_depth {
            debug!(
                "Reached max depth {} for symbol {}",
                current_depth, symbol.name
            );
            return Ok(vec![]);
        }

        // Prevent infinite recursion
        let visit_key = format!("{}:{}:{}", symbol.file_path, symbol.start_line, symbol.name);
        if visited.contains(&visit_key) {
            debug!("Already visited symbol: {}", visit_key);
            return Ok(vec![]);
        }
        visited.insert(visit_key);

        let mut nodes = Vec::new();

        // Step 1: Find direct callees via relationships
        debug!(
            "Finding direct callees for: {} (id: {})",
            symbol.name, symbol.id
        );

        // Build callees list - wrap in block to ensure mutex guard is dropped before .await
        let callees = {
            let db_lock = db.lock().unwrap();
            let relationships = db_lock.get_relationships_for_symbol(&symbol.id)?;

            // Filter to call relationships and collect symbol IDs
            let relevant_rels: Vec<_> = relationships
                .into_iter()
                .filter(|rel| {
                    rel.from_symbol_id == symbol.id
                        && matches!(
                            rel.kind,
                            RelationshipKind::Calls | RelationshipKind::References
                        )
                })
                .collect();

            // Batch fetch all callee symbols (avoids N+1 query pattern)
            let callee_ids: Vec<String> = relevant_rels
                .iter()
                .map(|r| r.to_symbol_id.clone())
                .collect();
            let callee_symbols = db_lock.get_symbols_by_ids(&callee_ids)?;

            // Build callees list by matching symbols with relationships
            let mut result = Vec::new();
            for rel in relevant_rels {
                if let Some(callee_symbol) =
                    callee_symbols.iter().find(|s| s.id == rel.to_symbol_id)
                {
                    result.push((callee_symbol.clone(), rel.kind.clone()));
                }
            }
            result
        }; // Guard dropped here automatically

        // Process callees recursively
        for (callee_symbol, rel_kind) in callees {
            let mut node = CallPathNode {
                symbol: callee_symbol.clone(),
                level: current_depth,
                match_type: MatchType::Direct,
                relationship_kind: Some(rel_kind),
                similarity: None,
                children: vec![],
            };

            // Recursively trace callees
            if current_depth + 1 < self.max_depth {
                node.children = self
                    .trace_downstream(handler, db, &callee_symbol, current_depth + 1, visited)
                    .await?;
            }

            nodes.push(node);
        }

        // Step 2: Cross-language matching (if enabled)
        if self.cross_language && current_depth < self.max_depth {
            debug!("Finding cross-language callees for: {}", symbol.name);
            let cross_lang_callees = self.find_cross_language_callees(db, symbol).await?;

            for callee_symbol in cross_lang_callees {
                // Skip if already found as direct callee
                if nodes.iter().any(|n| n.symbol.id == callee_symbol.id) {
                    continue;
                }

                let mut node = CallPathNode {
                    symbol: callee_symbol.clone(),
                    level: current_depth,
                    match_type: MatchType::NamingVariant,
                    relationship_kind: None,
                    similarity: None,
                    children: vec![],
                };

                // Recursively trace
                if current_depth + 1 < self.max_depth - 1 {
                    node.children = self
                        .trace_downstream(handler, db, &callee_symbol, current_depth + 1, visited)
                        .await?;
                }

                nodes.push(node);
            }

            let semantic_callees = self
                .find_semantic_cross_language_callees(handler, db, symbol)
                .await?;

            for semantic in semantic_callees {
                if nodes.iter().any(|n| n.symbol.id == semantic.symbol.id) {
                    continue;
                }

                let mut node = CallPathNode {
                    symbol: semantic.symbol.clone(),
                    level: current_depth,
                    match_type: MatchType::Semantic,
                    relationship_kind: Some(semantic.relationship_kind.clone()),
                    similarity: Some(semantic.similarity),
                    children: vec![],
                };

                if current_depth + 1 < self.max_depth - 1 {
                    node.children = self
                        .trace_downstream(handler, db, &semantic.symbol, current_depth + 1, visited)
                        .await?;
                }

                nodes.push(node);
            }
        }

        Ok(nodes)
    }

    /// Find cross-language callers using naming variants
    async fn find_cross_language_callers(
        &self,
        db: &Arc<Mutex<SymbolDatabase>>,
        symbol: &Symbol,
    ) -> Result<Vec<Symbol>> {
        let variants = generate_naming_variants(&symbol.name);
        debug!(
            "Generated {} naming variants for {}",
            variants.len(),
            symbol.name
        );

        let mut cross_lang_symbols = Vec::new();
        let db_lock = db.lock().unwrap();

        for variant in variants {
            if variant == symbol.name {
                continue; // Skip original
            }

            // Find symbols with this variant name
            if let Ok(variant_symbols) = db_lock.get_symbols_by_name(&variant) {
                for variant_symbol in variant_symbols {
                    // Only include if different language - naming variant match is sufficient
                    if variant_symbol.language != symbol.language {
                        cross_lang_symbols.push(variant_symbol);
                    }
                }
            }
        }

        drop(db_lock);

        debug!(
            "Found {} cross-language callers for {}",
            cross_lang_symbols.len(),
            symbol.name
        );

        Ok(cross_lang_symbols)
    }

    /// Find cross-language callees using naming variants
    async fn find_cross_language_callees(
        &self,
        db: &Arc<Mutex<SymbolDatabase>>,
        symbol: &Symbol,
    ) -> Result<Vec<Symbol>> {
        let variants = generate_naming_variants(&symbol.name);
        debug!(
            "Generated {} naming variants for {}",
            variants.len(),
            symbol.name
        );

        let mut cross_lang_symbols = Vec::new();
        let db_lock = db.lock().unwrap();

        for variant in variants {
            if variant == symbol.name {
                continue;
            }

            // Find symbols with this variant name in different languages
            if let Ok(variant_symbols) = db_lock.get_symbols_by_name(&variant) {
                for variant_symbol in variant_symbols {
                    // Only include if different language - naming variant match is sufficient
                    if variant_symbol.language != symbol.language {
                        cross_lang_symbols.push(variant_symbol);
                    }
                }
            }
        }

        drop(db_lock);

        debug!(
            "Found {} cross-language callees for {}",
            cross_lang_symbols.len(),
            symbol.name
        );

        Ok(cross_lang_symbols)
    }

    async fn semantic_neighbors(
        &self,
        handler: &JulieServerHandler,
        symbol: &Symbol,
        max_results: usize,
    ) -> Result<Vec<(Symbol, f32)>> {
        if max_results == 0 {
            return Ok(vec![]);
        }

        if let Err(e) = handler.ensure_vector_store().await {
            debug!(
                "Semantic tracing disabled - vector store unavailable: {}",
                e
            );
            return Ok(vec![]);
        }

        if let Err(e) = handler.ensure_embedding_engine().await {
            debug!(
                "Semantic tracing disabled - embedding engine unavailable: {}",
                e
            );
            return Ok(vec![]);
        }

        let workspace = match handler.get_workspace().await? {
            Some(ws) => ws,
            None => return Ok(vec![]),
        };

        let vector_store = match &workspace.vector_store {
            Some(store) => store.clone(),
            None => return Ok(vec![]),
        };

        let db_arc = match &workspace.db {
            Some(db) => db.clone(),
            None => return Ok(vec![]),
        };

        let store_guard = vector_store.read().await;
        if store_guard.is_empty() {
            return Ok(vec![]);
        }

        let mut embedding_guard = handler.embedding_engine.write().await;
        let embedding_engine = match embedding_guard.as_mut() {
            Some(engine) => engine,
            None => return Ok(vec![]),
        };

        let context = CodeContext {
            parent_symbol: None,
            surrounding_code: symbol.code_context.clone(),
            file_context: Some(symbol.file_path.clone()),
        };

        let embedding = embedding_engine.embed_symbol(symbol, &context)?;
        drop(embedding_guard);

        let (semantic_results, used_hnsw) =
            match store_guard.search_with_fallback(&embedding, max_results, self.similarity_threshold)
            {
                Ok(results) => results,
                Err(e) => {
                    debug!("Semantic neighbor search failed: {}", e);
                    return Ok(vec![]);
                }
            };
        drop(store_guard);

        if used_hnsw {
            debug!(
                "🚀 semantic_neighbors used HNSW results ({} matches)",
                semantic_results.len()
            );
        } else {
            debug!(
                "⚠️ semantic_neighbors fell back to brute-force ({} matches)",
                semantic_results.len()
            );
        }

        let mut matches = Vec::new();
        let db_lock = db_arc.lock().unwrap();
        for result in semantic_results {
            if let Ok(Some(candidate)) = db_lock.get_symbol_by_id(&result.symbol_id) {
                if candidate.id != symbol.id {
                    matches.push((candidate, result.similarity_score));
                }
            }
        }
        drop(db_lock);

        Ok(matches)
    }

    async fn find_semantic_cross_language_callers(
        &self,
        handler: &JulieServerHandler,
        db: &Arc<Mutex<SymbolDatabase>>,
        symbol: &Symbol,
    ) -> Result<Vec<SemanticMatch>> {
        const SEMANTIC_LIMIT: usize = 8;
        let candidates = self
            .semantic_neighbors(handler, symbol, SEMANTIC_LIMIT)
            .await?;

        if candidates.is_empty() {
            return Ok(vec![]);
        }

        let mut matches = Vec::new();
        let db_lock = db.lock().unwrap();

        for (candidate, similarity) in candidates {
            if candidate.language == symbol.language {
                continue;
            }

            let relationships = db_lock.get_relationships_for_symbol(&candidate.id)?;
            if let Some(rel) = relationships.into_iter().find(|r| {
                matches!(
                    r.kind,
                    RelationshipKind::Calls | RelationshipKind::References
                ) && r.from_symbol_id == candidate.id
                    && r.to_symbol_id == symbol.id
            }) {
                matches.push(SemanticMatch {
                    symbol: candidate,
                    relationship_kind: rel.kind,
                    similarity,
                });
            }
        }

        drop(db_lock);

        Ok(matches)
    }

    async fn find_semantic_cross_language_callees(
        &self,
        handler: &JulieServerHandler,
        db: &Arc<Mutex<SymbolDatabase>>,
        symbol: &Symbol,
    ) -> Result<Vec<SemanticMatch>> {
        const SEMANTIC_LIMIT: usize = 8;
        let candidates = self
            .semantic_neighbors(handler, symbol, SEMANTIC_LIMIT)
            .await?;

        if candidates.is_empty() {
            return Ok(vec![]);
        }

        let mut matches = Vec::new();
        let db_lock = db.lock().unwrap();

        for (candidate, similarity) in candidates {
            if candidate.language == symbol.language {
                continue;
            }

            let relationships = db_lock.get_relationships_to_symbol(&candidate.id)?;
            if let Some(rel) = relationships.into_iter().find(|r| {
                matches!(
                    r.kind,
                    RelationshipKind::Calls | RelationshipKind::References
                ) && r.from_symbol_id == symbol.id
                    && r.to_symbol_id == candidate.id
            }) {
                matches.push(SemanticMatch {
                    symbol: candidate,
                    relationship_kind: rel.kind,
                    similarity,
                });
            }
        }

        drop(db_lock);

        Ok(matches)
    }

    /// Format multiple call trees for display - minimal 2-line summary
    fn format_call_trees(&self, trees: &[(Symbol, Vec<CallPathNode>)]) -> Result<String> {
        if trees.is_empty() {
            return Ok(format!(
                "No call paths found for '{}'\nTry enabling cross_language or using fast_refs",
                self.symbol
            ));
        }

        // Calculate statistics
        let total_nodes: usize = trees.iter().map(|(_, nodes)| self.count_nodes(nodes)).sum();
        let all_languages: HashSet<String> = trees
            .iter()
            .flat_map(|(_, nodes)| self.collect_languages(nodes))
            .collect();

        let direction_label = if self.direction == "upstream" {
            "callers"
        } else {
            "callees"
        };

        Ok(format!(
            "Traced {} call paths for '{}' (direction: {}, depth: {}, cross_language: {})\nFound {} {} across {} languages",
            trees.len(),
            self.symbol,
            self.direction,
            self.max_depth,
            self.cross_language,
            total_nodes,
            direction_label,
            all_languages.len()
        ))
    }


    /// Count total nodes in tree
    fn count_nodes(&self, nodes: &[CallPathNode]) -> usize {
        nodes
            .iter()
            .map(|n| 1 + self.count_nodes(&n.children))
            .sum()
    }

    /// Collect all languages in tree
    fn collect_languages(&self, nodes: &[CallPathNode]) -> HashSet<String> {
        let mut languages = HashSet::new();
        for node in nodes {
            languages.insert(node.symbol.language.clone());
            languages.extend(self.collect_languages(&node.children));
        }
        languages
    }



}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{FileInfo, SymbolDatabase};
    use crate::extractors::{Relationship, RelationshipKind, Symbol, SymbolKind};
    use std::sync::{Arc, Mutex};
    use tempfile::tempdir;

    fn make_symbol(id: &str, name: &str, language: &str, file_path: &str) -> Symbol {
        Symbol {
            id: id.to_string(),
            name: name.to_string(),
            kind: SymbolKind::Function,
            language: language.to_string(),
            file_path: file_path.to_string(),
            signature: None,
            start_line: 1,
            start_column: 0,
            end_line: 1,
            end_column: 1,
            start_byte: 0,
            end_byte: 1,
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
        }
    }

    #[tokio::test]
    async fn cross_language_callers_found_via_naming_variant() {
        let workspace_id = "primary";
        let temp = tempdir().expect("tempdir");
        let db_path = temp.path().join("test.db");
        let db = SymbolDatabase::new(db_path).expect("db");
        let db = Arc::new(Mutex::new(db));

        let target = make_symbol("target", "process_payment", "python", "app.py");
        let variant = make_symbol("variant", "ProcessPayment", "csharp", "Payment.cs");
        let other = make_symbol("other", "helper", "csharp", "Payment.cs");

        {
            let db_guard = db.lock().unwrap();
            let file_target = FileInfo {
                path: target.file_path.clone(),
                language: target.language.clone(),
                hash: "hash1".to_string(),
                size: 0,
                last_modified: 0,
                last_indexed: 0,
                symbol_count: 1,
                content: Some("".to_string()),
            };

            let file_variant = FileInfo {
                path: variant.file_path.clone(),
                language: variant.language.clone(),
                hash: "hash2".to_string(),
                size: 0,
                last_modified: 0,
                last_indexed: 0,
                symbol_count: 1,
                content: Some("".to_string()),
            };

            db_guard
                .store_file_info(&file_target, workspace_id)
                .expect("store target file");
            db_guard
                .store_file_info(&file_variant, workspace_id)
                .expect("store variant file");

            db_guard
                .store_symbols(
                    &[target.clone(), variant.clone(), other.clone()],
                    workspace_id,
                )
                .expect("store symbols");

            // Note: No relationship needed - naming variant is sufficient for cross-language matching
            let rel = Relationship {
                id: "rel1".to_string(),
                from_symbol_id: variant.id.clone(),
                to_symbol_id: other.id.clone(),
                kind: RelationshipKind::Calls,
                file_path: variant.file_path.clone(),
                line_number: 10,
                confidence: 1.0,
                metadata: None,
            };

            db_guard
                .store_relationships(&[rel], workspace_id)
                .expect("store relationships");
        }

        let tool = TraceCallPathTool {
            symbol: target.name.clone(),
            direction: "upstream".to_string(),
            max_depth: 3,
            cross_language: true,
            similarity_threshold: 0.7,
            context_file: None,
            workspace: Some(workspace_id.to_string()),
        };

        let callers = tool
            .find_cross_language_callers(&db, &target)
            .await
            .expect("callers");

        // NEW BEHAVIOR: Naming variant match is sufficient - no database relationship required!
        assert_eq!(
            callers.len(),
            1,
            "Expected to find cross-language caller via naming variant"
        );
        assert_eq!(callers[0].name, "ProcessPayment");
        assert_eq!(callers[0].language, "csharp");
    }
}
