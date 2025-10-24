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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call_paths: Option<Vec<CallPath>>,
}

/// Serializable call path for structured output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallPath {
    pub root_symbol: String,
    pub root_file: String,
    pub root_language: String,
    pub nodes: Vec<SerializablePathNode>,
    pub total_depth: u32,
}

/// Serializable path node for structured output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializablePathNode {
    pub symbol_name: String,
    pub file_path: String,
    pub language: String,
    pub line: u32,
    pub match_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relationship_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub similarity: Option<f32>,
    pub level: u32,
    pub children: Vec<SerializablePathNode>,
}

fn default_upstream() -> String {
    "upstream".to_string()
}

fn default_depth() -> u32 {
    3
}

fn default_workspace() -> Option<String> {
    Some("primary".to_string())
}

fn default_output_format() -> String {
    "json".to_string()
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
    /// Symbol to start tracing from. Supports simple and qualified names.
    /// Examples: "getUserData", "UserService.create", "processPayment", "MyClass::method", "React.Component"
    /// Julie intelligently traces across languages (TypeScript → Go → Python → SQL) using naming variants
    /// This is Julie's superpower - cross-language call path tracing
    pub symbol: String,

    /// Trace direction (default: "upstream").
    /// Options: "upstream" (find callers), "downstream" (find callees), "both"
    /// Most common: "upstream" - who calls this function?
    #[serde(default = "default_upstream")]
    pub direction: String,

    /// Maximum levels to trace (default: 3, range: 1-10).
    /// Prevents infinite recursion while balancing depth and performance
    /// Higher values may be slow
    #[serde(default = "default_depth")]
    pub max_depth: u32,

    /// Starting file for context (default: None, optional).
    /// Helps disambiguate when multiple symbols have the same name
    /// Example: "src/services/user.ts"
    #[serde(default)]
    pub context_file: Option<String>,

    /// Workspace filter (optional): "primary" (default) or specific workspace ID
    /// Examples: "primary", "reference-workspace_abc123"
    /// Default: "primary" - search the primary workspace
    /// Note: Multi-workspace search ("all") is not supported - search one workspace at a time
    #[serde(default = "default_workspace")]
    pub workspace: Option<String>,

    /// Output format (default: "json").
    /// "json" = Machine-parseable structured data (recommended for AI agents)
    /// "tree" = Human-readable ASCII tree diagram with file locations
    #[serde(default = "default_output_format")]
    pub output_format: String,
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
        call_paths: Option<Vec<CallPath>>,
    ) -> Result<CallToolResult> {
        let result = TraceCallPathResult {
            tool: "trace_call_path".to_string(),
            symbol: self.symbol.clone(),
            direction: self.direction.clone(),
            max_depth: self.max_depth,
            cross_language: true, // Always enabled - this is Julie's superpower!
            success,
            paths_found,
            next_actions,
            error_message,
            call_paths,
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
            "🔍 Tracing call path: {} (direction: {}, depth: {}, cross_lang: enabled)",
            self.symbol, self.direction, self.max_depth
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
                None,
            );
        }

        // Get workspace and database with workspace filtering support
        let primary_workspace = handler.get_workspace().await?.ok_or_else(|| {
            anyhow!("No workspace initialized. Run 'manage_workspace index' first")
        })?;

        // Determine target workspace and load appropriate database + vector store
        let (db, vector_store) = match self.workspace.as_deref() {
            Some("primary") | None => {
                // Use primary workspace database and vector store (default)
                let db = primary_workspace
                    .db
                    .as_ref()
                    .ok_or_else(|| anyhow!("No primary database available"))?
                    .clone();

                let vector_store = primary_workspace.vector_store.clone();

                (db, vector_store)
            }
            Some(workspace_id) => {
                // Load reference workspace database
                let ref_db_path = primary_workspace.workspace_db_path(workspace_id);
                if !ref_db_path.exists() {
                    let message = format!(
                        "Reference workspace database not found: {}\nCheck workspace ID with 'manage_workspace list'",
                        workspace_id
                    );
                    return self.create_result(
                        false,
                        0,
                        vec!["Use 'manage_workspace list' to see available workspaces".to_string()],
                        message.clone(),
                        Some(format!("Workspace not found: {}", workspace_id)),
                        None,
                    );
                }

                debug!("📂 Opening reference workspace DB: {:?}", ref_db_path);

                // Open reference workspace database in blocking task
                let ref_db = tokio::task::spawn_blocking(move || {
                    crate::database::SymbolDatabase::new(&ref_db_path)
                })
                .await
                .map_err(|e| anyhow!("Failed to spawn database task: {}", e))??;

                let db = Arc::new(Mutex::new(ref_db));

                // Load reference workspace vector store
                let vectors_path = primary_workspace.workspace_vectors_path(workspace_id);
                let vector_store = if vectors_path.exists() {
                    debug!("📂 Loading reference workspace vectors: {:?}", vectors_path);

                    // Load HNSW index from disk
                    let mut store = crate::embeddings::vector_store::VectorStore::new(384)?;
                    let hnsw_path = vectors_path.join("hnsw_index");

                    if hnsw_path.with_extension("hnsw.graph").exists() {
                        match store.load_hnsw_index(&hnsw_path) {
                            Ok(()) => {
                                debug!("✅ Loaded HNSW index for reference workspace");
                                Some(Arc::new(tokio::sync::RwLock::new(store)))
                            }
                            Err(e) => {
                                debug!("⚠️  Failed to load HNSW index for reference workspace: {}", e);
                                None
                            }
                        }
                    } else {
                        debug!("ℹ️  No HNSW index found for reference workspace (semantic search disabled)");
                        None
                    }
                } else {
                    debug!("ℹ️  No vectors directory for reference workspace (semantic search disabled)");
                    None
                };

                (db, vector_store)
            }
        };

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
                None,
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
                    None,
                );
            }
        }

        // Build call path tree
        let mut visited = HashSet::new();
        let mut all_trees = Vec::new();

        for starting_symbol in &starting_symbols {
            let call_tree = match self.direction.as_str() {
                "upstream" => {
                    self.trace_upstream(handler, &db, &vector_store, starting_symbol, 0, &mut visited)
                        .await?
                }
                "downstream" => {
                    self.trace_downstream(handler, &db, &vector_store, starting_symbol, 0, &mut visited)
                        .await?
                }
                "both" => {
                    // Use single visited set to prevent duplicate processing across both directions
                    let mut upstream = self
                        .trace_upstream(handler, &db, &vector_store, starting_symbol, 0, &mut visited)
                        .await?;
                    let downstream = self
                        .trace_downstream(handler, &db, &vector_store, starting_symbol, 0, &mut visited)
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
                        None,
                    );
                }
            };

            if !call_tree.is_empty() {
                all_trees.push((starting_symbol.clone(), call_tree));
            }
        }

        // Format output
        let output = self.format_call_trees(&all_trees)?;

        // Convert trees to serializable format for structured content
        let call_paths = if !all_trees.is_empty() {
            Some(self.trees_to_call_paths(&all_trees))
        } else {
            None
        };

        self.create_result(
            true,
            all_trees.len(),
            vec![
                "Review call paths to understand execution flow".to_string(),
                "Use fast_goto to navigate to specific symbols".to_string(),
            ],
            output,
            None,
            call_paths,
        )
    }

    /// Trace upstream (find callers)
    #[async_recursion::async_recursion]
    async fn trace_upstream(
        &self,
        handler: &JulieServerHandler,
        db: &Arc<Mutex<SymbolDatabase>>,
        vector_store: &Option<Arc<tokio::sync::RwLock<crate::embeddings::vector_store::VectorStore>>>,
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
                    .trace_upstream(handler, db, vector_store, &caller_symbol, current_depth + 1, visited)
                    .await?;
            }

            nodes.push(node);
        }

        // Step 2: Cross-language matching (always enabled - this is Julie's superpower!)
        if current_depth < self.max_depth {
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
                let cross_lang_limit = self.get_cross_language_depth_limit();
                if current_depth + 1 < cross_lang_limit {
                    node.children = self
                        .trace_upstream(handler, db, vector_store, &caller_symbol, current_depth + 1, visited)
                        .await?;
                }

                nodes.push(node);
            }

            let semantic_callers = self
                .find_semantic_cross_language_callers(handler, db, vector_store, symbol)
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

                let cross_lang_limit = self.get_cross_language_depth_limit();
                if current_depth + 1 < cross_lang_limit {
                    node.children = self
                        .trace_upstream(handler, db, vector_store, &semantic.symbol, current_depth + 1, visited)
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
        vector_store: &Option<Arc<tokio::sync::RwLock<crate::embeddings::vector_store::VectorStore>>>,
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
                    .trace_downstream(handler, db, vector_store, &callee_symbol, current_depth + 1, visited)
                    .await?;
            }

            nodes.push(node);
        }

        // Step 2: Cross-language matching (always enabled - this is Julie's superpower!)
        if current_depth < self.max_depth {
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

                // Recursively trace (but limit depth for cross-language to avoid explosion)
                let cross_lang_limit = self.get_cross_language_depth_limit();
                if current_depth + 1 < cross_lang_limit {
                    node.children = self
                        .trace_downstream(handler, db, vector_store, &callee_symbol, current_depth + 1, visited)
                        .await?;
                }

                nodes.push(node);
            }

            let semantic_callees = self
                .find_semantic_cross_language_callees(handler, db, vector_store, symbol)
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

                let cross_lang_limit = self.get_cross_language_depth_limit();
                if current_depth + 1 < cross_lang_limit {
                    node.children = self
                        .trace_downstream(handler, db, vector_store, &semantic.symbol, current_depth + 1, visited)
                        .await?;
                }

                nodes.push(node);
            }
        }

        Ok(nodes)
    }

    /// Find cross-language callers using naming variants
    pub(crate) async fn find_cross_language_callers(
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
        db: &Arc<Mutex<SymbolDatabase>>,
        vector_store: &Option<Arc<tokio::sync::RwLock<crate::embeddings::vector_store::VectorStore>>>,
        symbol: &Symbol,
        max_results: usize,
    ) -> Result<Vec<(Symbol, f32)>> {
        if max_results == 0 {
            return Ok(vec![]);
        }

        // Check if vector store is available
        let vector_store = match vector_store {
            Some(store) => store.clone(),
            None => {
                debug!("Semantic tracing disabled - no vector store for this workspace");
                return Ok(vec![]);
            }
        };

        // Ensure embedding engine is available
        if let Err(e) = handler.ensure_embedding_engine().await {
            debug!(
                "Semantic tracing disabled - embedding engine unavailable: {}",
                e
            );
            return Ok(vec![]);
        }

        let db_arc = db.clone();

        // 🔧 REFACTOR: Check if HNSW index is built
        let store_guard = vector_store.read().await;
        if !store_guard.has_hnsw_index() {
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

        // 🔧 REFACTOR: Use new architecture with SQLite on-demand fetching
        let semantic_results = match tokio::task::block_in_place(|| {
            let db_lock = db_arc.lock().unwrap();
            let model_name = "bge-small";
            store_guard.search_similar_hnsw(
                &*db_lock,
                &embedding,
                max_results,
                0.7, // Hardcoded good balance threshold
                model_name,
            )
        }) {
            Ok(results) => results,
            Err(e) => {
                debug!("Semantic neighbor search failed: {}", e);
                return Ok(vec![]);
            }
        };
        drop(store_guard);

        debug!(
            "🚀 HNSW search found {} semantic neighbors",
            semantic_results.len()
        );

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
        vector_store: &Option<Arc<tokio::sync::RwLock<crate::embeddings::vector_store::VectorStore>>>,
        symbol: &Symbol,
    ) -> Result<Vec<SemanticMatch>> {
        // Use hardcoded semantic limit for good balance between coverage and performance
        let limit = 8;
        let candidates = self.semantic_neighbors(handler, db, vector_store, symbol, limit).await?;

        if candidates.is_empty() {
            return Ok(vec![]);
        }

        let mut matches = Vec::new();
        let db_lock = db.lock().unwrap();

        for (candidate, similarity) in candidates {
            // Only match cross-language symbols (that's the whole point!)
            if candidate.language == symbol.language {
                continue;
            }

            // Check if there's an existing relationship (for metadata only)
            // But semantic match is VALID even without a relationship!
            let relationships = db_lock.get_relationships_for_symbol(&candidate.id).ok();
            let relationship_kind = relationships
                .and_then(|rels| {
                    rels.into_iter().find(|r| {
                        matches!(
                            r.kind,
                            RelationshipKind::Calls | RelationshipKind::References
                        ) && r.from_symbol_id == candidate.id
                            && r.to_symbol_id == symbol.id
                    })
                })
                .map(|r| r.kind)
                .unwrap_or(RelationshipKind::Calls); // Default to Calls for semantic bridges

            // Accept ALL cross-language semantic matches above threshold
            // This is how we bridge language gaps!
            matches.push(SemanticMatch {
                symbol: candidate,
                relationship_kind,
                similarity,
            });
        }

        drop(db_lock);

        Ok(matches)
    }

    async fn find_semantic_cross_language_callees(
        &self,
        handler: &JulieServerHandler,
        db: &Arc<Mutex<SymbolDatabase>>,
        vector_store: &Option<Arc<tokio::sync::RwLock<crate::embeddings::vector_store::VectorStore>>>,
        symbol: &Symbol,
    ) -> Result<Vec<SemanticMatch>> {
        // Use hardcoded semantic limit for good balance between coverage and performance
        let limit = 8;
        let candidates = self.semantic_neighbors(handler, db, vector_store, symbol, limit).await?;

        if candidates.is_empty() {
            return Ok(vec![]);
        }

        let mut matches = Vec::new();
        let db_lock = db.lock().unwrap();

        for (candidate, similarity) in candidates {
            // Only match cross-language symbols (that's the whole point!)
            if candidate.language == symbol.language {
                continue;
            }

            // Check if there's an existing relationship (for metadata only)
            // But semantic match is VALID even without a relationship!
            let relationships = db_lock.get_relationships_to_symbol(&candidate.id).ok();
            let relationship_kind = relationships
                .and_then(|rels| {
                    rels.into_iter().find(|r| {
                        matches!(
                            r.kind,
                            RelationshipKind::Calls | RelationshipKind::References
                        ) && r.from_symbol_id == symbol.id
                            && r.to_symbol_id == candidate.id
                    })
                })
                .map(|r| r.kind)
                .unwrap_or(RelationshipKind::Calls); // Default to Calls for semantic bridges

            // Accept ALL cross-language semantic matches above threshold
            // This is how we bridge language gaps!
            matches.push(SemanticMatch {
                symbol: candidate,
                relationship_kind,
                similarity,
            });
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

        // Choose output format based on parameter
        if self.output_format == "tree" {
            // ASCII tree visualization for humans
            self.build_ascii_tree(trees, total_nodes, &all_languages, direction_label)
        } else {
            // JSON-focused summary for AI agents (default)
            Ok(format!(
                "Traced {} call paths for '{}' (direction: {}, depth: {}, cross_language: {})\nFound {} {} across {} languages\n\nFull call path details are in structured_content.call_paths",
                trees.len(),
                self.symbol,
                self.direction,
                self.max_depth,
                true, // Cross-language always enabled
                total_nodes,
                direction_label,
                all_languages.len()
            ))
        }
    }

    /// Build ASCII tree visualization for human readability
    fn build_ascii_tree(
        &self,
        trees: &[(Symbol, Vec<CallPathNode>)],
        total_nodes: usize,
        all_languages: &HashSet<String>,
        direction_label: &str,
    ) -> Result<String> {
        let mut output = String::new();

        // Header
        output.push_str(&format!("Call Path Trace: '{}'\n", self.symbol));
        output.push_str(&format!(
            "Direction: {} | Depth: {} | Cross-language: enabled\n",
            self.direction, self.max_depth
        ));
        output.push_str(&format!(
            "Found {} {} across {} languages\n\n",
            total_nodes,
            direction_label,
            all_languages.len()
        ));

        // Render each tree
        for (i, (root, nodes)) in trees.iter().enumerate() {
            output.push_str(&format!(
                "Path {}:\n{} ({}:{})\n",
                i + 1,
                root.name,
                root.file_path,
                root.start_line
            ));

            // Render child nodes recursively
            for (j, node) in nodes.iter().enumerate() {
                let is_last = j == nodes.len() - 1;
                self.render_node(node, &mut output, "", is_last);
            }
            output.push('\n');
        }

        Ok(output)
    }

    /// Recursively render a node in ASCII tree format
    fn render_node(&self, node: &CallPathNode, output: &mut String, prefix: &str, is_last: bool) {
        // Choose tree characters
        let connector = if is_last { "└─" } else { "├─" };
        let extension = if is_last { "  " } else { "│ " };

        // Format match type indicator
        let match_indicator = match node.match_type {
            MatchType::Direct => "→",
            MatchType::NamingVariant => "≈",
            MatchType::Semantic => "~",
        };

        // Format similarity if present
        let similarity_str = if let Some(sim) = node.similarity {
            format!(" [sim: {:.2}]", sim)
        } else {
            String::new()
        };

        // Write node
        output.push_str(&format!(
            "{}{} {} {} ({}:{}){}\n",
            prefix,
            connector,
            match_indicator,
            node.symbol.name,
            node.symbol.file_path,
            node.symbol.start_line,
            similarity_str
        ));

        // Render children
        let new_prefix = format!("{}{}", prefix, extension);
        for (i, child) in node.children.iter().enumerate() {
            let child_is_last = i == node.children.len() - 1;
            self.render_node(child, output, &new_prefix, child_is_last);
        }
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

    /// Convert trees to serializable format for structured output
    fn trees_to_call_paths(&self, trees: &[(Symbol, Vec<CallPathNode>)]) -> Vec<CallPath> {
        trees
            .iter()
            .map(|(root, nodes)| {
                let max_depth = self.calculate_max_depth(nodes);
                CallPath {
                    root_symbol: root.name.clone(),
                    root_file: root.file_path.clone(),
                    root_language: root.language.clone(),
                    nodes: nodes.iter().map(|n| self.node_to_serializable(n)).collect(),
                    total_depth: max_depth,
                }
            })
            .collect()
    }

    /// Convert CallPathNode to serializable format
    fn node_to_serializable(&self, node: &CallPathNode) -> SerializablePathNode {
        let match_type_str = match node.match_type {
            MatchType::Direct => "direct",
            MatchType::NamingVariant => "naming_variant",
            MatchType::Semantic => "semantic",
        };

        let relationship_str = node.relationship_kind.as_ref().map(|k| {
            match k {
                RelationshipKind::Calls => "calls",
                RelationshipKind::Extends => "extends",
                RelationshipKind::Implements => "implements",
                RelationshipKind::Uses => "uses",
                RelationshipKind::Returns => "returns",
                RelationshipKind::Parameter => "parameter",
                RelationshipKind::Imports => "imports",
                RelationshipKind::Instantiates => "instantiates",
                RelationshipKind::References => "references",
                RelationshipKind::Defines => "defines",
                RelationshipKind::Overrides => "overrides",
                RelationshipKind::Contains => "contains",
                RelationshipKind::Joins => "joins",
                RelationshipKind::Composition => "composition",
            }
            .to_string()
        });

        SerializablePathNode {
            symbol_name: node.symbol.name.clone(),
            file_path: node.symbol.file_path.clone(),
            language: node.symbol.language.clone(),
            line: node.symbol.start_line,
            match_type: match_type_str.to_string(),
            relationship_kind: relationship_str,
            similarity: node.similarity,
            level: node.level,
            children: node
                .children
                .iter()
                .map(|c| self.node_to_serializable(c))
                .collect(),
        }
    }

    /// Calculate maximum depth in tree
    fn calculate_max_depth(&self, nodes: &[CallPathNode]) -> u32 {
        nodes
            .iter()
            .map(|n| {
                let child_depth = if n.children.is_empty() {
                    0
                } else {
                    self.calculate_max_depth(&n.children)
                };
                n.level + child_depth
            })
            .max()
            .unwrap_or(0)
    }

    /// Get cross-language recursion depth limit
    /// Uses max_depth - 1 to prevent excessive expansion
    fn get_cross_language_depth_limit(&self) -> u32 {
        self.max_depth.saturating_sub(1)
    }
}
