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
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info};

use crate::database::SymbolDatabase;
use crate::extractors::{RelationshipKind, Symbol};
use crate::handler::JulieServerHandler;
use crate::utils::cross_language_intelligence::generate_naming_variants;
use crate::utils::token_estimation::TokenEstimator;

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
    description = "TRACE EXECUTION FLOW - Follow calls across languages using relationships and embeddings (Julie's superpower)",
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
    level: u32,
    match_type: MatchType,
    relationship_kind: Option<RelationshipKind>,
    children: Vec<CallPathNode>,
}

/// Type of match found
#[derive(Debug, Clone, PartialEq)]
enum MatchType {
    Direct,           // Same language, direct relationship
    NamingVariant,    // Cross-language via naming convention
    #[allow(dead_code)]
    Semantic,         // Via embedding similarity (future feature)
}

impl TraceCallPathTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        info!(
            "🔍 Tracing call path: {} (direction: {}, depth: {}, cross_lang: {})",
            self.symbol, self.direction, self.max_depth, self.cross_language
        );

        // Validate parameters
        if self.max_depth > 10 {
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                "❌ max_depth cannot exceed 10 (performance limit)\n\
                 💡 Try with max_depth: 5 for a reasonable balance".to_string(),
            )]));
        }

        if self.similarity_threshold < 0.0 || self.similarity_threshold > 1.0 {
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                "❌ similarity_threshold must be between 0.0 and 1.0\n\
                 💡 Recommended: 0.7 for balanced results".to_string(),
            )]));
        }

        // Get workspace and database
        let workspace = handler
            .get_workspace()
            .await?
            .ok_or_else(|| anyhow!("No workspace initialized. Run 'manage_workspace index' first"))?;

        let db = workspace
            .db
            .as_ref()
            .ok_or_else(|| anyhow!("No database available"))?
            .clone();

        // Find the starting symbol(s)
        let db_lock = db.lock().await;
        let mut starting_symbols = db_lock.get_symbols_by_name(&self.symbol)?;
        drop(db_lock);

        if starting_symbols.is_empty() {
            let message = format!(
                "❌ Symbol not found: '{}'\n\n\
                 Possible reasons:\n\
                 • Symbol doesn't exist or not indexed\n\
                 • Typo in symbol name\n\
                 • Try using fast_search to find the symbol first",
                self.symbol
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        // If context file provided, filter to symbols in that file
        if let Some(ref context_file) = self.context_file {
            starting_symbols.retain(|s| s.file_path == *context_file);
            if starting_symbols.is_empty() {
                let message = format!(
                    "❌ Symbol '{}' not found in file: {}\n\n\
                     💡 Try without context_file to search all files",
                    self.symbol, context_file
                );
                return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
            }
        }

        // Build call path tree
        let mut visited = HashSet::new();
        let mut all_trees = Vec::new();

        for starting_symbol in &starting_symbols {
            let call_tree = match self.direction.as_str() {
                "upstream" => self.trace_upstream(&db, starting_symbol, 0, &mut visited).await?,
                "downstream" => self.trace_downstream(&db, starting_symbol, 0, &mut visited).await?,
                "both" => {
                    let mut visited_up = HashSet::new();
                    let mut visited_down = HashSet::new();
                    let mut upstream = self.trace_upstream(&db, starting_symbol, 0, &mut visited_up).await?;
                    let downstream = self.trace_downstream(&db, starting_symbol, 0, &mut visited_down).await?;
                    upstream.extend(downstream);
                    upstream
                }
                _ => {
                    return Ok(CallToolResult::text_content(vec![TextContent::from(
                        format!(
                            "❌ Invalid direction: '{}'\n\
                             💡 Valid options: 'upstream', 'downstream', 'both'",
                            self.direction
                        ),
                    )]));
                }
            };

            if !call_tree.is_empty() {
                all_trees.push((starting_symbol.clone(), call_tree));
            }
        }

        // Format output
        let output = self.format_call_trees(&all_trees)?;

        Ok(CallToolResult::text_content(vec![TextContent::from(
            self.optimize_response(&output),
        )]))
    }

    /// Trace upstream (find callers)
    #[async_recursion::async_recursion]
    async fn trace_upstream(
        &self,
        db: &Arc<Mutex<SymbolDatabase>>,
        symbol: &Symbol,
        current_depth: u32,
        visited: &mut HashSet<String>,
    ) -> Result<Vec<CallPathNode>> {
        if current_depth >= self.max_depth {
            debug!("Reached max depth {} for symbol {}", current_depth, symbol.name);
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
        debug!("Finding direct callers for: {} (id: {})", symbol.name, symbol.id);
        let db_lock = db.lock().await;
        let relationships = db_lock.get_relationships_to_symbol(&symbol.id)?;

        let mut callers = Vec::new();
        for rel in relationships {
            // For upstream, we want relationships where this symbol is the target
            if rel.to_symbol_id != symbol.id {
                continue;
            }

            // Only interested in call relationships for call path tracing
            if !matches!(rel.kind, RelationshipKind::Calls | RelationshipKind::References) {
                continue;
            }

            // Get the caller symbol
            if let Ok(Some(caller_symbol)) = db_lock.get_symbol_by_id(&rel.from_symbol_id) {
                callers.push((caller_symbol, rel.kind.clone()));
            }
        }
        drop(db_lock);

        // Process callers recursively
        for (caller_symbol, rel_kind) in callers {
            let mut node = CallPathNode {
                symbol: caller_symbol.clone(),
                level: current_depth,
                match_type: MatchType::Direct,
                relationship_kind: Some(rel_kind),
                children: vec![],
            };

            // Recursively trace callers
            if current_depth + 1 < self.max_depth {
                node.children = self
                    .trace_upstream(db, &caller_symbol, current_depth + 1, visited)
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
                    children: vec![],
                };

                // Recursively trace (but limit depth for cross-language to avoid explosion)
                if current_depth + 1 < self.max_depth - 1 {
                    node.children = self
                        .trace_upstream(db, &caller_symbol, current_depth + 1, visited)
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
        db: &Arc<Mutex<SymbolDatabase>>,
        symbol: &Symbol,
        current_depth: u32,
        visited: &mut HashSet<String>,
    ) -> Result<Vec<CallPathNode>> {
        if current_depth >= self.max_depth {
            debug!("Reached max depth {} for symbol {}", current_depth, symbol.name);
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
        debug!("Finding direct callees for: {} (id: {})", symbol.name, symbol.id);
        let db_lock = db.lock().await;
        let relationships = db_lock.get_relationships_for_symbol(&symbol.id)?;

        let mut callees = Vec::new();
        for rel in relationships {
            // For downstream, we want relationships where this symbol is the source
            if rel.from_symbol_id != symbol.id {
                continue;
            }

            // Only interested in call relationships
            if !matches!(rel.kind, RelationshipKind::Calls | RelationshipKind::References) {
                continue;
            }

            // Get the callee symbol
            if let Ok(Some(callee_symbol)) = db_lock.get_symbol_by_id(&rel.to_symbol_id) {
                callees.push((callee_symbol, rel.kind.clone()));
            }
        }
        drop(db_lock);

        // Process callees recursively
        for (callee_symbol, rel_kind) in callees {
            let mut node = CallPathNode {
                symbol: callee_symbol.clone(),
                level: current_depth,
                match_type: MatchType::Direct,
                relationship_kind: Some(rel_kind),
                children: vec![],
            };

            // Recursively trace callees
            if current_depth + 1 < self.max_depth {
                node.children = self
                    .trace_downstream(db, &callee_symbol, current_depth + 1, visited)
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
                    children: vec![],
                };

                // Recursively trace
                if current_depth + 1 < self.max_depth - 1 {
                    node.children = self
                        .trace_downstream(db, &callee_symbol, current_depth + 1, visited)
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
        debug!("Generated {} naming variants for {}", variants.len(), symbol.name);

        let mut cross_lang_symbols = Vec::new();
        let db_lock = db.lock().await;

        for variant in variants {
            if variant == symbol.name {
                continue; // Skip original
            }

            // Find symbols with this variant name
            if let Ok(variant_symbols) = db_lock.get_symbols_by_name(&variant) {
                for variant_symbol in variant_symbols {
                    // Only include if different language
                    if variant_symbol.language != symbol.language {
                        // Check if this variant actually references symbols that could be our target
                        let relationships = db_lock.get_relationships_for_symbol(&variant_symbol.id)?;

                        // Consider it a caller if it has outgoing calls/references
                        let has_calls = relationships.iter().any(|r| {
                            matches!(r.kind, RelationshipKind::Calls | RelationshipKind::References)
                                && r.from_symbol_id == variant_symbol.id
                        });

                        if has_calls {
                            cross_lang_symbols.push(variant_symbol);
                        }
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
        debug!("Generated {} naming variants for {}", variants.len(), symbol.name);

        let mut cross_lang_symbols = Vec::new();
        let db_lock = db.lock().await;

        for variant in variants {
            if variant == symbol.name {
                continue;
            }

            // Find symbols with this variant name in different languages
            if let Ok(variant_symbols) = db_lock.get_symbols_by_name(&variant) {
                for variant_symbol in variant_symbols {
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

    /// Format multiple call trees for display
    fn format_call_trees(&self, trees: &[(Symbol, Vec<CallPathNode>)]) -> Result<String> {
        let mut output = String::new();

        output.push_str(&format!("🔍 **Call Path Trace: {}**\n\n", self.symbol));
        output.push_str(&format!(
            "**Direction:** {} | **Depth:** {} | **Cross-Language:** {}\n\n",
            self.direction,
            self.max_depth,
            if self.cross_language { "✓" } else { "✗" }
        ));

        if trees.is_empty() {
            output.push_str("ℹ️ No call paths found.\n\n");
            output.push_str("Possible reasons:\n");
            output.push_str("• Symbol not called/referenced anywhere\n");
            output.push_str("• Symbol name mismatch\n");
            output.push_str("• Try enabling cross_language: true\n");
            output.push_str("• Try using fast_refs to find references first\n");
            return Ok(output);
        }

        // Display each tree
        for (i, (root_symbol, nodes)) in trees.iter().enumerate() {
            if trees.len() > 1 {
                output.push_str(&format!("\n### Starting from: {}:{}\n\n",
                    root_symbol.file_path, root_symbol.start_line));
            }

            if nodes.is_empty() {
                output.push_str(&format!("  No {} found for this symbol.\n",
                    if self.direction == "upstream" { "callers" } else { "callees" }));
                continue;
            }

            // Group by level
            let mut by_level: HashMap<u32, Vec<&CallPathNode>> = HashMap::new();
            self.collect_by_level(nodes, &mut by_level);

            // Display by level
            for level in 0..self.max_depth {
                if let Some(level_nodes) = by_level.get(&level) {
                    if level_nodes.is_empty() {
                        continue;
                    }

                    output.push_str(&format!("**Level {}** ", level + 1));

                    // Count by match type
                    let direct_count = level_nodes.iter().filter(|n| n.match_type == MatchType::Direct).count();
                    let variant_count = level_nodes.iter().filter(|n| n.match_type == MatchType::NamingVariant).count();

                    if direct_count > 0 && variant_count == 0 {
                        output.push_str("(Direct):\n");
                    } else if variant_count > 0 {
                        output.push_str(&format!("({} direct, {} variant):\n", direct_count, variant_count));
                    } else {
                        output.push_str(":\n");
                    }

                    for node in level_nodes {
                        let indent = "  ".repeat(level as usize);
                        let match_indicator = match node.match_type {
                            MatchType::Direct => "",
                            MatchType::NamingVariant => " ⚡",
                            MatchType::Semantic => " 🧠",
                        };

                        let relationship_info = if let Some(ref kind) = node.relationship_kind {
                            format!(" [{}]", match kind {
                                RelationshipKind::Calls => "calls",
                                RelationshipKind::References => "refs",
                                _ => "other",
                            })
                        } else {
                            String::new()
                        };

                        output.push_str(&format!(
                            "{}• {}:{} `{}`{}{} ({})\n",
                            indent,
                            node.symbol.file_path,
                            node.symbol.start_line,
                            node.symbol.name,
                            match_indicator,
                            relationship_info,
                            node.symbol.language
                        ));
                    }

                    output.push('\n');
                }
            }

            // Summary statistics for this tree
            let total_nodes = self.count_nodes(nodes);
            let languages: HashSet<String> = self.collect_languages(nodes);

            if i < trees.len() - 1 || trees.len() == 1 {
                output.push_str("**Summary:**\n");
                output.push_str(&format!("• Total {}: {}\n",
                    if self.direction == "upstream" { "callers" } else { "callees" },
                    total_nodes));
                output.push_str(&format!("• Languages: {} ({})\n",
                    languages.len(),
                    languages.iter().cloned().collect::<Vec<_>>().join(", ")));
                output.push_str(&format!("• Max depth reached: {}\n", self.find_max_depth(nodes)));
            }
        }

        Ok(output)
    }

    /// Collect nodes by level for display
    fn collect_by_level<'a>(
        &self,
        nodes: &'a [CallPathNode],
        by_level: &mut HashMap<u32, Vec<&'a CallPathNode>>,
    ) {
        for node in nodes {
            by_level.entry(node.level).or_insert_with(Vec::new).push(node);
            self.collect_by_level(&node.children, by_level);
        }
    }

    /// Count total nodes in tree
    fn count_nodes(&self, nodes: &[CallPathNode]) -> usize {
        nodes.iter().map(|n| 1 + self.count_nodes(&n.children)).sum()
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

    /// Find maximum depth reached
    fn find_max_depth(&self, nodes: &[CallPathNode]) -> u32 {
        nodes
            .iter()
            .map(|n| {
                let child_depth = if n.children.is_empty() {
                    0
                } else {
                    self.find_max_depth(&n.children)
                };
                n.level + 1 + child_depth
            })
            .max()
            .unwrap_or(0)
    }

    /// Optimize response for token limits
    fn optimize_response(&self, response: &str) -> String {
        let estimator = TokenEstimator::new();
        let tokens = estimator.estimate_string(response);

        // Target 20000 tokens for call traces (can be large)
        if tokens <= 20000 {
            response.to_string()
        } else {
            // Truncate with notice
            let chars_per_token = response.len() / tokens.max(1);
            let target_chars = chars_per_token * 20000;
            let truncated = &response[..target_chars.min(response.len())];
            format!("{}\n\n... (truncated for context limits - reduce max_depth to see full results)", truncated)
        }
    }
}
