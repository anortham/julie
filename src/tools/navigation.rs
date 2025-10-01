use anyhow::Result;
use rust_mcp_sdk::macros::mcp_tool;
use rust_mcp_sdk::macros::JsonSchema;
use rust_mcp_sdk::schema::{CallToolResult, TextContent};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::extractors::{Relationship, Symbol, SymbolKind};
use crate::handler::JulieServerHandler;
use crate::utils::{
    cross_language_intelligence::generate_naming_variants,
    progressive_reduction::ProgressiveReducer,
    token_estimation::TokenEstimator,
};
use crate::workspace::registry_service::WorkspaceRegistryService;

//*********************//
// Navigation Tools    //
//*********************//

#[mcp_tool(
    name = "fast_goto",
    description = "JUMP TO SOURCE - Navigate directly to where symbols are defined with lightning speed",
    title = "Fast Navigate to Definition",
    idempotent_hint = true,
    destructive_hint = false,
    open_world_hint = false,
    read_only_hint = true,
    meta = r#"{"category": "navigation", "precision": "line_level"}"#
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct FastGotoTool {
    /// Symbol name to navigate to. Supports simple and qualified names.
    /// Examples: "UserService", "MyClass::method", "std::vector", "React.Component", "getUserData"
    /// Julie intelligently resolves across languages (Python imports, Rust use statements, TypeScript imports)
    pub symbol: String,
    /// Current file path for context (helps resolve ambiguous symbols).
    /// Example: "src/services/user.ts" when multiple "UserService" classes exist
    /// Format: Relative path from workspace root
    #[serde(default)]
    pub context_file: Option<String>,
    /// Line number in context file where symbol is referenced.
    /// Helps disambiguate when symbol appears multiple times in the same file.
    /// Example: 142 (line where "UserService" is imported or used)
    #[serde(default)]
    pub line_number: Option<u32>,
    /// Workspace filter (optional): "all" (search all workspaces), "primary" (primary only), or workspace ID
    /// Examples: "all", "primary", "project-b_a3f2b8c1"
    /// Default: "primary" - search only the primary workspace for focused results
    #[serde(default = "default_workspace")]
    pub workspace: Option<String>,
}

fn default_workspace() -> Option<String> {
    Some("primary".to_string())
}

impl FastGotoTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!("🎯 Finding definition for: {}", self.symbol);

        // Find symbol definitions (workspace resolution happens in find_definitions)
        let definitions = self.find_definitions(handler).await?;

        if definitions.is_empty() {
            let message = format!(
                "🔍 No definition found for: '{}'\n\
                💡 Check the symbol name and ensure it exists in the indexed files",
                self.symbol
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                message,
            )]));
        }

        // Format results
        let mut message = format!(
            "🎯 Found {} definition(s) for: '{}'\n\n",
            definitions.len(),
            self.symbol
        );

        for (i, symbol) in definitions.iter().enumerate() {
            message.push_str(&format!(
                "{}. {} [{}]\n\
                📁 {}:{}:{}\n\
                🏷️ Kind: {:?}\n",
                i + 1,
                symbol.name,
                symbol.language,
                symbol.file_path,
                symbol.start_line,
                symbol.start_column,
                symbol.kind
            ));

            if let Some(signature) = &symbol.signature {
                message.push_str(&format!("   📝 {}", signature));
            }
            message.push('\n');
        }

        Ok(CallToolResult::text_content(vec![TextContent::from(
            message,
        )]))
    }

    async fn find_definitions(&self, handler: &JulieServerHandler) -> Result<Vec<Symbol>> {
        debug!("🔍 Finding definitions for: {}", self.symbol);

        // Resolve workspace filtering
        let workspace_filter = self.resolve_workspace_filter(handler).await?;

        // If workspace filtering is specified, use database search for precise workspace isolation
        if let Some(workspace_ids) = workspace_filter {
            debug!("🎯 Using workspace-filtered database search for goto definition");
            return self.database_find_definitions(handler, workspace_ids).await;
        }

        // For "all" workspaces, use the existing search engine approach
        // Strategy 1: Use SearchEngine for O(log n) performance instead of O(n) linear scan
        let mut exact_matches = Vec::new();

        // Use indexed search for exact matches - MUCH faster than linear scan!
        match handler.active_search_engine().await {
            Ok(search_engine) => {
                let search_engine = search_engine.read().await;
                match search_engine.search(&self.symbol).await {
            Ok(search_results) => {
                // Use SearchResult's symbol directly - no O(n) linear lookup needed!
                for search_result in search_results {
                    // Only include exact name matches for definitions
                    if search_result.symbol.name == self.symbol {
                        exact_matches.push(search_result.symbol);
                    }
                }
                debug!(
                    "⚡ Indexed search found {} exact matches",
                    exact_matches.len()
                );
            }
                Err(e) => {
                    debug!("Search engine failed, falling back to SQLite database: {}", e);
                    // Fallback to database search for exact name lookup (indexed, fast)
                    if let Ok(workspace) = handler.get_workspace().await {
                        if let Some(workspace) = workspace {
                            if let Some(db) = workspace.db.as_ref() {
                                let db_lock = db.lock().await;
                                exact_matches = db_lock.get_symbols_by_name(&self.symbol).unwrap_or_default();
                            }
                        }
                    }
                }
            }
            }
            Err(e) => {
                debug!("Search engine unavailable, using SQLite database: {}", e);
                // Fallback to database search for exact name lookup (indexed, fast)
                if let Ok(workspace) = handler.get_workspace().await {
                    if let Some(workspace) = workspace {
                        if let Some(db) = workspace.db.as_ref() {
                            let db_lock = db.lock().await;
                            exact_matches = db_lock.get_symbols_by_name(&self.symbol).unwrap_or_default();
                        }
                    }
                }
            }
        }

        // OPTIMIZED: Query symbols by name FIRST using indexed query, then get relationships
        // This avoids loading ALL symbols and relationships (O(n) → O(log n))
        let matching_symbols = if let Ok(workspace) = handler.get_workspace().await {
            if let Some(workspace) = workspace {
                if let Some(db) = workspace.db.as_ref() {
                    let db_lock = db.lock().await;
                    // Use exact name match to find symbols
                    db_lock.find_symbols_by_name(&self.symbol).unwrap_or_default()
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        // Strategy 2: Use relationships to find actual definitions
        // PERFORMANCE FIX: Use targeted queries instead of loading ALL relationships
        // This changes from O(n) linear scan to O(k * log n) indexed queries where k = matching_symbols.len()
        if !matching_symbols.is_empty() {
            if let Ok(workspace) = handler.get_workspace().await {
                if let Some(workspace) = workspace {
                    if let Some(db) = workspace.db.as_ref() {
                        let db_lock = db.lock().await;

                        // Query relationships for each matching symbol using indexed query
                        for symbol in &matching_symbols {
                            if let Ok(relationships) = db_lock.get_relationships_for_symbol(&symbol.id) {
                                for relationship in relationships {
                                    // Check if this relationship represents a definition or import
                                    match &relationship.kind {
                                        crate::extractors::base::RelationshipKind::Imports |
                                        crate::extractors::base::RelationshipKind::Defines |
                                        crate::extractors::base::RelationshipKind::Extends => {
                                            // The symbol itself is the definition we want
                                            exact_matches.push(symbol.clone());
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Remove duplicates based on symbol id
        exact_matches.sort_by(|a, b| a.id.cmp(&b.id));
        exact_matches.dedup_by(|a, b| a.id == b.id);

        // Strategy 3: Cross-language resolution with naming conventions + semantic search
        // This leverages Julie's unique CASCADE architecture:
        // - Fast: Naming convention variants (Tantivy indexed search)
        // - Smart: Semantic embeddings (HNSW similarity)
        if exact_matches.is_empty() {
            debug!(
                "🌍 Attempting cross-language resolution for '{}'",
                self.symbol
            );

            // 3a. Try naming convention variants (fast, works across Python/JS/C#/Rust)
            // Examples: getUserData -> get_user_data (Python), GetUserData (C#)
            // Uses Julie's Intelligence Layer for smart variant generation
            match handler.active_search_engine().await {
                Ok(search_engine) => {
                    let search_engine = search_engine.read().await;

                    // Generate all naming convention variants using shared intelligence module
                    let variants = generate_naming_variants(&self.symbol);

                    for variant in variants {
                        if variant != self.symbol {
                            // Avoid duplicate searches
                            match search_engine.search(&variant).await {
                                Ok(search_results) => {
                                    for search_result in search_results {
                                        if search_result.symbol.name == variant {
                                            debug!("🎯 Found cross-language match: {} -> {}", self.symbol, variant);
                                            exact_matches.push(search_result.symbol);
                                        }
                                    }
                                }
                                Err(_) => {
                                    // Skip failed variant searches - not critical
                                    debug!("Variant search failed for: {}", variant);
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    debug!("Search engine not available for cross-language resolution: {}", e);
                    // Fall through to semantic search (Strategy 4)
                }
            }

            // 3b. If still no matches, embeddings will catch semantically similar symbols
            // (e.g., getUserData -> fetchUserInfo, retrieveUserDetails)
            // This happens automatically in Strategy 4 below
        }

        // Strategy 4: HNSW-powered semantic matching (FAST!)
        if exact_matches.is_empty() {
            debug!("🧠 Using HNSW semantic search for: {}", self.symbol);

            // Get embedding engine and vector store from workspace
            if let Ok(()) = handler.ensure_embedding_engine().await {
                if let Ok(workspace_opt) = handler.get_workspace().await {
                    if let Some(workspace) = workspace_opt {
                        // Get embedding for query
                        let mut embedding_guard = handler.embedding_engine.write().await;
                        if let Some(embedding_engine) = embedding_guard.as_mut() {
                            if let Ok(query_embedding) = embedding_engine.embed_text(&self.symbol) {
                                // Access vector store
                                if let Some(vector_store_arc) = &workspace.vector_store {
                                    let vector_store = vector_store_arc.read().await;

                                    // CASCADE fallback: HNSW (fast) → Brute-force (slower but works)
                                    let similar_symbols = if vector_store.has_hnsw_index() {
                                        // Fast path: Use HNSW for O(log n) approximate search
                                        debug!("🚀 Using HNSW index for fast semantic search");
                                        match vector_store.search_similar_hnsw(&query_embedding, 10, 0.7) {
                                            Ok(results) => {
                                                debug!("Found {} semantically similar symbols via HNSW", results.len());
                                                results
                                            }
                                            Err(e) => {
                                                debug!("HNSW search failed: {}, falling back to brute-force", e);
                                                // Fallback to brute-force if HNSW fails
                                                vector_store.search_similar(&query_embedding, 10, 0.7)
                                                    .unwrap_or_else(|e| {
                                                        debug!("Brute-force search also failed: {}", e);
                                                        Vec::new()
                                                    })
                                            }
                                        }
                                    } else {
                                        // Fallback path: Use brute-force O(n) search when HNSW not available
                                        debug!("⚠️ HNSW index not available - using brute-force semantic search");
                                        vector_store.search_similar(&query_embedding, 10, 0.7)
                                            .unwrap_or_else(|e| {
                                                debug!("Brute-force semantic search failed: {}", e);
                                                Vec::new()
                                            })
                                    };

                                    // Get actual symbol data from database for all results
                                    if !similar_symbols.is_empty() {
                                        debug!("📊 Processing {} similar symbols from semantic search", similar_symbols.len());
                                        if let Some(db_arc) = &workspace.db {
                                            let db = db_arc.lock().await;
                                            for result in similar_symbols {
                                                if let Ok(Some(symbol)) = db.get_symbol_by_id(&result.symbol_id) {
                                                    exact_matches.push(symbol);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Prioritize results
        exact_matches.sort_by(|a, b| {
            // First by definition priority (classes > functions > variables)
            let priority_cmp = self
                .definition_priority(&a.kind)
                .cmp(&self.definition_priority(&b.kind));
            if priority_cmp != std::cmp::Ordering::Equal {
                return priority_cmp;
            }

            // Then by context file preference if provided
            if let Some(context_file) = &self.context_file {
                let a_in_context = a.file_path.contains(context_file);
                let b_in_context = b.file_path.contains(context_file);
                match (a_in_context, b_in_context) {
                    (true, false) => return std::cmp::Ordering::Less,
                    (false, true) => return std::cmp::Ordering::Greater,
                    _ => {}
                }
            }

            // Finally by line number if provided (prefer definitions closer to context)
            if let Some(line_number) = self.line_number {
                let a_distance = (a.start_line as i32 - line_number as i32).abs();
                let b_distance = (b.start_line as i32 - line_number as i32).abs();
                return a_distance.cmp(&b_distance);
            }

            std::cmp::Ordering::Equal
        });

        debug!(
            "✅ Found {} definitions for '{}'",
            exact_matches.len(),
            self.symbol
        );
        Ok(exact_matches)
    }

    // Helper functions for cross-language naming convention conversion
    #[allow(dead_code)]
    fn to_snake_case(&self, s: &str) -> String {
        let mut result = String::new();
        let mut chars = s.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch.is_uppercase() {
                if !result.is_empty() && chars.peek().is_some_and(|c| c.is_lowercase()) {
                    result.push('_');
                }
                result.push(ch.to_lowercase().next().unwrap());
            } else {
                result.push(ch);
            }
        }
        result
    }

    #[allow(dead_code)]
    fn to_camel_case(&self, s: &str) -> String {
        let mut result = String::new();
        let mut capitalize_next = false;

        for ch in s.chars() {
            if ch == '_' {
                capitalize_next = true;
            } else if capitalize_next {
                result.push(ch.to_uppercase().next().unwrap());
                capitalize_next = false;
            } else {
                result.push(ch);
            }
        }
        result
    }

    #[allow(dead_code)]
    fn to_pascal_case(&self, s: &str) -> String {
        let camel = self.to_camel_case(s);
        if camel.is_empty() {
            return camel;
        }

        let mut chars = camel.chars();
        match chars.next() {
            None => String::new(),
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        }
    }

    fn definition_priority(&self, kind: &SymbolKind) -> u8 {
        match kind {
            SymbolKind::Class | SymbolKind::Interface => 1,
            SymbolKind::Function => 2,
            SymbolKind::Method | SymbolKind::Constructor => 3,
            SymbolKind::Type | SymbolKind::Enum => 4,
            SymbolKind::Variable | SymbolKind::Constant => 5,
            _ => 10,
        }
    }

    /// Format optimized results with token optimization for FastGotoTool
    pub fn format_optimized_results(&self, symbols: &[Symbol]) -> String {
        use crate::utils::progressive_reduction::ProgressiveReducer;
        use crate::utils::token_estimation::TokenEstimator;

        let mut lines = vec![format!("🎯 Go to Definition: {}", self.symbol)];

        // Add context information if provided
        if let Some(context_file) = &self.context_file {
            if let Some(line_number) = self.line_number {
                lines.push(format!("📍 Context: {}:{}", context_file, line_number));
            } else {
                lines.push(format!("📍 Context: {}", context_file));
            }
        }

        let count_line_index = lines.len(); // Remember where the count line will be
        lines.push(format!(
            "📊 Showing {} of {} definitions",
            symbols.len(),
            symbols.len()
        ));
        lines.push(String::new());

        // Token optimization: apply progressive reduction first, then early termination if needed
        let token_estimator = TokenEstimator::new();
        let token_limit: usize = 15000; // 15K token limit to stay within Claude's context window
        let progressive_reducer = ProgressiveReducer::new();

        // Calculate initial header tokens
        let header_text = lines.join("\n");
        let header_tokens = token_estimator.estimate_string(&header_text);
        let available_tokens = token_limit.saturating_sub(header_tokens);

        // Create formatted symbol items
        let mut all_items = Vec::new();
        for symbol in symbols {
            let mut item_lines = vec![
                format!(
                    "📍 {} [{}]",
                    symbol.name,
                    format!("{:?}", symbol.kind).to_lowercase()
                ),
                format!("   📄 File: {}", symbol.file_path),
                format!(
                    "   📍 Location: {}:{}",
                    symbol.start_line, symbol.start_column
                ),
            ];

            if let Some(signature) = &symbol.signature {
                item_lines.push(format!("   🔧 Signature: {}", signature));
            }

            if let Some(doc_comment) = &symbol.doc_comment {
                item_lines.push(format!("   📝 Documentation: {}", doc_comment));
            }

            if let Some(visibility) = &symbol.visibility {
                item_lines.push(format!("   👁️  Visibility: {:?}", visibility));
            }

            if let Some(semantic_group) = &symbol.semantic_group {
                item_lines.push(format!("   🏷️  Group: {}", semantic_group));
            }

            if let Some(confidence) = symbol.confidence {
                item_lines.push(format!("   🎯 Confidence: {:.2}", confidence));
            }

            // Include code_context if available (this is what triggers token optimization)
            if let Some(context) = &symbol.code_context {
                use crate::utils::context_truncation::ContextTruncator;
                item_lines.push("   📄 Context:".to_string());
                let context_lines: Vec<String> = context.lines().map(|s| s.to_string()).collect();
                let truncator = ContextTruncator::new();
                let max_lines = 10; // Max 10 lines per symbol for token control
                let final_lines = if context_lines.len() > max_lines {
                    truncator.truncate_lines(&context_lines, max_lines)
                } else {
                    context_lines
                };
                for context_line in &final_lines {
                    item_lines.push(format!("   {}", context_line));
                }
            }

            item_lines.push(String::new());
            all_items.push(item_lines.join("\n"));
        }

        // Define token estimator function for items
        let estimate_items_tokens = |items: &[&String]| -> usize {
            let mut total_tokens = 0;
            for item in items {
                total_tokens += token_estimator.estimate_string(item);
            }
            total_tokens
        };

        // Try progressive reduction first
        let item_refs: Vec<&String> = all_items.iter().collect();
        let reduced_item_refs =
            progressive_reducer.reduce(&item_refs, available_tokens, estimate_items_tokens);

        let (items_to_show, reduction_applied) = if reduced_item_refs.len() < all_items.len() {
            // Progressive reduction was applied - update the count line using the correct index
            lines[count_line_index] = format!(
                "📊 Showing {} of {} definitions - Applied progressive reduction",
                reduced_item_refs.len(),
                symbols.len()
            );
            let items: Vec<String> = reduced_item_refs.into_iter().cloned().collect();
            (items, true)
        } else {
            // No reduction needed
            (all_items, false)
        };

        // Add the items we decided to show
        for item in &items_to_show {
            lines.push(item.clone());
        }

        // Add next actions if we have results
        if !items_to_show.is_empty() {
            lines.push("🎯 Suggested next actions:".to_string());
            lines.push("   • Jump to definition and start editing".to_string());
            lines.push("   • Use fast_refs to see all usages".to_string());
            lines.push("   • Search for related symbols".to_string());
        } else {
            lines.push("❌ No definitions found".to_string());
            lines.push("🎯 Try searching with fast_search for broader results".to_string());
        }

        // Add reduction warning if truncated significantly
        if reduction_applied {
            lines.push(String::new());
            lines.push("⚠️  Response truncated to stay within token limits".to_string());
            lines.push("💡 Use more specific search terms for focused results".to_string());
        }

        lines.join("\n")
    }

    /// Find definitions using database search with workspace filtering
    async fn database_find_definitions(
        &self,
        handler: &JulieServerHandler,
        workspace_ids: Vec<String>,
    ) -> Result<Vec<Symbol>> {
        let workspace = handler
            .get_workspace()
            .await?
            .ok_or_else(|| anyhow::anyhow!("No workspace initialized"))?;

        let db = workspace
            .db
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No database available"))?;

        let db_lock = db.lock().await;

        // Find exact matches by name with workspace filtering
        let mut exact_matches =
            db_lock.find_symbols_by_pattern(&self.symbol, Some(workspace_ids.clone()))?;

        // Filter for exact name matches (find_symbols_by_pattern uses LIKE)
        exact_matches.retain(|symbol| symbol.name == self.symbol);

        // Strategy 2: Cross-language Intelligence Layer - naming convention variants
        // This enables workspace-filtered searches to find getUserData -> get_user_data -> GetUserData
        if exact_matches.is_empty() {
            debug!(
                "🌍 Attempting cross-language resolution for '{}' in workspace-filtered search",
                self.symbol
            );

            // Generate all naming convention variants using shared intelligence module
            let variants = generate_naming_variants(&self.symbol);

            for variant in variants {
                if variant != self.symbol {
                    // Try each variant with workspace filtering
                    if let Ok(mut variant_matches) =
                        db_lock.find_symbols_by_pattern(&variant, Some(workspace_ids.clone()))
                    {
                        // Filter for exact variant matches
                        variant_matches.retain(|symbol| symbol.name == variant);

                        if !variant_matches.is_empty() {
                            debug!("🎯 Found cross-language match: {} -> {} ({} results)",
                                self.symbol, variant, variant_matches.len());
                            exact_matches.extend(variant_matches);
                        }
                    }
                }
            }
        }

        // Remove duplicates based on symbol id
        exact_matches.sort_by(|a, b| a.id.cmp(&b.id));
        exact_matches.dedup_by(|a, b| a.id == b.id);

        // Prioritize results
        exact_matches.sort_by(|a, b| {
            // First by definition priority (classes > functions > variables)
            let priority_cmp = self
                .definition_priority(&a.kind)
                .cmp(&self.definition_priority(&b.kind));
            if priority_cmp != std::cmp::Ordering::Equal {
                return priority_cmp;
            }

            // Then by context file preference if provided
            if let Some(context_file) = &self.context_file {
                let a_in_context = a.file_path.contains(context_file);
                let b_in_context = b.file_path.contains(context_file);
                match (a_in_context, b_in_context) {
                    (true, false) => return std::cmp::Ordering::Less,
                    (false, true) => return std::cmp::Ordering::Greater,
                    _ => {}
                }
            }

            // Finally by file path alphabetically
            a.file_path.cmp(&b.file_path)
        });

        debug!(
            "🗄️ Database find definitions returned {} results",
            exact_matches.len()
        );
        Ok(exact_matches)
    }

    /// Resolve workspace filtering parameter to a list of workspace IDs
    async fn resolve_workspace_filter(
        &self,
        handler: &JulieServerHandler,
    ) -> Result<Option<Vec<String>>> {
        let workspace_param = self.workspace.as_deref().unwrap_or("primary");

        match workspace_param {
            "all" => {
                // Search across all workspaces - return None to indicate no filtering
                Ok(None)
            }
            "primary" => {
                // Search only primary workspace
                Ok(Some(vec!["primary".to_string()]))
            }
            workspace_id => {
                // Validate the workspace ID exists
                if let Some(primary_workspace) = handler.get_workspace().await? {
                    let registry_service =
                        WorkspaceRegistryService::new(primary_workspace.root.clone());

                    // Check if it's a valid workspace ID
                    match registry_service.get_workspace(workspace_id).await? {
                        Some(_) => Ok(Some(vec![workspace_id.to_string()])),
                        None => {
                            // Invalid workspace ID
                            Err(anyhow::anyhow!(
                                "Workspace '{}' not found. Use 'all', 'primary', or a valid workspace ID",
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

#[mcp_tool(
    name = "fast_refs",
    description = "FIND ALL IMPACT - See all references before you change code (prevents surprises)",
    title = "Fast Find All References",
    idempotent_hint = true,
    destructive_hint = false,
    open_world_hint = false,
    read_only_hint = true,
    meta = r#"{"category": "navigation", "scope": "workspace"}"#
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct FastRefsTool {
    /// Symbol name to find all references/usages for.
    /// Examples: "UserService", "handleRequest", "myFunction", "CONSTANT_NAME"
    /// Same format as fast_goto - Julie will find every place this symbol is used
    pub symbol: String,
    /// Include the symbol definition in results (default: true).
    /// Set false to see only usages, true to see definition + all usages
    /// Useful for refactoring - see complete impact before changes
    #[serde(default = "default_true")]
    pub include_definition: bool,
    /// Maximum references to return (default: 50, range: 1-500).
    /// Large symbols may have hundreds of references - use limit to control response size
    /// Tip: Start with default, increase if you need comprehensive coverage
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// Workspace filter (optional): "all" (search all workspaces), "primary" (primary only), or workspace ID
    /// Examples: "all", "primary", "project-b_a3f2b8c1"
    /// Default: "primary" - search only the primary workspace for focused results
    #[serde(default = "default_workspace_refs")]
    pub workspace: Option<String>,
}

fn default_true() -> bool {
    true
}
fn default_limit() -> u32 {
    50
}
fn default_workspace_refs() -> Option<String> {
    Some("primary".to_string())
}

impl FastRefsTool {
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
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                message,
            )]));
        }

        // Use token-optimized formatting
        let message = self.format_optimized_results(&definitions, &references);
        Ok(CallToolResult::text_content(vec![TextContent::from(
            message,
        )]))
    }

    async fn find_references_and_definitions(
        &self,
        handler: &JulieServerHandler,
    ) -> Result<(Vec<Symbol>, Vec<Relationship>)> {
        debug!(
            "🔍 Searching for references to '{}' using indexed search",
            self.symbol
        );

        // No longer load ALL relationships here - we'll use targeted queries below
        // This fixes the N+1 query pattern identified in STATUS.md #4

        // Strategy 1: Use SearchEngine for O(log n) performance instead of O(n) linear scan
        let mut definitions = Vec::new();

        // Use indexed search for exact matches - MUCH faster than linear scan!
        match handler.active_search_engine().await {
            Ok(search_engine) => {
                let search_engine = search_engine.read().await;
                match search_engine.search(&self.symbol).await {
            Ok(search_results) => {
                // Use SearchResult's symbol directly - no O(n) linear lookup needed!
                for search_result in search_results {
                    // Only include exact name matches for definitions
                    if search_result.symbol.name == self.symbol {
                        definitions.push(search_result.symbol);
                    }
                }
                debug!(
                    "⚡ Indexed search found {} exact matches",
                    definitions.len()
                );
            }
                Err(e) => {
                    debug!("Search engine failed, falling back to SQLite database: {}", e);
                    // Fallback to database search for exact name lookup (indexed, fast)
                    if let Ok(workspace) = handler.get_workspace().await {
                        if let Some(workspace) = workspace {
                            if let Some(db) = workspace.db.as_ref() {
                                let db_lock = db.lock().await;
                                definitions = db_lock.get_symbols_by_name(&self.symbol).unwrap_or_default();
                            }
                        }
                    }
                }
            }
            }
            Err(e) => {
                debug!("Search engine unavailable, using SQLite database: {}", e);
                // Fallback to database search for exact name lookup (indexed, fast)
                if let Ok(workspace) = handler.get_workspace().await {
                    if let Some(workspace) = workspace {
                        if let Some(db) = workspace.db.as_ref() {
                            let db_lock = db.lock().await;
                            definitions = db_lock.get_symbols_by_name(&self.symbol).unwrap_or_default();
                        }
                    }
                }
            }
        }

        // Cross-language naming convention matching using additional searches
        // TODO: Re-implement with proper search engine access
        /*
        let variants = vec![
            self.to_snake_case(&self.symbol),
            self.to_camel_case(&self.symbol),
            self.to_pascal_case(&self.symbol),
        ];

        for variant in variants {
            if variant != self.symbol {
                // Avoid duplicate searches
                match search_engine.search(&variant).await {
                    Ok(search_results) => {
                        for search_result in search_results {
                            if search_result.symbol.name == variant {
                                definitions.push(search_result.symbol);
                            }
                        }
                    }
                    Err(_) => {
                        // Skip failed variant searches - not critical
                        debug!("Variant search failed for: {}", variant);
                    }
                }
            }
        }
        */

        // Remove duplicates
        definitions.sort_by(|a, b| a.id.cmp(&b.id));
        definitions.dedup_by(|a, b| a.id == b.id);

        // Strategy 2: Find direct relationships - REFERENCES TO this symbol (not FROM it)
        // PERFORMANCE FIX: Use targeted queries instead of loading ALL relationships
        // This changes from O(n) linear scan to O(k * log n) indexed queries where k = definitions.len()
        let mut references: Vec<Relationship> = Vec::new();

        if let Ok(workspace) = handler.get_workspace().await {
            if let Some(workspace) = workspace {
                if let Some(db) = workspace.db.as_ref() {
                    let db_lock = db.lock().await;

                    // For each definition, query relationships TO that symbol using indexed query
                    for definition in &definitions {
                        if let Ok(symbol_references) = db_lock.get_relationships_to_symbol(&definition.id) {
                            // INFLATION FIX: get_relationships_to_symbol already filters for to_symbol_id
                            references.extend(symbol_references);
                        }
                    }
                }
            }
        }

        // Strategy 3: Semantic similarity matching DISABLED to prevent false positives
        // TODO: Re-enable with better similarity thresholds and validation
        debug!("⚠️  Semantic similarity analysis disabled to prevent reference inflation");

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

        debug!(
            "✅ Found {} definitions and {} references for '{}'",
            definitions.len(),
            references.len(),
            self.symbol
        );

        Ok((definitions, references))
    }

    // Helper functions for cross-language naming convention conversion
    // (reuse implementation from GotoDefinitionTool)
    #[allow(dead_code)]
    fn to_snake_case(&self, s: &str) -> String {
        let mut result = String::new();
        let mut chars = s.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch.is_uppercase() {
                if !result.is_empty() && chars.peek().is_some_and(|c| c.is_lowercase()) {
                    result.push('_');
                }
                result.push(ch.to_lowercase().next().unwrap());
            } else {
                result.push(ch);
            }
        }
        result
    }

    #[allow(dead_code)]
    fn to_camel_case(&self, s: &str) -> String {
        let mut result = String::new();
        let mut capitalize_next = false;

        for ch in s.chars() {
            if ch == '_' {
                capitalize_next = true;
            } else if capitalize_next {
                result.push(ch.to_uppercase().next().unwrap());
                capitalize_next = false;
            } else {
                result.push(ch);
            }
        }
        result
    }

    #[allow(dead_code)]
    fn to_pascal_case(&self, s: &str) -> String {
        let camel = self.to_camel_case(s);
        if camel.is_empty() {
            return camel;
        }

        let mut chars = camel.chars();
        match chars.next() {
            None => String::new(),
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        }
    }

    /// Format optimized results with token optimization for FastRefsTool
    pub fn format_optimized_results(
        &self,
        symbols: &[Symbol],
        relationships: &[Relationship],
    ) -> String {
        let mut lines = vec![format!("🔗 References for: '{}'", self.symbol)];

        // Token optimization: apply progressive reduction first, then early termination if needed
        let token_estimator = TokenEstimator::new();
        let token_limit: usize = 15000; // 15K token limit to stay within Claude's context window
        let progressive_reducer = ProgressiveReducer::new();

        // Calculate initial header tokens
        let header_text = lines.join("\n");
        let header_tokens = token_estimator.estimate_string(&header_text);
        let available_tokens = token_limit.saturating_sub(header_tokens);

        // Combine all items (symbols + relationships) for unified processing
        let mut all_items = Vec::new();

        // Add definitions if included
        if self.include_definition && !symbols.is_empty() {
            for symbol in symbols {
                all_items.push(format!(
                    "📍 Definition: {} [{}] - {}:{}:{}",
                    symbol.name,
                    format!("{:?}", symbol.kind).to_lowercase(),
                    symbol.file_path,
                    symbol.start_line,
                    symbol.start_column
                ));
            }
        }

        // Add references
        for relationship in relationships {
            all_items.push(format!(
                "🔗 Reference: {} - {}:{} (confidence: {:.2})",
                self.symbol,
                relationship.file_path,
                relationship.line_number,
                relationship.confidence
            ));
        }

        // Define token estimator function for items
        let estimate_items_tokens = |items: &[&String]| -> usize {
            let mut total_tokens = 0;
            for item in items {
                total_tokens += token_estimator.estimate_string(item);
            }
            total_tokens
        };

        // Try progressive reduction first
        let item_refs: Vec<&String> = all_items.iter().collect();
        let reduced_item_refs =
            progressive_reducer.reduce(&item_refs, available_tokens, estimate_items_tokens);

        let (items_to_show, reduction_message) = if reduced_item_refs.len() < all_items.len() {
            // Progressive reduction was applied
            let items: Vec<String> = reduced_item_refs.into_iter().cloned().collect();
            let total_items = symbols.len() + relationships.len();
            let message = format!(
                "📊 Showing {} of {} results - Applied progressive reduction {} → {}",
                items.len(),
                total_items,
                all_items.len(),
                items.len()
            );
            (items, message)
        } else {
            // No reduction needed
            let total_items = symbols.len() + relationships.len();
            let message = format!("📊 Showing {} of {} results", all_items.len(), total_items);
            (all_items, message)
        };

        lines.push(reduction_message);
        lines.push(String::new());

        // Add the items we decided to show
        for item in &items_to_show {
            lines.push(item.clone());
        }

        // Add next actions if we have results
        if !items_to_show.is_empty() {
            lines.push(String::new());
            lines.push("🎯 Suggested next actions:".to_string());
            lines.push("   • Use fast_goto to see full definitions".to_string());
            lines.push("   • Edit files to refactor symbol usage".to_string());
            lines.push("   • Search for related symbols".to_string());
        }

        lines.join("\n")
    }
}
