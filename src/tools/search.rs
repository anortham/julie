use rust_mcp_sdk::schema::{CallToolResult, TextContent};
use rust_mcp_sdk::macros::mcp_tool;
use rust_mcp_sdk::macros::JsonSchema;
use serde::{Deserialize, Serialize};
use anyhow::Result;
use tracing::{debug, warn};
use std::collections::HashMap;

use crate::handler::JulieServerHandler;
use crate::extractors::{Symbol, SymbolKind};
use crate::utils::{token_estimation::TokenEstimator, context_truncation::ContextTruncator, progressive_reduction::ProgressiveReducer, path_relevance::PathRelevanceScorer, exact_match_boost::ExactMatchBoost};
use super::shared::OptimizedResponse;

//******************//
//   Search Tools   //
//******************//

#[mcp_tool(
    name = "fast_search",
    description = "SEARCH BEFORE CODING - Find existing implementations to avoid duplication with lightning speed",
    title = "Fast Unified Search (Text + Semantic)",
    idempotent_hint = true,
    destructive_hint = false,
    open_world_hint = false,
    read_only_hint = true,
    meta = r#"{"category": "search", "performance": "sub_10ms"}"#
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct FastSearchTool {
    /// Search query supporting multiple patterns and code constructs.
    /// Examples: "getUserData", "handle*", "class UserService", "import React", "TODO", "async function"
    /// Supports: exact match, wildcards (*), camelCase tokenization, partial matching
    pub query: String,
    /// Search algorithm: "text" (exact/pattern match, <10ms), "semantic" (AI similarity, <100ms), "hybrid" (both, balanced)
    /// Default: "text" for speed. Use "semantic" when text search fails to find conceptually similar code.
    /// Use "hybrid" for comprehensive results when you need maximum coverage.
    #[serde(default = "default_text")]
    pub mode: String,
    /// Programming language filter (optional).
    /// Valid: "rust", "typescript", "javascript", "python", "java", "csharp", "php", "ruby", "swift", "kotlin", "go", "c", "cpp", "lua", "sql", "html", "css", "vue", "bash", "gdscript", "dart", "zig"
    /// Example: "typescript" to search only .ts/.tsx files
    #[serde(default)]
    pub language: Option<String>,
    /// File path pattern using glob syntax (optional).
    /// Examples: "src/", "*.test.ts", "**/components/**", "tests/", "!node_modules/"
    /// Supports: directories, extensions, nested paths, exclusions with !
    #[serde(default)]
    pub file_pattern: Option<String>,
    /// Maximum results to return (default: 50, range: 1-500).
    /// Lower = faster response, Higher = more comprehensive
    /// Tip: Start with default, increase if you need more results
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_limit() -> u32 { 50 }
fn default_text() -> String { "text".to_string() }

impl FastSearchTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!("🔍 Fast search: {} (mode: {})", self.query, self.mode);

        // Check if workspace is indexed
        let is_indexed = *handler.is_indexed.read().await;
        if !is_indexed {
            let message = "❌ Workspace not indexed yet!\n💡 Run index_workspace first to enable fast search.";
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        // Perform search based on mode
        let symbols = match self.mode.as_str() {
            "semantic" => self.semantic_search(handler).await?,
            "hybrid" => self.hybrid_search(handler).await?,
            "text" | _ => self.text_search(handler).await?,
        };

        // Create optimized response with confidence scoring
        let confidence = self.calculate_search_confidence(&symbols);
        let mut optimized = OptimizedResponse::new(symbols, confidence);

        // Add insights based on patterns found
        if let Some(insights) = self.generate_search_insights(&optimized.results) {
            optimized = optimized.with_insights(insights);
        }

        // Add smart next actions
        let next_actions = self.suggest_next_actions(&optimized.results);
        optimized = optimized.with_next_actions(next_actions);

        // Optimize for tokens
        optimized.optimize_for_tokens(Some(self.limit as usize));

        if optimized.results.is_empty() {
            let message = format!(
                "🔍 No results found for: '{}'\n\
                💡 Try a broader search term, different mode, or check spelling",
                self.query
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        // Format optimized results
        let message = self.format_optimized_results(&optimized);
        Ok(CallToolResult::text_content(vec![TextContent::from(message)]))
    }

    async fn text_search(&self, handler: &JulieServerHandler) -> Result<Vec<Symbol>> {
        // Use SearchEngine for FAST indexed search instead of O(n) linear scan!
        let search_engine = handler.search_engine.read().await;

        // Perform indexed search using Tantivy - this should be <10ms!
        let search_results = search_engine.search(&self.query).await.map_err(|e| {
            debug!("Search engine failed, falling back to linear search: {}", e);
            anyhow::anyhow!("Search failed: {}", e)
        });

        match search_results {
            Ok(results) => {
                // Use SearchResult symbols directly - no linear lookup needed!
                let mut matched_symbols = Vec::new();

                for search_result in results {
                    // Use the full Symbol from SearchResult directly - no linear lookup needed!
                    matched_symbols.push(search_result.symbol);
                }

                // Apply combined scoring: PathRelevanceScorer + ExactMatchBoost for optimal ranking
                let path_scorer = PathRelevanceScorer::new(&self.query);
                let exact_match_booster = ExactMatchBoost::new(&self.query);
                matched_symbols.sort_by(|a, b| {
                    // Combine path relevance (production vs test) with exact match boost
                    let path_score_a = path_scorer.calculate_score(&a.file_path);
                    let exact_boost_a = exact_match_booster.calculate_boost(&a.name);
                    let combined_score_a = path_score_a * exact_boost_a;

                    let path_score_b = path_scorer.calculate_score(&b.file_path);
                    let exact_boost_b = exact_match_booster.calculate_boost(&b.name);
                    let combined_score_b = path_score_b * exact_boost_b;

                    // Sort in descending order (higher combined scores first)
                    combined_score_b.partial_cmp(&combined_score_a).unwrap_or(std::cmp::Ordering::Equal)
                });

                debug!("🚀 Indexed search returned {} results (ranked by PathRelevanceScorer + ExactMatchBoost)", matched_symbols.len());
                Ok(matched_symbols)
            }
            Err(_) => {
                // Fallback to linear search if index fails
                warn!("⚠️  Search engine failed, using linear search fallback");
                let symbols = handler.symbols.read().await;
                let query_lower = self.query.to_lowercase();

                let mut results: Vec<Symbol> = symbols.iter()
                    .filter(|symbol| {
                        let name_match = symbol.name.to_lowercase().contains(&query_lower);
                        let language_match = self.language.as_ref()
                            .map(|lang| symbol.language.eq_ignore_ascii_case(lang))
                            .unwrap_or(true);
                        name_match && language_match
                    })
                    .cloned()
                    .collect();

                // Apply combined scoring: PathRelevanceScorer + ExactMatchBoost for optimal ranking
                let path_scorer = PathRelevanceScorer::new(&self.query);
                let exact_match_booster = ExactMatchBoost::new(&self.query);
                results.sort_by(|a, b| {
                    // Combine path relevance (production vs test) with exact match boost
                    let path_score_a = path_scorer.calculate_score(&a.file_path);
                    let exact_boost_a = exact_match_booster.calculate_boost(&a.name);
                    let combined_score_a = path_score_a * exact_boost_a;

                    let path_score_b = path_scorer.calculate_score(&b.file_path);
                    let exact_boost_b = exact_match_booster.calculate_boost(&b.name);
                    let combined_score_b = path_score_b * exact_boost_b;

                    // Sort in descending order (higher combined scores first)
                    combined_score_b.partial_cmp(&combined_score_a).unwrap_or(std::cmp::Ordering::Equal)
                });

                debug!("📝 Linear search fallback returned {} results (ranked by PathRelevanceScorer + ExactMatchBoost)", results.len());
                Ok(results)
            }
        }
    }

    async fn semantic_search(&self, handler: &JulieServerHandler) -> Result<Vec<Symbol>> {
        // For now, delegate to text search - full semantic implementation coming soon
        debug!("🧠 Semantic search mode (using text fallback)");
        self.text_search(handler).await
    }

    async fn hybrid_search(&self, handler: &JulieServerHandler) -> Result<Vec<Symbol>> {
        // For now, delegate to text search - full hybrid implementation coming soon
        debug!("🔄 Hybrid search mode (using text fallback)");
        self.text_search(handler).await
    }

    /// Calculate confidence score based on search quality and result relevance
    fn calculate_search_confidence(&self, symbols: &[Symbol]) -> f32 {
        if symbols.is_empty() { return 0.0; }

        let mut confidence: f32 = 0.5; // Base confidence

        // Exact name matches boost confidence
        let exact_matches = symbols.iter()
            .filter(|s| s.name.to_lowercase() == self.query.to_lowercase())
            .count();
        if exact_matches > 0 {
            confidence += 0.3;
        }

        // Partial matches are medium confidence
        let partial_matches = symbols.iter()
            .filter(|s| s.name.to_lowercase().contains(&self.query.to_lowercase()))
            .count();
        if partial_matches > exact_matches {
            confidence += 0.2;
        }

        // More results can indicate ambiguity (lower confidence)
        if symbols.len() > 20 {
            confidence -= 0.1;
        } else if symbols.len() < 5 {
            confidence += 0.1;
        }

        confidence.clamp(0.0, 1.0)
    }

    /// Generate intelligent insights about search patterns
    fn generate_search_insights(&self, symbols: &[Symbol]) -> Option<String> {
        if symbols.is_empty() { return None; }

        let mut insights = Vec::new();

        // Language distribution
        let mut lang_counts = HashMap::new();
        for symbol in symbols {
            *lang_counts.entry(&symbol.language).or_insert(0) += 1;
        }

        if lang_counts.len() > 1 {
            let main_lang = lang_counts.iter().max_by_key(|(_, count)| *count).unwrap();
            insights.push(format!("Found across {} languages (mainly {})",
                lang_counts.len(), main_lang.0));
        }

        // Kind distribution
        let mut kind_counts = HashMap::new();
        for symbol in symbols {
            *kind_counts.entry(&symbol.kind).or_insert(0) += 1;
        }

        if let Some((dominant_kind, count)) = kind_counts.iter().max_by_key(|(_, count)| *count) {
            if *count > symbols.len() / 2 {
                insights.push(format!("Mostly {:?}s ({} of {})",
                    dominant_kind, count, symbols.len()));
            }
        }

        if insights.is_empty() { None } else { Some(insights.join(", ")) }
    }

    /// Suggest intelligent next actions based on search results
    fn suggest_next_actions(&self, symbols: &[Symbol]) -> Vec<String> {
        let mut actions = Vec::new();

        if symbols.len() == 1 {
            actions.push("Use fast_goto to jump to definition".to_string());
            actions.push("Use fast_refs to see all usages".to_string());
        } else if symbols.len() > 1 {
            actions.push("Narrow search with language filter".to_string());
            actions.push("Use fast_refs on specific symbols".to_string());
        }

        // Check if we have functions that might be entry points
        if symbols.iter().any(|s| matches!(s.kind, SymbolKind::Function) && s.name.contains("main")) {
            actions.push("Use fast_explore to understand architecture".to_string());
        }

        if symbols.iter().any(|s| s.name.to_lowercase().contains(&self.query.to_lowercase())) {
            actions.push("Consider exact name match for precision".to_string());
        }

        actions
    }

    /// Format optimized response with insights and next actions
    pub fn format_optimized_results(&self, optimized: &OptimizedResponse<Symbol>) -> String {
        let mut lines = vec![
            format!("⚡ Fast Search: '{}' (mode: {})", self.query, self.mode),
        ];

        // Add insights if available
        if let Some(insights) = &optimized.insights {
            lines.push(format!("💡 {}", insights));
        }

        lines.push(String::new());

        // Token optimization: apply progressive reduction first, then early termination if needed
        let token_estimator = TokenEstimator::new();
        let token_limit: usize = 15000; // 15K token limit to stay within Claude's context window
        let progressive_reducer = ProgressiveReducer::new();

        // Calculate initial header tokens
        let header_text = lines.join("\n");
        let header_tokens = token_estimator.estimate_string(&header_text);
        let available_tokens = token_limit.saturating_sub(header_tokens);

        // Define token estimator function for symbols
        let estimate_symbols_tokens = |symbols: &[&Symbol]| -> usize {
            let mut total_tokens = 0;
            for (i, symbol) in symbols.iter().enumerate() {
                let mut symbol_text = String::new();
                symbol_text.push_str(&format!("{}. {} [{}]\n", i + 1, symbol.name, symbol.language));
                symbol_text.push_str(&format!("   📁 {}:{}-{}\n", symbol.file_path, symbol.start_line, symbol.end_line));

                if let Some(signature) = &symbol.signature {
                    symbol_text.push_str(&format!("   📝 {}\n", signature));
                }

                if let Some(context) = &symbol.code_context {
                    symbol_text.push_str("   📄 Context:\n");
                    let context_lines: Vec<String> = context.lines().map(|s| s.to_string()).collect();
                    let max_lines = 10;

                    if context_lines.len() > max_lines {
                        let truncated_lines: Vec<String> = context_lines.iter().take(max_lines).cloned().collect();
                        let lines_truncated = context_lines.len() - max_lines;
                        for context_line in &truncated_lines {
                            symbol_text.push_str(&format!("   {}\n", context_line));
                        }
                        symbol_text.push_str(&format!("   ({} more lines truncated)\n", lines_truncated));
                    } else {
                        for context_line in &context_lines {
                            symbol_text.push_str(&format!("   {}\n", context_line));
                        }
                    }
                }

                total_tokens += token_estimator.estimate_string(&symbol_text);
            }
            total_tokens
        };

        // Try progressive reduction first
        let symbol_refs: Vec<&Symbol> = optimized.results.iter().collect();
        let reduced_symbol_refs = progressive_reducer.reduce(&symbol_refs, available_tokens, estimate_symbols_tokens);

        let (symbols_to_show, reduction_message) = if reduced_symbol_refs.len() < optimized.results.len() {
            // Progressive reduction was applied
            let symbols: Vec<Symbol> = reduced_symbol_refs.into_iter().cloned().collect();
            let message = format!("📊 Showing {} of {} results (confidence: {:.1}) - Applied progressive reduction {} → {}",
                    symbols.len(), optimized.total_found, optimized.confidence, optimized.results.len(), symbols.len());
            (symbols, message)
        } else {
            // No reduction needed
            let message = format!("📊 Showing {} of {} results (confidence: {:.1})",
                    optimized.results.len(), optimized.total_found, optimized.confidence);
            (optimized.results.clone(), message)
        };

        lines[1] = reduction_message;

        // Format the symbols we decided to show
        for (i, symbol) in symbols_to_show.iter().enumerate() {
            lines.push(format!(
                "{}. {} [{}]",
                i + 1, symbol.name, symbol.language
            ));
            lines.push(format!(
                "   📁 {}:{}-{}",
                symbol.file_path, symbol.start_line, symbol.end_line
            ));

            if let Some(signature) = &symbol.signature {
                lines.push(format!("   📝 {}", signature));
            }

            // Add code context if available
            if let Some(context) = &symbol.code_context {
                lines.push("   📄 Context:".to_string());

                // Apply context truncation using ContextTruncator
                let truncator = ContextTruncator::new();
                let context_lines: Vec<String> = context.lines().map(|s| s.to_string()).collect();
                let max_lines = 10; // Max 10 lines per symbol

                if context_lines.len() > max_lines {
                    // Truncate and show truncation message
                    let truncated_lines = truncator.truncate_lines(&context_lines, max_lines);
                    let lines_truncated = context_lines.len() - max_lines;

                    // Add truncated lines
                    for context_line in &truncated_lines {
                        lines.push(format!("   {}", context_line));
                    }

                    // Add truncation message
                    lines.push(format!("   ({} more lines truncated)", lines_truncated));
                } else {
                    // Add all lines if within limit
                    for context_line in &context_lines {
                        lines.push(format!("   {}", context_line));
                    }
                }
            }

            lines.push(String::new());
        }

        // Add next actions
        if !optimized.next_actions.is_empty() {
            lines.push("🎯 Suggested next actions:".to_string());
            for action in &optimized.next_actions {
                lines.push(format!("   • {}", action));
            }
        }

        lines.join("\n")
    }
}