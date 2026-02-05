//! Fast search tool for code intelligence
//!
//! Provides unified search across text and semantic methods with support for:
//! - Multiple search modes (text, semantic, hybrid)
//! - Language and file pattern filtering
//! - Line-level grep-style search
//! - Graceful degradation (CASCADE architecture)
//! - Per-workspace isolation

// Public API re-exports
pub use self::query::{matches_glob_pattern, preprocess_fallback_query};
pub use self::query_preprocessor::{
    PreprocessedQuery, QueryType, detect_query_type, preprocess_query,
    sanitize_query, validate_query,
};
pub use self::types::{LineMatch, LineMatchStrategy};

// Internal modules
pub(crate) mod formatting; // Exposed for testing
pub(crate) mod hybrid_search; // Exposed for testing
mod line_mode;
mod query;
pub mod query_preprocessor; // Public for testing
mod scoring;
pub(crate) mod semantic_search; // Exposed for testing
pub mod text_search;
mod types;

use anyhow::Result;
use schemars::JsonSchema;
use crate::mcp_compat::{CallToolResult, Content, CallToolResultExt, WithStructuredContent};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::handler::JulieServerHandler;
use crate::health::SystemStatus;
use crate::tools::shared::OptimizedResponse;

//******************//
//   Search Tools   //
//******************//

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct FastSearchTool {
    /// Search query (text or pattern)
    pub query: String,
    /// Search method: "auto" (default, detects from query), "text", "semantic", or "hybrid"
    #[serde(default = "default_search_method")]
    pub search_method: String,
    /// Language filter: "rust", "typescript", "javascript", "python", "java", "csharp", "php", "ruby", "swift", "kotlin", "go", "c", "cpp", "lua", "qml", "r", "sql", "html", "css", "vue", "bash", "gdscript", "dart", "zig"
    #[serde(default)]
    pub language: Option<String>,
    /// File pattern filter (glob syntax)
    #[serde(default)]
    pub file_pattern: Option<String>,
    /// Maximum results (default: 10, range: 1-500)
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// Workspace filter: "primary" (default) or workspace ID
    #[serde(default = "default_workspace")]
    pub workspace: Option<String>,
    /// Search target: "content" (default) or "definitions"
    #[serde(default = "default_search_target")]
    pub search_target: String,
    /// Output format: "symbols" (default) or "lines"
    #[serde(default = "default_output")]
    pub output: Option<String>,
    /// Context lines before/after match (default: 1)
    #[serde(default = "default_context_lines")]
    pub context_lines: Option<u32>,
    /// Output format: "lean" (default - grep-style text), "json", "toon", or "auto"
    #[serde(default = "default_output_format")]
    pub output_format: Option<String>,
}

fn default_limit() -> u32 {
    10 // Reduced from 15 with enhanced scoring (better quality = fewer results needed)
}
fn default_search_method() -> String {
    "auto".to_string()
}
fn default_workspace() -> Option<String> {
    Some("primary".to_string())
}
fn default_output() -> Option<String> {
    Some("symbols".to_string())
}
fn default_context_lines() -> Option<u32> {
    Some(1) // 1 before + match + 1 after = 3 total lines (minimal context)
}
fn default_output_format() -> Option<String> {
    None // None = lean format (grep-style text). Override with "json", "toon", or "auto"
}

fn default_search_target() -> String {
    "content".to_string() // fast_search focuses on content, fast_goto handles symbol definitions
}

/// Auto-detect optimal search method from query characteristics.
///
/// Detection logic:
/// - If query contains code pattern chars → "text" (exact matching)
/// - Otherwise → "hybrid" (best quality for general search)
///
/// Code pattern indicators: `: < > [ ] ( ) { } => ?. &&`
///
/// # Examples
/// ```
/// use julie::tools::search::detect_search_method;
///
/// assert_eq!(detect_search_method(": BaseClass"), "text");
/// assert_eq!(detect_search_method("ILogger<"), "text");
/// assert_eq!(detect_search_method("[Fact]"), "text");
/// assert_eq!(detect_search_method("authentication logic"), "hybrid");
/// ```
pub fn detect_search_method(query: &str) -> &'static str {
    // Multi-char patterns (check first to avoid false positives)
    let multi_char_patterns = ["=>", "?.", "&&"];
    for pattern in &multi_char_patterns {
        if query.contains(pattern) {
            return "text";
        }
    }

    // Single-char patterns
    let single_char_patterns = [':', '<', '>', '[', ']', '(', ')', '{', '}'];
    if query.chars().any(|c| single_char_patterns.contains(&c)) {
        return "text";
    }

    // Default to hybrid (best quality for natural language)
    "hybrid"
}

impl FastSearchTool {
    /// Preprocess query for FTS5 fallback search (exposed for testing)
    pub fn preprocess_fallback_query(&self, query: &str) -> String {
        query::preprocess_fallback_query(query)
    }

    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!(
            "🔍 Fast search: {} (method: {})",
            self.query, self.search_method
        );

        // Determine target workspace for health check
        // If workspace parameter specified, check that workspace; otherwise check primary
        let target_workspace_id = if self.workspace.is_some() {
            // Resolve workspace filter to get actual workspace ID
            self.resolve_workspace_filter(handler)
                .await?
                .and_then(|ids| ids.first().cloned())
        } else {
            None
        };

        // Check system readiness with graceful degradation (workspace-aware!)
        let readiness = crate::health::HealthChecker::check_system_readiness(
            handler,
            target_workspace_id.as_deref(),
        )
        .await?;

        match readiness {
            SystemStatus::NotReady => {
                if self.output.as_deref() == Some("lines") {
                    debug!(
                        "Line-mode search requested before readiness; attempting SQLite fallback"
                    );
                } else {
                    let message = "❌ Workspace not indexed yet!\n💡 Run 'manage_workspace index' first to enable fast search.";
                    return Ok(CallToolResult::text_content(vec![Content::text(
                        message,
                    )]));
                }
            }
            SystemStatus::SqliteOnly { symbol_count } => {
                // Graceful degradation: Use SQLite FTS5 for search
                debug!(
                    "🔍 Using SQLite FTS5 search ({} symbols available)",
                    symbol_count
                );
            }
            SystemStatus::FullyReady { symbol_count } => {
                debug!(
                    "✅ All systems ready ({} symbols, embeddings available)",
                    symbol_count
                );
            }
        }

        // Check output format - if "lines" mode, use FTS5 directly for line-level results
        if self.output.as_deref() == Some("lines") {
            debug!("📄 Line-level output mode requested");
            return line_mode::line_mode_search(
                &self.query,
                &self.language,
                &self.file_pattern,
                self.limit,
                &self.workspace,
                handler,
            )
            .await;
        }

        // Auto-detect search method if needed
        let search_method = if self.search_method == "auto" {
            let detected = detect_search_method(&self.query);
            debug!("🔍 Auto-detected search method: {} (query: {})", detected, self.query);
            detected
        } else {
            self.search_method.as_str()
        };

        // Perform search based on search method
        let symbols = match search_method {
            "semantic" => {
                let workspace_ids = self.resolve_workspace_filter(handler).await?;
                semantic_search::semantic_search_impl(
                    &self.query,
                    &self.language,
                    &self.file_pattern,
                    self.limit,
                    workspace_ids,
                    handler,
                )
                .await?
            }
            "hybrid" => {
                let workspace_ids = self.resolve_workspace_filter(handler).await?;
                hybrid_search::hybrid_search_impl(
                    &self.query,
                    &self.language,
                    &self.file_pattern,
                    self.limit,
                    workspace_ids,
                    &self.search_target,
                    self.context_lines,
                    handler,
                )
                .await?
            }
            _ => {
                // "text" or any other mode defaults to text search
                let workspace_ids = self.resolve_workspace_filter(handler).await?;
                text_search::text_search_impl(
                    &self.query,
                    &self.language,
                    &self.file_pattern,
                    self.limit,
                    workspace_ids,
                    &self.search_target,
                    self.context_lines,
                    handler,
                )
                .await?
            }
        };

        // Truncate code_context to save tokens (default: 3 lines total)
        let symbols = formatting::truncate_code_context(symbols, self.context_lines);

        // Create optimized response with confidence scoring
        let confidence = scoring::calculate_search_confidence(&self.query, &symbols);
        let mut optimized = OptimizedResponse::new("fast_search", symbols, confidence);

        // Add insights based on patterns found (includes .julieignore hint for low-quality results)
        if let Some(insights) = scoring::generate_search_insights(&optimized.results, confidence) {
            optimized = optimized.with_insights(insights);
        }

        // Add smart next actions
        let next_actions = scoring::suggest_next_actions(&self.query, &optimized.results);
        optimized = optimized.with_next_actions(next_actions);

        // Optimize for tokens
        optimized.optimize_for_tokens(Some(self.limit as usize));

        if optimized.results.is_empty() {
            // Semantic fallback: If text search returns 0 results, try semantic search
            if self.search_method == "text" {
                debug!("🔄 Text search returned 0 results, attempting semantic fallback");

                let workspace_ids = self.resolve_workspace_filter(handler).await?;
                match semantic_search::semantic_search_impl(
                    &self.query,
                    &self.language,
                    &self.file_pattern,
                    self.limit,
                    workspace_ids,
                    handler,
                )
                .await
                {
                    Ok(semantic_symbols) if !semantic_symbols.is_empty() => {
                        debug!(
                            "✅ Semantic fallback found {} results",
                            semantic_symbols.len()
                        );

                        // Truncate code_context to save tokens
                        let semantic_symbols =
                            formatting::truncate_code_context(semantic_symbols, self.context_lines);

                        // Create optimized response with confidence scoring
                        let confidence =
                            scoring::calculate_search_confidence(&self.query, &semantic_symbols);
                        let mut optimized =
                            OptimizedResponse::new("fast_search", semantic_symbols, confidence);

                        // Add fallback message to insights
                        let fallback_message = "🔄 Text search returned 0 results. Showing semantic matches instead.\n💡 Semantic search finds conceptually similar code even when exact terms don't match.";
                        optimized = optimized.with_insights(fallback_message.to_string());

                        // Add insights based on patterns found
                        if let Some(insights) =
                            scoring::generate_search_insights(&optimized.results, confidence)
                        {
                            // Append to existing insights
                            let combined_insights = format!("{}\n\n{}", fallback_message, insights);
                            optimized = optimized.with_insights(combined_insights);
                        }

                        // Add smart next actions
                        let next_actions =
                            scoring::suggest_next_actions(&self.query, &optimized.results);
                        optimized = optimized.with_next_actions(next_actions);

                        // Optimize for tokens
                        optimized.optimize_for_tokens(Some(self.limit as usize));

                        // Return based on output_format (same logic as main return)
                        return match self.output_format.as_deref() {
                            None | Some("lean") => {
                                // Lean mode (DEFAULT): Grep-style text output
                                let lean_output = formatting::format_lean_search_results(&self.query, &optimized);
                                debug!(
                                    "✅ Returning lean search results ({} chars, {} results)",
                                    lean_output.len(),
                                    optimized.results.len()
                                );
                                Ok(CallToolResult::text_content(vec![Content::text(lean_output)]))
                            }
                            Some("toon") => {
                                // TOON mode: Compact tabular format
                                let toon_response = formatting::ToonResponse {
                                    tool: optimized.tool.clone(),
                                    results: optimized.results.iter().map(formatting::ToonSymbol::from).collect(),
                                    confidence: optimized.confidence,
                                    total_found: optimized.total_found,
                                    insights: optimized.insights.clone(),
                                    next_actions: optimized.next_actions.clone(),
                                };

                                match toon_format::encode_default(&toon_response) {
                                    Ok(toon) => {
                                        debug!("✅ Encoded search results to TOON ({} chars)", toon.len());
                                        Ok(CallToolResult::text_content(vec![Content::text(toon)]))
                                    }
                                    Err(e) => {
                                        warn!("❌ TOON encoding failed: {}, falling back to lean format", e);
                                        let lean_output = formatting::format_lean_search_results(&self.query, &optimized);
                                        Ok(CallToolResult::text_content(vec![Content::text(lean_output)]))
                                    }
                                }
                            }
                            Some("auto") => {
                                // Auto mode: TOON for 10+ results, lean otherwise
                                if optimized.results.len() >= 10 {
                                    let toon_response = formatting::ToonResponse {
                                        tool: optimized.tool.clone(),
                                        results: optimized.results.iter().map(formatting::ToonSymbol::from).collect(),
                                        confidence: optimized.confidence,
                                        total_found: optimized.total_found,
                                        insights: optimized.insights.clone(),
                                        next_actions: optimized.next_actions.clone(),
                                    };

                                    match toon_format::encode_default(&toon_response) {
                                        Ok(toon) => {
                                            debug!(
                                                "✅ Auto-selected TOON for {} results ({} chars)",
                                                optimized.results.len(),
                                                toon.len()
                                            );
                                            return Ok(CallToolResult::text_content(vec![Content::text(toon)]));
                                        }
                                        Err(e) => {
                                            warn!("❌ TOON encoding failed: {}, using lean format", e);
                                            // Fall through to lean
                                        }
                                    }
                                }

                                // Default to lean for small/medium responses
                                let lean_output = formatting::format_lean_search_results(&self.query, &optimized);
                                debug!(
                                    "✅ Auto-selected lean for {} results ({} chars)",
                                    optimized.results.len(),
                                    lean_output.len()
                                );
                                Ok(CallToolResult::text_content(vec![Content::text(lean_output)]))
                            }
                            Some("json") => {
                                // JSON mode: Full structured content
                                let structured = serde_json::to_value(&optimized)?;
                                let structured_map = if let serde_json::Value::Object(map) = structured {
                                    map
                                } else {
                                    return Err(anyhow::anyhow!("Expected JSON object"));
                                };
                                debug!(
                                    "✅ Returning search results as JSON ({} results)",
                                    optimized.results.len()
                                );
                                Ok(CallToolResult::text_content(vec![]).with_structured_content(structured_map))
                            }
                            Some(unknown) => {
                                // Unknown format - warn and use lean
                                warn!(
                                    "⚠️ Unknown output_format '{}', using lean format",
                                    unknown
                                );
                                let lean_output = formatting::format_lean_search_results(&self.query, &optimized);
                                Ok(CallToolResult::text_content(vec![Content::text(lean_output)]))
                            }
                        };
                    }
                    Ok(_) => {
                        debug!("⚠️ Semantic fallback also returned 0 results");
                    }
                    Err(e) => {
                        debug!("⚠️ Semantic fallback failed: {}", e);
                    }
                }
            }

            // If we get here, either not text mode or semantic fallback failed/returned 0
            let message = format!(
                "🔍 No results found for: '{}'\n\
                💡 Try a broader search term, different mode, or check spelling",
                self.query
            );
            return Ok(CallToolResult::text_content(vec![Content::text(
                message,
            )]));
        }

        // Return based on output_format
        // Default is "lean" - optimized text format for AI agent consumption
        match self.output_format.as_deref() {
            None | Some("lean") => {
                // Lean mode (DEFAULT): Grep-style text output, minimal tokens, maximum readability
                let lean_output = formatting::format_lean_search_results(&self.query, &optimized);
                debug!(
                    "✅ Returning lean search results ({} chars, {} results)",
                    lean_output.len(),
                    optimized.results.len()
                );
                Ok(CallToolResult::text_content(vec![Content::text(lean_output)]))
            }
            Some("toon") => {
                // TOON mode: Compact tabular format for structured data needs
                let toon_response = formatting::ToonResponse {
                    tool: optimized.tool.clone(),
                    results: optimized
                        .results
                        .iter()
                        .map(formatting::ToonSymbol::from)
                        .collect(),
                    confidence: optimized.confidence,
                    total_found: optimized.total_found,
                    insights: optimized.insights.clone(),
                    next_actions: optimized.next_actions.clone(),
                };

                match toon_format::encode_default(&toon_response) {
                    Ok(toon) => {
                        debug!("✅ Encoded search results to TOON ({} chars)", toon.len());
                        Ok(CallToolResult::text_content(vec![Content::text(toon)]))
                    }
                    Err(e) => {
                        warn!(
                            "❌ TOON encoding failed: {}, falling back to lean format",
                            e
                        );
                        // Fall back to lean instead of JSON
                        let lean_output =
                            formatting::format_lean_search_results(&self.query, &optimized);
                        Ok(CallToolResult::text_content(vec![Content::text(lean_output)]))
                    }
                }
            }
            Some("auto") => {
                // Auto mode: Lean for most cases, TOON only for very large results (10+)
                if optimized.results.len() >= 10 {
                    let toon_response = formatting::ToonResponse {
                        tool: optimized.tool.clone(),
                        results: optimized
                            .results
                            .iter()
                            .map(formatting::ToonSymbol::from)
                            .collect(),
                        confidence: optimized.confidence,
                        total_found: optimized.total_found,
                        insights: optimized.insights.clone(),
                        next_actions: optimized.next_actions.clone(),
                    };

                    match toon_format::encode_default(&toon_response) {
                        Ok(toon) => {
                            debug!(
                                "✅ Auto-selected TOON for {} results ({} chars)",
                                optimized.results.len(),
                                toon.len()
                            );
                            return Ok(CallToolResult::text_content(vec![Content::text(toon)]));
                        }
                        Err(e) => {
                            warn!("❌ TOON encoding failed: {}, using lean format", e);
                            // Fall through to lean
                        }
                    }
                }

                // Default to lean for small/medium responses
                let lean_output = formatting::format_lean_search_results(&self.query, &optimized);
                debug!(
                    "✅ Auto-selected lean for {} results ({} chars)",
                    optimized.results.len(),
                    lean_output.len()
                );
                Ok(CallToolResult::text_content(vec![Content::text(lean_output)]))
            }
            Some("json") => {
                // JSON mode: Full structured content for programmatic access
                let structured = serde_json::to_value(&optimized)?;
                let structured_map = if let serde_json::Value::Object(map) = structured {
                    map
                } else {
                    return Err(anyhow::anyhow!("Expected JSON object"));
                };
                debug!(
                    "✅ Returning search results as JSON ({} results)",
                    optimized.results.len()
                );
                Ok(CallToolResult::text_content(vec![]).with_structured_content(structured_map))
            }
            Some(unknown) => {
                // Unknown format - warn and use lean
                warn!(
                    "⚠️ Unknown output_format '{}', using lean format",
                    unknown
                );
                let lean_output = formatting::format_lean_search_results(&self.query, &optimized);
                Ok(CallToolResult::text_content(vec![Content::text(lean_output)]))
            }
        }
    }

    /// Resolve workspace filtering parameter to a list of workspace IDs
    async fn resolve_workspace_filter(
        &self,
        handler: &JulieServerHandler,
    ) -> Result<Option<Vec<String>>> {
        let workspace_param = self.workspace.as_deref().unwrap_or("primary");

        match workspace_param {
            "all" => {
                // Multi-workspace search is not supported - architectural decision
                // Use ManageWorkspaceTool for listing/managing multiple workspaces
                Err(anyhow::anyhow!(
                    "Searching all workspaces is not supported. Search one workspace at a time.\n\
                     Use 'primary' (default) or specify a specific workspace ID.\n\
                     To list available workspaces, use ManageWorkspaceTool with operation='list'."
                ))
            }
            "primary" => {
                // Resolve primary workspace ID for precise workspace filtering
                let workspace = handler.get_workspace().await?;
                if let Some(workspace) = workspace {
                    let registry_service =
                        crate::workspace::registry_service::WorkspaceRegistryService::new(
                            workspace.root.clone(),
                        );
                    match registry_service.get_primary_workspace_id().await? {
                        Some(workspace_id) => {
                            debug!("🔍 Resolved primary workspace to ID: {}", workspace_id);
                            Ok(Some(vec![workspace_id]))
                        }
                        None => {
                            debug!("🔍 No primary workspace ID found, using SQLite FTS5 search");
                            Ok(None)
                        }
                    }
                } else {
                    debug!("🔍 No workspace available, using SQLite FTS5 search");
                    Ok(None)
                }
            }
            workspace_id => {
                // Validate the workspace ID exists
                if let Some(primary_workspace) = handler.get_workspace().await? {
                    let registry_service =
                        crate::workspace::registry_service::WorkspaceRegistryService::new(
                            primary_workspace.root.clone(),
                        );

                    // Check if it's a valid workspace ID
                    match registry_service.get_workspace(workspace_id).await? {
                        Some(_) => Ok(Some(vec![workspace_id.to_string()])),
                        None => {
                            // Invalid workspace ID - provide fuzzy match suggestion
                            let all_workspaces = registry_service.get_all_workspaces().await?;
                            let workspace_ids: Vec<&str> =
                                all_workspaces.iter().map(|w| w.id.as_str()).collect();

                            if let Some((best_match, distance)) =
                                crate::utils::string_similarity::find_closest_match(
                                    workspace_id,
                                    &workspace_ids,
                                )
                            {
                                // Only suggest if the distance is reasonable (< 50% of query length)
                                if distance < workspace_id.len() / 2 {
                                    return Err(anyhow::anyhow!(
                                        "Workspace '{}' not found. Did you mean '{}'?",
                                        workspace_id,
                                        best_match
                                    ));
                                }
                            }

                            // No close match found
                            Err(anyhow::anyhow!(
                                "Workspace '{}' not found. Use 'primary' or a valid workspace ID",
                                workspace_id
                            ))
                        }
                    }
                } else {
                    Err(anyhow::anyhow!(
                        "No primary workspace found. Initialize workspace first."
                    ))
                }
            }
        }
    }
}
