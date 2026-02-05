use anyhow::Result;
use schemars::JsonSchema;
use crate::mcp_compat::CallToolResult;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::extractors::base::{Relationship, Symbol};
use crate::handler::JulieServerHandler;
use crate::tools::shared::create_toonable_result;

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

fn default_output_format() -> Option<String> {
    Some("auto".to_string())
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct FindLogicTool {
    /// Business domain keywords to search for
    /// Examples: "payment" for payment processing logic, "auth" for authentication, "user" for user management, "order" for order processing
    /// Can use multiple keywords: "payment checkout billing" for broader coverage
    pub domain: String,
    /// Maximum number of business logic symbols to return (default: 50).
    /// Higher values = more comprehensive results but longer response
    /// Recommended: 20-50 for focused analysis, 100+ for comprehensive review
    #[serde(default = "default_max_results")]
    pub max_results: i32,
    /// Group results by architectural layer (default: true).
    /// true = organized by layer (controllers, services, models) for architectural understanding
    /// false = flat list sorted by relevance score
    #[serde(default = "default_true")]
    pub group_by_layer: bool,
    /// Minimum business relevance score threshold (default: 0.3, range: 0.0-1.0).
    /// Higher values = more selective, only highly relevant business logic
    /// Recommended: 0.3 for broad coverage, 0.7 for core business logic only
    #[serde(default = "default_min_score")]
    pub min_business_score: f32,
    /// Output format: "json", "toon", or "auto" (default - TOON for 5+ results)
    #[serde(default = "default_output_format")]
    pub output_format: Option<String>,
}

impl FindLogicTool {
    /// Detect architectural layer from file path
    fn detect_architectural_layer(file_path: &str) -> &'static str {
        let path_lower = file_path.to_lowercase();
        if path_lower.contains("/controller") || path_lower.ends_with("controller.") {
            "Controllers"
        } else if path_lower.contains("/service") || path_lower.ends_with("service.") {
            "Services"
        } else if path_lower.contains("/model") || path_lower.ends_with("model.") {
            "Models"
        } else if path_lower.contains("/repository") || path_lower.ends_with("repository.") {
            "Repositories"
        } else if path_lower.contains("/util") || path_lower.contains("/utils") {
            "Utilities"
        } else if path_lower.contains("/handler") || path_lower.ends_with("handler.") {
            "Handlers"
        } else if path_lower.contains("/middleware") {
            "Middleware"
        } else {
            "Other"
        }
    }

    /// Helper: Create structured result with markdown for dual output
    fn create_result(
        &self,
        business_logic_symbols: Vec<BusinessLogicSymbol>,
        intelligence_layers: Vec<String>,
        next_actions: Vec<String>,
        _markdown: String,
        output_format: Option<&str>,
    ) -> Result<CallToolResult> {
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

        // Use shared TOON/JSON formatter
        let toon_flat = result.to_toon_flat();
        create_toonable_result(
            &result,      // JSON data (full metadata + results)
            &toon_flat,   // TOON data (just the business_symbols array)
            output_format,
            5,  // Auto threshold: 5+ results use TOON
            result.found_count,
            "find_logic"
        )
    }

    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!(
            "🏢 🧠 SUPER GENIUS: Finding business logic for domain: {}",
            self.domain
        );

        // 🚀 4-TIER INTELLIGENT SEARCH ARCHITECTURE
        // This replaces primitive O(n) filtering with intelligent indexed queries

        let mut candidates: Vec<Symbol> = Vec::new();
        let mut search_insights: Vec<String> = Vec::new();

        // ═══════════════════════════════════════════════════════════════════
        // TIER 1: Ultra-Fast Keyword Search (SQLite FTS5) - <10ms
        // ═══════════════════════════════════════════════════════════════════
        debug!("🔍 Tier 1: Ultra-fast keyword search via FTS5");
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
        // OPTIMIZATION: Cap Candidates Before Expensive Tier 4 Graph Analysis
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
        // TIER 4: Relationship Graph Centrality Analysis
        // ═══════════════════════════════════════════════════════════════════
        debug!("📊 Tier 4: Analyzing relationship graph for business importance");
        if let Err(e) = self
            .analyze_business_importance(&mut candidates, handler)
            .await
        {
            debug!("⚠️ Tier 4 failed: {}", e);
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

        let business_logic_symbols: Vec<BusinessLogicSymbol> = business_symbols
            .iter()
            .map(|symbol| BusinessLogicSymbol {
                name: symbol.name.clone(),
                kind: format!("{:?}", symbol.kind),
                language: symbol.language.clone(),
                file_path: symbol.file_path.clone(),
                start_line: symbol.start_line,
                confidence: Some(symbol.confidence.unwrap_or(0.0)), // Wrapped in Some for TOON compatibility
                signature: symbol.signature.clone(),
            })
            .collect();

        // Format with intelligence insights
        let mut message = "🧠 SUPER GENIUS Business Logic Discovery\n".to_string();
        message.push_str(&format!(
            "🔬 Intelligence Layers: {}\n\n",
            search_insights.join(" | ")
        ));
        message.push_str(
            &self.format_optimized_results(&business_logic_symbols, &business_relationships),
        );

        self.create_result(
            business_logic_symbols,
            search_insights,
            vec![
                "Review business logic symbols".to_string(),
                "Use fast_goto to navigate to definitions".to_string(),
                "Use fast_refs to see usage patterns".to_string(),
            ],
            message,
            self.output_format.as_deref(),
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
        let db_lock = match db.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                warn!(
                    "Database mutex poisoned in get_business_relationships, recovering: {}",
                    poisoned
                );
                poisoned.into_inner()
            }
        };

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
        symbols: &[BusinessLogicSymbol],
        _relationships: &[Relationship],
    ) -> String {
        use std::collections::HashMap;

        if symbols.is_empty() {
            return format!(
                "No business logic found for domain '{}'\nTry lowering min_business_score or different keywords",
                self.domain
            );
        }

        if self.group_by_layer {
            // Group symbols by architectural layer
            let mut layers: HashMap<&str, Vec<&BusinessLogicSymbol>> = HashMap::new();
            for symbol in symbols {
                let layer = Self::detect_architectural_layer(&symbol.file_path);
                layers.entry(layer).or_default().push(symbol);
            }

            // Format grouped output
            let mut result = format!(
                "Found {} business logic components for '{}' (grouped by layer):\n\n",
                symbols.len(),
                self.domain
            );

            // Sort layers for consistent output
            let mut layer_names: Vec<&str> = layers.keys().copied().collect();
            layer_names.sort();

            for layer_name in layer_names {
                let layer_symbols = &layers[layer_name];
                result.push_str(&format!(
                    "## {} ({} components)\n",
                    layer_name,
                    layer_symbols.len()
                ));

                // Sort symbols by confidence score (highest first)
                let mut sorted_symbols = layer_symbols.clone();
                sorted_symbols.sort_by(|a, b| {
                    b.confidence
                        .partial_cmp(&a.confidence)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });

                for symbol in sorted_symbols.iter().take(10) {
                    // Show top 10 per layer
                    result.push_str(&format!(
                        "- **{}** ({}) - {} - {}:{}\n",
                        symbol.name,
                        symbol.kind,
                        symbol.language,
                        symbol.file_path,
                        symbol.start_line
                    ));
                    if let Some(sig) = &symbol.signature {
                        if !sig.is_empty() {
                            result.push_str(&format!("  └─ `{}`\n", sig));
                        }
                    }
                }

                if layer_symbols.len() > 10 {
                    result.push_str(&format!("... and {} more\n", layer_symbols.len() - 10));
                }
                result.push('\n');
            }

            result
        } else {
            // Flat list sorted by relevance score
            let mut sorted_symbols = symbols.to_vec();
            sorted_symbols.sort_by(|a, b| {
                b.confidence
                    .partial_cmp(&a.confidence)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            let mut result = format!(
                "Found {} business logic components for '{}' (sorted by relevance):\n\n",
                symbols.len(),
                self.domain
            );

            for symbol in sorted_symbols.iter().take(20) {
                // Show top 20 in flat view
                result.push_str(&format!(
                    "- **{}** ({}) - {} - {}:{} (score: {:.2})\n",
                    symbol.name,
                    symbol.kind,
                    symbol.language,
                    symbol.file_path,
                    symbol.start_line,
                    symbol.confidence.unwrap_or(0.0)
                ));
                if let Some(sig) = &symbol.signature {
                    if !sig.is_empty() {
                        result.push_str(&format!("  └─ `{}`\n", sig));
                    }
                }
            }

            if symbols.len() > 20 {
                result.push_str(&format!("\n... and {} more components", symbols.len() - 20));
            }

            result
        }
    }
}
