use anyhow::Result;
use rust_mcp_sdk::macros::{mcp_tool, JsonSchema};
use rust_mcp_sdk::schema::{CallToolResult, TextContent};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::extractors::base::{Relationship, Symbol};
use crate::handler::JulieServerHandler;

use super::types::{BusinessLogicSymbol, FindLogicResult};

pub mod search;

// Re-export for use in tool
pub use search::MAX_GRAPH_ANALYSIS_CANDIDATES;

fn default_max_results() -> i32 {
    50
}

fn default_true() -> bool {
    true
}

fn default_min_score() -> f32 {
    0.3
}

#[mcp_tool(
    name = "find_logic",
    description = "DISCOVER CORE LOGIC - Filter framework noise, focus on domain business logic",
    title = "Find Business Logic"
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct FindLogicTool {
    /// Business domain keywords to search for
    /// Examples: "payment" for payment processing logic, "auth" for authentication, "user" for user management, "order" for order processing
    /// Can use multiple keywords: "payment checkout billing" for broader coverage
    pub domain: String,
    /// Maximum number of business logic symbols to return
    /// Higher values = more comprehensive results but longer response
    /// Recommended: 20-50 for focused analysis, 100+ for comprehensive review
    /// Default: 50 - balanced for most use cases
    #[serde(default = "default_max_results")]
    pub max_results: i32,
    /// Group results by architectural layer (controllers, services, models, etc.)
    /// true = organized by layer for architectural understanding
    /// false = flat list sorted by relevance score
    /// Default: true - better organization
    #[serde(default = "default_true")]
    pub group_by_layer: bool,
    /// Minimum business relevance score threshold (0.0 to 1.0)
    /// Higher values = more selective, only highly relevant business logic
    /// Recommended: 0.3 for broad coverage, 0.7 for core business logic only
    /// Default: 0.3 - broad coverage
    #[serde(default = "default_min_score")]
    pub min_business_score: f32,
}

impl FindLogicTool {
    /// Helper: Create structured result with markdown for dual output
    fn create_result(
        &self,
        business_symbols: Vec<Symbol>,
        intelligence_layers: Vec<String>,
        next_actions: Vec<String>,
        markdown: String,
    ) -> Result<CallToolResult> {
        let business_logic_symbols: Vec<BusinessLogicSymbol> = business_symbols
            .iter()
            .map(|symbol| BusinessLogicSymbol {
                name: symbol.name.clone(),
                kind: format!("{:?}", symbol.kind),
                language: symbol.language.clone(),
                file_path: symbol.file_path.clone(),
                start_line: symbol.start_line,
                confidence: symbol.confidence.unwrap_or(0.0),
                signature: symbol.signature.clone(),
            })
            .collect();

        let result = FindLogicResult {
            tool: "find_logic".to_string(),
            domain: self.domain.clone(),
            found_count: business_logic_symbols.len(),
            max_results: self.max_results as usize,
            min_business_score: self.min_business_score,
            group_by_layer: self.group_by_layer,
            intelligence_layers,
            business_symbols: business_logic_symbols,
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
            "🏢 🧠 SUPER GENIUS: Finding business logic for domain: {}",
            self.domain
        );

        // 🚀 MULTI-TIER INTELLIGENT SEARCH ARCHITECTURE
        // This replaces primitive O(n) filtering with intelligent indexed queries

        let mut candidates: Vec<Symbol> = Vec::new();
        let mut search_insights: Vec<String> = Vec::new();

        // ═══════════════════════════════════════════════════════════════════
        // TIER 1: Ultra-Fast Keyword Search (Tantivy + FTS5) - <10ms
        // ═══════════════════════════════════════════════════════════════════
        debug!("🔍 Tier 1: Ultra-fast keyword search via Tantivy/FTS5");
        match self.search_by_keywords(handler).await {
            Ok(keyword_matches) => {
                search_insights.push(format!("Keyword search: {} matches", keyword_matches.len()));
                candidates.extend(keyword_matches);
            }
            Err(e) => {
                debug!("⚠️ Tier 1 failed: {}", e);
                search_insights.push("Keyword search: unavailable".to_string());
            }
        }

        // ═══════════════════════════════════════════════════════════════════
        // TIER 2: Tree-Sitter AST Pattern Recognition - Architectural Intelligence
        // ═══════════════════════════════════════════════════════════════════
        debug!("🌳 Tier 2: Tree-sitter AST pattern recognition");
        match self.find_architectural_patterns(handler).await {
            Ok(ast_matches) => {
                search_insights.push(format!("AST patterns: {} matches", ast_matches.len()));
                candidates.extend(ast_matches);
            }
            Err(e) => {
                debug!("⚠️ Tier 2 failed: {}", e);
                search_insights.push("AST patterns: unavailable".to_string());
            }
        }

        // ═══════════════════════════════════════════════════════════════════
        // TIER 3: Path-Based Architectural Layer Detection
        // ═══════════════════════════════════════════════════════════════════
        debug!("🗂️ Tier 3: Applying path-based architectural intelligence");
        self.apply_path_intelligence(&mut candidates);

        // ═══════════════════════════════════════════════════════════════════
        // OPTIMIZATION: Cap Candidates Before Expensive Tier 5 Graph Analysis
        // ═══════════════════════════════════════════════════════════════════
        // Combined strategy: Filter by threshold + hard cap to prevent N-to-M explosion
        let original_count = candidates.len();

        // Strategy 1: Early filter by min_business_score (user-controlled)
        candidates.retain(|s| s.confidence.unwrap_or(0.0) >= self.min_business_score);
        debug!(
            "🔍 Filtered {} → {} candidates above threshold {:.1}",
            original_count,
            candidates.len(),
            self.min_business_score
        );

        // Strategy 2: Hard cap at 100 for graph analysis (prevents pathological cases)
        if candidates.len() > MAX_GRAPH_ANALYSIS_CANDIDATES {
            // Sort by score before truncating to keep best candidates
            candidates.sort_by(|a, b| {
                let score_a = a.confidence.unwrap_or(0.0);
                let score_b = b.confidence.unwrap_or(0.0);
                score_b
                    .partial_cmp(&score_a)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            candidates.truncate(MAX_GRAPH_ANALYSIS_CANDIDATES);
            debug!(
                "⚡ Capped to {} top candidates for graph analysis (performance protection)",
                MAX_GRAPH_ANALYSIS_CANDIDATES
            );
        }

        // ═══════════════════════════════════════════════════════════════════
        // TIER 4: Semantic HNSW Business Concept Matching - AI-Powered
        // ═══════════════════════════════════════════════════════════════════
        debug!("🧠 Tier 4: Semantic HNSW concept matching");
        match self.semantic_business_search(handler).await {
            Ok(semantic_matches) => {
                search_insights.push(format!(
                    "Semantic search: {} matches",
                    semantic_matches.len()
                ));
                candidates.extend(semantic_matches);
            }
            Err(e) => {
                debug!("⚠️ Tier 4 failed: {}", e);
                search_insights.push("Semantic search: unavailable".to_string());
            }
        }

        // ═══════════════════════════════════════════════════════════════════
        // TIER 5: Relationship Graph Centrality Analysis
        // ═══════════════════════════════════════════════════════════════════
        debug!("📊 Tier 5: Analyzing relationship graph for business importance");
        if let Err(e) = self
            .analyze_business_importance(&mut candidates, handler)
            .await
        {
            debug!("⚠️ Tier 5 failed: {}", e);
            search_insights.push("Graph analysis: unavailable".to_string());
        } else {
            search_insights.push("Graph analysis: complete".to_string());
        }

        // ═══════════════════════════════════════════════════════════════════
        // FINAL PROCESSING: Deduplicate, Score, Rank, and Limit
        // ═══════════════════════════════════════════════════════════════════
        candidates = self.deduplicate_and_rank(candidates);

        // Filter by minimum business score threshold
        let business_symbols: Vec<Symbol> = candidates
            .into_iter()
            .filter(|s| s.confidence.unwrap_or(0.0) >= self.min_business_score)
            .take(self.max_results as usize)
            .collect();

        // Get relationships between business logic symbols
        let business_relationships = self
            .get_business_relationships(&business_symbols, handler)
            .await?;

        // Format with intelligence insights
        let mut message = "🧠 SUPER GENIUS Business Logic Discovery\n".to_string();
        message.push_str(&format!(
            "🔬 Intelligence Layers: {}\n\n",
            search_insights.join(" | ")
        ));
        message
            .push_str(&self.format_optimized_results(&business_symbols, &business_relationships));

        self.create_result(
            business_symbols,
            search_insights,
            vec![
                "Review business logic symbols".to_string(),
                "Use fast_goto to navigate to definitions".to_string(),
                "Use fast_refs to see usage patterns".to_string(),
            ],
            message,
        )
    }

    /// Get relationships between business logic symbols using intelligent queries
    async fn get_business_relationships(
        &self,
        business_symbols: &[Symbol],
        handler: &JulieServerHandler,
    ) -> Result<Vec<Relationship>> {
        let workspace = handler
            .get_workspace()
            .await?
            .ok_or_else(|| anyhow::anyhow!("No workspace available"))?;
        let db = workspace
            .db
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No database available"))?;
        let db_lock = db.lock().unwrap();

        let business_symbol_ids: std::collections::HashSet<String> =
            business_symbols.iter().map(|s| s.id.clone()).collect();

        let mut relationships: Vec<Relationship> = Vec::new();

        // Use targeted queries instead of loading ALL relationships
        for symbol in business_symbols {
            if let Ok(symbol_rels) = db_lock.get_relationships_for_symbol(&symbol.id) {
                for rel in symbol_rels {
                    // Only include relationships where both ends are business symbols
                    if business_symbol_ids.contains(&rel.from_symbol_id)
                        && business_symbol_ids.contains(&rel.to_symbol_id)
                    {
                        relationships.push(rel);
                    }
                }
            }
        }

        debug!("🔗 Found {} business relationships", relationships.len());
        Ok(relationships)
    }

    /// Format optimized results for FindLogicTool
    pub fn format_optimized_results(
        &self,
        symbols: &[Symbol],
        _relationships: &[Relationship],
    ) -> String {
        if symbols.is_empty() {
            return format!(
                "No business logic found for domain '{}'\nTry lowering min_business_score or different keywords",
                self.domain
            );
        }

        // Get top symbol names for summary
        let top_symbols: Vec<String> = symbols
            .iter()
            .take(5)
            .map(|s| s.name.clone())
            .collect();

        format!(
            "Found {} business logic components for '{}'\nTop: {}",
            symbols.len(),
            self.domain,
            top_symbols.join(", ")
        )
    }
}
