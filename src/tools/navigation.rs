use rust_mcp_sdk::schema::{CallToolResult, TextContent};
use rust_mcp_sdk::{macros::mcp_tool};
use rust_mcp_sdk::macros::JsonSchema;
use serde::{Deserialize, Serialize};
use anyhow::Result;
use tracing::debug;

use crate::handler::JulieServerHandler;
use crate::extractors::{Symbol, SymbolKind, Relationship};
use crate::utils::{token_estimation::TokenEstimator, progressive_reduction::ProgressiveReducer};

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
}

impl FastGotoTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!("🎯 Finding definition for: {}", self.symbol);

        // Check if workspace is indexed
        let is_indexed = *handler.is_indexed.read().await;
        if !is_indexed {
            let message = "❌ Workspace not indexed yet!\n💡 Run index_workspace first to enable navigation.";
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        // Find symbol definitions
        let definitions = self.find_definitions(handler).await?;

        if definitions.is_empty() {
            let message = format!(
                "🔍 No definition found for: '{}'\n\
                💡 Check the symbol name and ensure it exists in the indexed files",
                self.symbol
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
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

        Ok(CallToolResult::text_content(vec![TextContent::from(message)]))
    }

    async fn find_definitions(&self, handler: &JulieServerHandler) -> Result<Vec<Symbol>> {
        debug!("🔍 Finding definitions for: {}", self.symbol);

        // Strategy 1: Use SearchEngine for O(log n) performance instead of O(n) linear scan
        let search_engine = handler.search_engine.read().await;
        let mut exact_matches = Vec::new();

        // Use indexed search for exact matches - MUCH faster than linear scan!
        match search_engine.search(&self.symbol).await {
            Ok(search_results) => {
                // Use SearchResult's symbol directly - no O(n) linear lookup needed!
                for search_result in search_results {
                    // Only include exact name matches for definitions
                    if search_result.symbol.name == self.symbol {
                        exact_matches.push(search_result.symbol);
                    }
                }
                debug!("⚡ Indexed search found {} exact matches", exact_matches.len());
            }
            Err(e) => {
                debug!("Search engine failed, falling back to linear search: {}", e);
                // Fallback to linear search only if indexed search fails
                let symbols = handler.symbols.read().await;
                exact_matches = symbols.iter()
                    .filter(|symbol| symbol.name == self.symbol)
                    .cloned()
                    .collect();
            }
        }

        let relationships = handler.relationships.read().await;

        // Strategy 2: Use relationships to find actual definitions
        // Look for symbols that are referenced/imported with this name
        let symbols = handler.symbols.read().await; // Get symbols for relationship lookup
        for relationship in relationships.iter() {
            if let Some(target_symbol) = symbols.iter().find(|s| s.id == relationship.to_symbol_id) {
                // Check if this relationship represents a definition or import
                match &relationship.kind {
                    crate::extractors::base::RelationshipKind::Imports => {
                        if target_symbol.name == self.symbol {
                            exact_matches.push(target_symbol.clone());
                        }
                    }
                    crate::extractors::base::RelationshipKind::Defines => {
                        if target_symbol.name == self.symbol {
                            exact_matches.push(target_symbol.clone());
                        }
                    }
                    crate::extractors::base::RelationshipKind::Extends => {
                        if target_symbol.name == self.symbol {
                            exact_matches.push(target_symbol.clone());
                        }
                    }
                    _ => {}
                }
            }
        }

        // Remove duplicates based on symbol id
        exact_matches.sort_by(|a, b| a.id.cmp(&b.id));
        exact_matches.dedup_by(|a, b| a.id == b.id);

        // Strategy 3: Cross-language resolution using additional indexed searches
        if exact_matches.is_empty() {
            debug!("🌍 Attempting cross-language resolution for '{}'", self.symbol);

            // Use indexed search for naming convention variants instead of O(n) linear scan
            let variants = vec![
                self.to_snake_case(&self.symbol),
                self.to_camel_case(&self.symbol),
                self.to_pascal_case(&self.symbol)
            ];

            for variant in variants {
                if variant != self.symbol {  // Avoid duplicate searches
                    match search_engine.search(&variant).await {
                        Ok(search_results) => {
                            for search_result in search_results {
                                if search_result.symbol.name == variant {
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

        // Strategy 4: Semantic matching if still no results
        // TODO: DISABLED - This AI embedding computation on all 2458 symbols was causing UI hangs
        // The expensive O(n) AI processing needs to be optimized or made optional
        if false && exact_matches.is_empty() { // Disabled for performance
            debug!("🧠 Semantic matching temporarily disabled for performance");
            if let Ok(()) = handler.ensure_embedding_engine().await {
                let mut embedding_guard = handler.embedding_engine.write().await;
                if let Some(embedding_engine) = embedding_guard.as_mut() {
                    if let Ok(query_embedding) = embedding_engine.embed_text(&self.symbol) {
                        let symbols = handler.symbols.read().await;
                        for symbol in symbols.iter() {
                            let symbol_text = format!("{} {:?}", symbol.name, symbol.kind);
                            if let Ok(symbol_embedding) = embedding_engine.embed_text(&symbol_text) {
                                let similarity = crate::embeddings::cosine_similarity(&query_embedding, &symbol_embedding);
                                if similarity > 0.7 { // High similarity threshold for definitions
                                    exact_matches.push(symbol.clone());
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
            let priority_cmp = self.definition_priority(&a.kind).cmp(&self.definition_priority(&b.kind));
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

        debug!("✅ Found {} definitions for '{}'", exact_matches.len(), self.symbol);
        Ok(exact_matches)
    }

    // Helper functions for cross-language naming convention conversion
    fn to_snake_case(&self, s: &str) -> String {
        let mut result = String::new();
        let mut chars = s.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch.is_uppercase() {
                if !result.is_empty() && chars.peek().map_or(false, |c| c.is_lowercase()) {
                    result.push('_');
                }
                result.push(ch.to_lowercase().next().unwrap());
            } else {
                result.push(ch);
            }
        }
        result
    }

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
}

fn default_true() -> bool { true }
fn default_limit() -> u32 { 50 }

impl FastRefsTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!("🔗 Finding references for: {}", self.symbol);

        // Check if workspace is indexed
        let is_indexed = *handler.is_indexed.read().await;
        if !is_indexed {
            let message = "❌ Workspace not indexed yet!\n💡 Run index_workspace first to enable navigation.";
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        // Find references
        let (definitions, references) = self.find_references_and_definitions(handler).await?;

        if definitions.is_empty() && references.is_empty() {
            let message = format!(
                "🔍 No references found for: '{}'\n\
                💡 Check the symbol name and ensure it exists in the indexed files",
                self.symbol
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        // Use token-optimized formatting
        let message = self.format_optimized_results(&definitions, &references);
        Ok(CallToolResult::text_content(vec![TextContent::from(message)]))
    }

    async fn find_references_and_definitions(&self, handler: &JulieServerHandler) -> Result<(Vec<Symbol>, Vec<Relationship>)> {
        debug!("🔍 Searching for references to '{}' using indexed search", self.symbol);

        // Get required data from handler
        let relationships = handler.relationships.read().await;

        // Strategy 1: Use SearchEngine for O(log n) performance instead of O(n) linear scan
        let search_engine = handler.search_engine.read().await;
        let mut definitions = Vec::new();

        // Use indexed search for exact matches - MUCH faster than linear scan!
        match search_engine.search(&self.symbol).await {
            Ok(search_results) => {
                // Use SearchResult's symbol directly - no O(n) linear lookup needed!
                for search_result in search_results {
                    // Only include exact name matches for definitions
                    if search_result.symbol.name == self.symbol {
                        definitions.push(search_result.symbol);
                    }
                }
                debug!("⚡ Indexed search found {} exact matches", definitions.len());
            }
            Err(e) => {
                debug!("Search engine failed, falling back to linear search: {}", e);
                // Fallback to linear search only if indexed search fails
                let symbols = handler.symbols.read().await;
                for symbol in symbols.iter() {
                    if symbol.name == self.symbol {
                        definitions.push(symbol.clone());
                    }
                }
            }
        }

        // Cross-language naming convention matching using additional searches
        let variants = vec![
            self.to_snake_case(&self.symbol),
            self.to_camel_case(&self.symbol),
            self.to_pascal_case(&self.symbol)
        ];

        for variant in variants {
            if variant != self.symbol {  // Avoid duplicate searches
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

        // Remove duplicates
        definitions.sort_by(|a, b| a.id.cmp(&b.id));
        definitions.dedup_by(|a, b| a.id == b.id);

        // Strategy 2: Find direct relationships - REFERENCES TO this symbol (not FROM it)
        let symbol_ids: Vec<String> = definitions.iter().map(|s| s.id.clone()).collect();
        let mut references: Vec<Relationship> = relationships.iter()
            .filter(|rel| {
                // INFLATION FIX: Only count relationships where target is REFERENCED (to_symbol_id)
                // NOT where target does the referencing (from_symbol_id)
                symbol_ids.iter().any(|id| rel.to_symbol_id == *id)
            })
            .cloned()
            .collect();

        // Strategy 3: Semantic similarity matching DISABLED to prevent false positives
        // TODO: Re-enable with better similarity thresholds and validation
        debug!("⚠️  Semantic similarity analysis disabled to prevent reference inflation");

        // Sort references by confidence and location
        references.sort_by(|a, b| {
            // First by confidence (descending)
            let conf_cmp = b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal);
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

        debug!("✅ Found {} definitions and {} references for '{}'", definitions.len(), references.len(), self.symbol);

        Ok((definitions, references))
    }

    // Helper functions for cross-language naming convention conversion
    // (reuse implementation from GotoDefinitionTool)
    fn to_snake_case(&self, s: &str) -> String {
        let mut result = String::new();
        let mut chars = s.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch.is_uppercase() {
                if !result.is_empty() && chars.peek().map_or(false, |c| c.is_lowercase()) {
                    result.push('_');
                }
                result.push(ch.to_lowercase().next().unwrap());
            } else {
                result.push(ch);
            }
        }
        result
    }

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
    pub fn format_optimized_results(&self, symbols: &[Symbol], relationships: &[Relationship]) -> String {
        let mut lines = vec![
            format!("🔗 References for: '{}'", self.symbol),
        ];

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
                all_items.push(format!("📍 Definition: {} [{}] - {}:{}:{}",
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
            all_items.push(format!("🔗 Reference: {} - {}:{} (confidence: {:.2})",
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
        let reduced_item_refs = progressive_reducer.reduce(&item_refs, available_tokens, estimate_items_tokens);

        let (items_to_show, reduction_message) = if reduced_item_refs.len() < all_items.len() {
            // Progressive reduction was applied
            let items: Vec<String> = reduced_item_refs.into_iter().cloned().collect();
            let total_items = symbols.len() + relationships.len();
            let message = format!("📊 Showing {} of {} results - Applied progressive reduction {} → {}",
                    items.len(), total_items, all_items.len(), items.len());
            (items, message)
        } else {
            // No reduction needed
            let total_items = symbols.len() + relationships.len();
            let message = format!("📊 Showing {} of {} results",
                    all_items.len(), total_items);
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