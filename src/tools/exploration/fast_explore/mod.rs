//! FastExploreTool - Multi-mode code exploration
//!
//! Unified exploration tool with multiple strategies:
//! - logic: Find business logic by domain keywords (from find_logic)
//! - similar: Find semantically similar code (IMPLEMENTED)
//! - tests: Discover tests for symbols (CANCELLED - use fast_refs + fast_search instead)
//! - dependencies: Analyze transitive dependencies (IMPLEMENTED)

use anyhow::Result;
use rust_mcp_sdk::macros::{JsonSchema, mcp_tool};
use rust_mcp_sdk::schema::CallToolResult;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::debug;

use crate::database::SymbolDatabase;
use crate::extractors::base::{Relationship, RelationshipKind};
use crate::handler::JulieServerHandler;
use crate::tools::exploration::find_logic::FindLogicTool;
use crate::workspace::registry::generate_workspace_id;

/// Exploration mode selector
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExploreMode {
    /// Find business logic by domain keywords (5-tier CASCADE: FTS5 → AST → Path → HNSW → Graph)
    Logic,

    /// Find semantically similar code using HNSW embeddings
    Similar,

    /// Discover tests for symbols (CANCELLED - use fast_refs + fast_search composition)
    #[allow(dead_code)]
    Tests,

    /// Analyze transitive dependencies via graph traversal
    Dependencies,

    /// Explore type intelligence: implementations, hierarchies, return types, parameter types
    Types,
}

fn default_mode() -> ExploreMode {
    ExploreMode::Logic
}

#[mcp_tool(
    name = "fast_explore",
    description = "Explore codebases with modes: logic (business logic), similar (duplicates), dependencies (graph), types (type analysis). Julie 2.0: Default limit 10 per mode (optimized for token efficiency with focused results).",
    title = "Multi-Mode Code Exploration"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct FastExploreTool {
    /// Exploration mode (default: "logic")
    #[serde(default = "default_mode")]
    pub mode: ExploreMode,

    // ═══════════════════════════════════════════════════════════════════
    // Logic Mode Parameters
    // ═══════════════════════════════════════════════════════════════════
    /// Business domain keywords to search for (logic mode)
    /// Examples: "payment", "auth", "user", "order"
    /// Can use multiple keywords: "payment checkout billing"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,

    /// Maximum results to return (default: 10, logic mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_results: Option<i32>,

    /// Group by architectural layer (default: true, logic mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_by_layer: Option<bool>,

    /// Minimum business relevance score (default: 0.3, range: 0.0-1.0, logic mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_business_score: Option<f32>,

    // ═══════════════════════════════════════════════════════════════════
    // Similar Mode Parameters (Phase 3)
    // ═══════════════════════════════════════════════════════════════════
    /// Symbol to find duplicates of (similar mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,

    /// Similarity threshold 0.0-1.0 (default: 0.8, similar mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f32>,

    // ═══════════════════════════════════════════════════════════════════
    // Tests Mode Parameters (Phase 4)
    // ═══════════════════════════════════════════════════════════════════
    /// Include integration tests (default: true, tests mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_integration: Option<bool>,

    // ═══════════════════════════════════════════════════════════════════
    // Dependencies Mode Parameters (Phase 5)
    // ═══════════════════════════════════════════════════════════════════
    /// Dependency analysis depth (default: 3, deps mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depth: Option<i32>,

    // ═══════════════════════════════════════════════════════════════════
    // Types Mode Parameters (Phase 6)
    // ═══════════════════════════════════════════════════════════════════
    /// Type name to explore (required for types mode)
    /// Examples: "PaymentProcessor", "CheckoutStep", "UserProfile"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_name: Option<String>,

    /// Type of exploration (default: "all", types mode)
    /// Options: "implementations", "hierarchy", "returns", "parameters", "all"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exploration_type: Option<String>,

    /// Maximum results per category (default: 10, types mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,

    // ═══════════════════════════════════════════════════════════════════
    // Common Parameters (All Modes)
    // ═══════════════════════════════════════════════════════════════════
    /// Optional file pattern filter (all modes)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_pattern: Option<String>,

    /// Workspace filter (default: "primary", all modes)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
}

impl FastExploreTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        match self.mode {
            ExploreMode::Logic => self.explore_logic(handler).await,
            ExploreMode::Similar => self.explore_similar(handler).await,
            ExploreMode::Tests => {
                anyhow::bail!("tests mode not yet implemented (Phase 4)")
            }
            ExploreMode::Dependencies => self.explore_dependencies(handler).await,
            ExploreMode::Types => self.explore_types(handler).await,
        }
    }

    /// Logic mode: Delegate to existing FindLogicTool implementation
    async fn explore_logic(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        // Validate required parameters for logic mode
        let domain = self
            .domain
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("domain parameter required for logic mode"))?
            .clone();

        // Create FindLogicTool with parameters
        let find_logic_tool = FindLogicTool {
            domain,
            max_results: self.max_results.unwrap_or(10), // Julie 2.0: Reduced from 50 for token efficiency
            group_by_layer: self.group_by_layer.unwrap_or(true),
            min_business_score: self.min_business_score.unwrap_or(0.3),
        };

        // Delegate to existing implementation
        find_logic_tool.call_tool(handler).await
    }

    /// Similar mode: Find semantically duplicate code using HNSW embeddings
    async fn explore_similar(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        // Validate required parameters for similar mode
        let symbol_name = self
            .symbol
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("symbol parameter required for similar mode"))?;

        let threshold = self.threshold.unwrap_or(0.8);
        let limit = self.max_results.unwrap_or(10) as usize; // Julie 2.0: Reduced from 50 for token efficiency

        // Validate threshold range
        if !(0.0..=1.0).contains(&threshold) {
            anyhow::bail!("threshold must be between 0.0 and 1.0, got {}", threshold);
        }

        // Get workspace
        let workspace = handler.get_workspace().await?.ok_or_else(|| {
            anyhow::anyhow!("No workspace available. Please index workspace first.")
        })?;
        let db_path = workspace.db_path();

        // Ensure vector store is initialized
        handler.ensure_vector_store().await?;

        // Get vector store from workspace
        let vector_store = workspace
            .vector_store
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Vector store not initialized for workspace"))?;

        let has_hnsw = {
            let store_guard = vector_store.read().await;
            store_guard.has_hnsw_index()
        };

        if !has_hnsw {
            use rust_mcp_sdk::schema::TextContent;
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                serde_json::to_string(&json!({
                    "error": "Embeddings not yet ready",
                    "message": "HNSW index is still building in the background. Please try again in a few moments.",
                    "symbol": symbol_name,
                    "total_found": 0,
                    "results": []
                }))?,
            )]));
        }

        // Step 1: Try to get stored embedding for the symbol (REUSE existing embedding)
        // This is faster and more accurate than re-embedding the raw name
        let query_embedding = {
            // Try to find the symbol by name first
            let db_path_for_lookup = db_path.clone();
            let symbol_name_clone = symbol_name.clone();

            let found_symbols = tokio::task::spawn_blocking(move || {
                if let Ok(database) = SymbolDatabase::new(&db_path_for_lookup) {
                    database.get_symbols_by_name(&symbol_name_clone)
                } else {
                    Err(anyhow::anyhow!("Failed to open database"))
                }
            })
            .await
            .map_err(|e| anyhow::anyhow!("spawn_blocking join error: {}", e))??;

            // If symbol found, try to reuse its stored embedding
            if let Some(symbol) = found_symbols.first() {
                let db_path_for_emb = db_path.clone();
                let symbol_id = symbol.id.clone();

                let stored_embedding = tokio::task::spawn_blocking(move || {
                    if let Ok(database) = SymbolDatabase::new(&db_path_for_emb) {
                        database.get_embedding_for_symbol(&symbol_id, "bge-small")
                    } else {
                        Err(anyhow::anyhow!("Failed to open database"))
                    }
                })
                .await
                .map_err(|e| anyhow::anyhow!("spawn_blocking join error: {}", e))??;

                if let Some(embedding) = stored_embedding {
                    debug!(
                        "✨ Reusing stored embedding for '{}' (includes signature, docs, context)",
                        symbol_name
                    );
                    embedding
                } else {
                    // Fallback: generate embedding using full symbol context (not just bare name)
                    // This ensures consistent quality with indexed embeddings
                    debug!(
                        "⚠️  No stored embedding for '{}', building from full symbol context",
                        symbol_name
                    );
                    handler.ensure_embedding_engine().await?;
                    let mut engine = handler.embedding_engine.write().await;
                    if let Some(ref mut engine) = *engine {
                        let embedding_text = engine.build_embedding_text(symbol);
                        engine.embed_text(&embedding_text)?
                    } else {
                        anyhow::bail!("Embedding engine not available")
                    }
                }
            } else {
                // Symbol not found, generate embedding from raw name (user might be exploring)
                debug!(
                    "🔎 Symbol '{}' not found in database, embedding raw name",
                    symbol_name
                );
                handler.ensure_embedding_engine().await?;
                let mut engine = handler.embedding_engine.write().await;
                if let Some(ref mut engine) = *engine {
                    engine.embed_text(symbol_name)?
                } else {
                    anyhow::bail!("Embedding engine not available")
                }
            }
        };

        debug!(
            "🔍 Searching for symbols similar to '{}' (threshold: {})",
            symbol_name, threshold
        );

        // Step 2: Search using HNSW for similar symbols
        let vector_store_clone = vector_store.clone();
        let db_path_clone = db_path.clone();
        let query_embedding_clone = query_embedding.clone();
        let model_name = "bge-small".to_string();

        let similar_results = tokio::task::spawn_blocking(move || {
            if let Ok(database) = SymbolDatabase::new(&db_path_clone) {
                let store_guard = vector_store_clone.blocking_read();
                store_guard.search_similar_hnsw(
                    &database,
                    &query_embedding_clone,
                    limit,
                    threshold,
                    &model_name,
                )
            } else {
                Err(anyhow::anyhow!(
                    "Failed to open database at {:?}",
                    db_path_clone
                ))
            }
        })
        .await
        .map_err(|e| anyhow::anyhow!("spawn_blocking join error: {}", e))??;

        debug!(
            "🚀 HNSW search found {} similar symbols",
            similar_results.len()
        );

        // Step 3: Fetch actual symbol data
        let symbols = if !similar_results.is_empty() {
            let symbol_ids: Vec<String> = similar_results
                .iter()
                .map(|r| r.symbol_id.clone())
                .collect();

            let db_path_for_fetch = db_path.clone();
            tokio::task::spawn_blocking(move || {
                if let Ok(database) = SymbolDatabase::new(&db_path_for_fetch) {
                    database.get_symbols_by_ids(&symbol_ids)
                } else {
                    Err(anyhow::anyhow!(
                        "Failed to open database at {:?}",
                        db_path_for_fetch
                    ))
                }
            })
            .await
            .map_err(|e| anyhow::anyhow!("spawn_blocking join error: {}", e))??
        } else {
            Vec::new()
        };

        // Step 4: Combine symbols with their similarity scores
        let results: Vec<serde_json::Value> = symbols
            .iter()
            .enumerate()
            .filter_map(|(idx, symbol)| {
                similar_results.get(idx).map(|sim_result| {
                    json!({
                        "symbol_name": symbol.name,
                        "file_path": symbol.file_path,
                        "kind": symbol.kind,
                        "language": symbol.language,
                        "signature": symbol.signature,
                        "similarity_score": sim_result.similarity_score,
                        "line": symbol.start_line,
                        "doc_comment": symbol.doc_comment,
                    })
                })
            })
            .collect();

        // Step 5: Format response
        let response = json!({
            "query_symbol": symbol_name,
            "threshold": threshold,
            "total_found": results.len(),
            "results": results,
            "tip": "Similarity scores range from 0.0 (unrelated) to 1.0 (identical). High scores (>0.8) indicate likely code duplicates.",
        });

        use rust_mcp_sdk::schema::TextContent;
        Ok(CallToolResult::text_content(vec![TextContent::from(
            serde_json::to_string(&response)?,
        )]))
    }

    /// Dependencies mode: Analyze transitive dependencies via graph traversal
    async fn explore_dependencies(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        use std::collections::{HashSet, VecDeque};

        // Validate required parameters
        let symbol_name = self
            .symbol
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("symbol parameter required for dependencies mode"))?;

        let max_depth = self.depth.unwrap_or(3).min(10); // Cap at 10 to prevent infinite recursion

        // Get workspace
        let workspace = handler.get_workspace().await?.ok_or_else(|| {
            anyhow::anyhow!("No workspace available. Please index workspace first.")
        })?;

        // Generate workspace ID from root path
        let workspace_id = generate_workspace_id(&workspace.root.to_string_lossy())?;
        let db_path = workspace.workspace_db_path(&workspace_id);

        debug!(
            "🔍 Analyzing dependencies for '{}' (max depth: {})",
            symbol_name, max_depth
        );

        // Find the symbol by name
        let db = tokio::task::spawn_blocking({
            let db_path = db_path.clone();
            move || SymbolDatabase::new(&db_path)
        })
        .await
        .map_err(|e| anyhow::anyhow!("spawn_blocking join error: {}", e))??;

        let symbols = db.get_symbols_by_name(symbol_name)?;

        if symbols.is_empty() {
            // Symbol not found - return empty result
            use rust_mcp_sdk::schema::TextContent;
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                serde_json::to_string(&json!({
                    "symbol": symbol_name,
                    "found": false,
                    "message": format!("Symbol '{}' not found in workspace", symbol_name),
                    "depth": max_depth,
                    "total_dependencies": 0,
                    "dependencies": []
                }))?,
            )]));
        }

        // Use the first matching symbol (most relevant)
        let root_symbol = &symbols[0];

        // Build dependency tree using BFS for level-by-level traversal
        let mut visited = HashSet::new();
        let mut queue: VecDeque<(String, i32)> = VecDeque::new(); // (symbol_id, current_depth)
        let mut dependency_map: std::collections::HashMap<String, Vec<(Relationship, i32)>> =
            std::collections::HashMap::new();

        // Start with root symbol
        queue.push_back((root_symbol.id.clone(), 0));
        visited.insert(root_symbol.id.clone());

        while let Some((current_symbol_id, current_depth)) = queue.pop_front() {
            if current_depth >= max_depth {
                continue; // Reached max depth
            }

            // Get relationships for this symbol (what it depends on)
            let relationships = db.get_relationships_for_symbol(&current_symbol_id)?;

            // Filter to dependency-relevant relationship kinds
            let dep_relationships: Vec<_> = relationships
                .into_iter()
                .filter(|r| {
                    matches!(
                        r.kind,
                        RelationshipKind::Imports
                            | RelationshipKind::Uses
                            | RelationshipKind::Calls
                            | RelationshipKind::References
                            | RelationshipKind::Extends
                            | RelationshipKind::Implements
                    )
                })
                .collect();

            for rel in dep_relationships {
                let target_symbol_id = rel.to_symbol_id.clone();

                // Store relationship with depth
                dependency_map
                    .entry(current_symbol_id.clone())
                    .or_insert_with(Vec::new)
                    .push((rel.clone(), current_depth + 1));

                // Add to queue if not visited
                if !visited.contains(&target_symbol_id) {
                    visited.insert(target_symbol_id.clone());
                    queue.push_back((target_symbol_id, current_depth + 1));
                }
            }
        }

        // Build result tree
        let dependencies = self.build_dependency_tree_from_map(
            &db,
            &root_symbol.id,
            &dependency_map,
            0,
            max_depth,
        )?;

        let total_dependencies = visited.len() - 1; // Exclude root symbol

        let response = json!({
            "symbol": symbol_name,
            "found": true,
            "depth": max_depth,
            "total_dependencies": total_dependencies,
            "dependencies": dependencies,
            "tip": "Dependencies show what this symbol imports, uses, calls, or references. Use depth parameter to control how deep the analysis goes."
        });

        use rust_mcp_sdk::schema::TextContent;
        Ok(CallToolResult::text_content(vec![TextContent::from(
            serde_json::to_string(&response)?,
        )]))
    }

    /// Helper to build dependency tree from relationship map
    fn build_dependency_tree_from_map(
        &self,
        db: &SymbolDatabase,
        symbol_id: &str,
        dependency_map: &std::collections::HashMap<String, Vec<(Relationship, i32)>>,
        current_depth: i32,
        max_depth: i32,
    ) -> Result<Vec<serde_json::Value>> {
        if current_depth >= max_depth {
            return Ok(vec![]);
        }

        let mut result = Vec::new();

        if let Some(deps) = dependency_map.get(symbol_id) {
            for (rel, depth) in deps {
                if let Ok(Some(target_symbol)) = db.get_symbol_by_id(&rel.to_symbol_id) {
                    let children = self.build_dependency_tree_from_map(
                        db,
                        &rel.to_symbol_id,
                        dependency_map,
                        current_depth + 1,
                        max_depth,
                    )?;

                    result.push(json!({
                        "name": target_symbol.name,
                        "kind": format!("{}", rel.kind),
                        "file_path": target_symbol.file_path,
                        "line": rel.line_number,
                        "depth": depth,
                        "symbol_kind": target_symbol.kind.to_string(),
                        "children": children
                    }));
                }
            }
        }

        Ok(result)
    }

    /// Types mode: Explore type intelligence (implementations, hierarchies, return types, parameters)
    async fn explore_types(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        use rust_mcp_sdk::schema::TextContent;
        use serde_json::json;

        // Validate required parameter
        let type_name = self
            .type_name
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("type_name parameter required for types mode"))?
            .clone();

        let exploration_type = self.exploration_type.as_deref().unwrap_or("all");
        let limit = self.limit.unwrap_or(10) as usize; // Julie 2.0: Reduced from 50 for token efficiency

        // Get workspace and database
        let workspace = handler.get_workspace().await?.ok_or_else(|| {
            anyhow::anyhow!("No workspace available. Please index workspace first.")
        })?;

        let db_arc = workspace
            .db
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Database not initialized for workspace"))?;
        let db = db_arc.lock().expect("Failed to lock database");

        // Query database based on exploration_type
        let mut implementations = Vec::new();
        let mut returns = Vec::new();
        let mut parameters = Vec::new();
        let mut hierarchy = serde_json::Map::new();

        match exploration_type {
            "implementations" => {
                implementations = db
                    .find_type_implementations(&type_name, None)?
                    .into_iter()
                    .take(limit)
                    .collect();
            }
            "returns" => {
                returns = db
                    .find_functions_returning_type(&type_name, None)?
                    .into_iter()
                    .take(limit)
                    .collect();
            }
            "parameters" => {
                parameters = db
                    .find_functions_with_parameter_type(&type_name, None)?
                    .into_iter()
                    .take(limit)
                    .collect();
            }
            "hierarchy" => {
                let (parents, children) = db.find_type_hierarchy(&type_name, None)?;
                hierarchy.insert("parents".to_string(), json!(parents));
                hierarchy.insert("children".to_string(), json!(children));
            }
            "all" | _ => {
                // Query all categories
                implementations = db
                    .find_type_implementations(&type_name, None)?
                    .into_iter()
                    .take(limit)
                    .collect();
                returns = db
                    .find_functions_returning_type(&type_name, None)?
                    .into_iter()
                    .take(limit)
                    .collect();
                parameters = db
                    .find_functions_with_parameter_type(&type_name, None)?
                    .into_iter()
                    .take(limit)
                    .collect();

                let (parents, children) = db.find_type_hierarchy(&type_name, None)?;
                hierarchy.insert("parents".to_string(), json!(parents));
                hierarchy.insert("children".to_string(), json!(children));
            }
        }

        // Count actual hierarchy entries (parents + children arrays), not HashMap size
        let hierarchy_count = hierarchy
            .get("parents")
            .and_then(|v| v.as_array().map(|arr| arr.len()))
            .unwrap_or(0)
            + hierarchy
                .get("children")
                .and_then(|v| v.as_array().map(|arr| arr.len()))
                .unwrap_or(0);

        let total_found =
            implementations.len() + returns.len() + parameters.len() + hierarchy_count;

        let result = json!({
            "type_name": type_name,
            "exploration_type": exploration_type,
            "limit": limit,
            "results": {
                "implementations": implementations,
                "hierarchy": hierarchy,
                "returns": returns,
                "parameters": parameters
            },
            "total_found": total_found,
        });

        Ok(CallToolResult::text_content(vec![TextContent::from(
            serde_json::to_string_pretty(&result)?,
        )]))
    }
}
