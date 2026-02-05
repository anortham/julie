//! FastExploreTool - Multi-mode code exploration
//!
//! Unified exploration tool with multiple strategies:
//! - logic: Find business logic by domain keywords (from find_logic)
//! - similar: Find semantically similar code (IMPLEMENTED)
//! - tests: Discover tests for symbols (CANCELLED - use fast_refs + fast_search instead)
//! - dependencies: Analyze transitive dependencies (IMPLEMENTED)

use anyhow::Result;
use schemars::JsonSchema;
use crate::mcp_compat::{CallToolResult, Content, CallToolResultExt};
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
    /// Find business logic by domain keywords (4-tier CASCADE: Tantivy → AST → Path → Graph)
    Logic,

    /// Find semantically similar code (deprecated - embeddings removed)
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
            output_format: Some("auto".to_string()), // Auto mode: TOON for 5+ results, JSON for <5
        };

        // Delegate to existing implementation
        find_logic_tool.call_tool(handler).await
    }

    /// Similar mode: Find semantically similar code
    /// NOTE: Embedding-based similarity search has been removed. Use fast_search with
    /// search_method="semantic" for text-based similarity, or use fast_refs to find
    /// related symbols through the relationship graph.
    async fn explore_similar(&self, _handler: &JulieServerHandler) -> Result<CallToolResult> {
        let symbol_name = self
            .symbol
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("symbol parameter required for similar mode"))?;

        let threshold = self.threshold.unwrap_or(0.8);
        if !(0.0..=1.0).contains(&threshold) {
            anyhow::bail!("threshold must be between 0.0 and 1.0, got {}", threshold);
        }

        let response = json!({
            "query_symbol": symbol_name,
            "total_found": 0,
            "results": [],
            "message": "Semantic similarity search (embeddings/HNSW) has been removed. Use fast_search with search_method='semantic' for text-based similarity, or use fast_refs to find related symbols through the relationship graph.",
        });

        Ok(CallToolResult::text_content(vec![Content::text(
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
            return Ok(CallToolResult::text_content(vec![Content::text(
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

        Ok(CallToolResult::text_content(vec![Content::text(
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

        Ok(CallToolResult::text_content(vec![Content::text(
            serde_json::to_string_pretty(&result)?,
        )]))
    }
}
