use rust_mcp_sdk::schema::{schema_utils::CallToolError, CallToolResult, TextContent};
use rust_mcp_sdk::{macros::mcp_tool, tool_box};
use rust_mcp_sdk::macros::JsonSchema;
use serde::{Deserialize, Serialize};
use anyhow::Result;
use tracing::{info, debug, warn, error};
use std::path::{Path, PathBuf};
use std::fs;
use glob::glob;

use crate::handler::JulieServerHandler;
use crate::extractors::{Symbol, SymbolKind, Relationship};
use crate::extractors::typescript::TypeScriptExtractor;

//******************//
// Index Workspace  //
//******************//
#[mcp_tool(
    name = "index_workspace",
    description = "Index the current workspace for fast code intelligence. Must be run first to enable semantic search.",
    title = "Index Workspace for Code Intelligence",
    idempotent_hint = true,
    destructive_hint = false,
    open_world_hint = false,
    read_only_hint = true,
    meta = r#"{"priority": "high", "category": "initialization"}"#
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct IndexWorkspaceTool {
    /// Optional workspace path (defaults to current directory)
    #[serde(default)]
    pub workspace_path: Option<String>,
    /// Force re-indexing even if index exists
    #[serde(default)]
    pub force_reindex: Option<bool>,
}

impl IndexWorkspaceTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        info!("📚 Starting workspace indexing...");

        let workspace_path = self.workspace_path.as_deref().unwrap_or(".");
        let force_reindex = self.force_reindex.unwrap_or(false);

        // Check if already indexed and not forcing reindex
        if !force_reindex {
            let is_indexed = *handler.is_indexed.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
            if is_indexed {
                let symbol_count = handler.symbols.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?.len();
                let message = format!(
                    "✅ Workspace already indexed!\n\
                    📊 Found {} symbols\n\
                    💡 Use force_reindex: true to re-index",
                    symbol_count
                );
                return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
            }
        }

        // Perform indexing
        match self.index_workspace_files(handler, workspace_path).await {
            Ok((symbol_count, file_count, relationship_count)) => {
                // Mark as indexed
                *handler.is_indexed.write().map_err(|e| anyhow::anyhow!("Lock error: {}", e))? = true;

                let message = format!(
                    "🎉 Workspace indexing complete!\n\
                    📁 Indexed {} files\n\
                    🔍 Extracted {} symbols\n\
                    🔗 Found {} relationships\n\
                    ⚡ Ready for search and navigation!",
                    file_count, symbol_count, relationship_count
                );
                Ok(CallToolResult::text_content(vec![TextContent::from(message)]))
            },
            Err(e) => {
                error!("Failed to index workspace: {}", e);
                let message = format!(
                    "❌ Workspace indexing failed: {}\n\
                    💡 Check that the path exists and contains source files",
                    e
                );
                Ok(CallToolResult::text_content(vec![TextContent::from(message)]))
            }
        }
    }

    async fn index_workspace_files(&self, handler: &JulieServerHandler, workspace_path: &str) -> Result<(usize, usize, usize)> {
        info!("🔍 Scanning workspace: {}", workspace_path);

        // Clear existing data if force reindex
        if self.force_reindex.unwrap_or(false) {
            handler.symbols.write().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?.clear();
            handler.relationships.write().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?.clear();
        }

        let mut total_symbols = 0;
        let mut total_files = 0;
        let mut total_relationships = 0;

        // Define supported file patterns (starting with TypeScript/JavaScript)
        let patterns = vec![
            "**/*.ts",
            "**/*.tsx",
            "**/*.js",
            "**/*.jsx",
            "**/*.mts",
            "**/*.cts",
        ];

        for pattern in patterns {
            let full_pattern = format!("{}/{}", workspace_path, pattern);
            debug!("Scanning pattern: {}", full_pattern);

            for entry in glob(&full_pattern).map_err(|e| anyhow::anyhow!("Glob error: {}", e))? {
                match entry {
                    Ok(path) => {
                        if let Err(e) = self.process_file(handler, &path).await {
                            warn!("Failed to process file {:?}: {}", path, e);
                            continue;
                        }
                        total_files += 1;
                    }
                    Err(e) => {
                        warn!("Error accessing file: {}", e);
                    }
                }
            }
        }

        // Get final counts
        total_symbols = handler.symbols.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?.len();
        total_relationships = handler.relationships.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?.len();

        info!("✅ Indexing complete: {} files, {} symbols, {} relationships",
              total_files, total_symbols, total_relationships);

        Ok((total_symbols, total_files, total_relationships))
    }

    async fn process_file(&self, handler: &JulieServerHandler, file_path: &Path) -> Result<()> {
        debug!("Processing file: {:?}", file_path);

        // Read file content
        let content = fs::read_to_string(file_path)
            .map_err(|e| anyhow::anyhow!("Failed to read file {:?}: {}", file_path, e))?;

        // Determine language and use appropriate extractor
        let language = self.detect_language(file_path);
        let file_path_str = file_path.to_string_lossy().to_string();

        match language.as_str() {
            "typescript" | "javascript" => {
                self.extract_typescript_symbols(handler, &file_path_str, &content, &language).await
            }
            _ => {
                debug!("Unsupported language: {} for file: {:?}", language, file_path);
                Ok(())
            }
        }
    }

    async fn extract_typescript_symbols(&self, handler: &JulieServerHandler, file_path: &str, content: &str, language: &str) -> Result<()> {
        // Create TypeScript extractor
        let mut extractor = TypeScriptExtractor::new(
            language.to_string(),
            file_path.to_string(),
            content.to_string()
        );

        // Parse the file
        let mut parser = tree_sitter::Parser::new();
        let tree_sitter_language = if language == "typescript" {
            tree_sitter_typescript::LANGUAGE_TSX.into()
        } else {
            tree_sitter_javascript::LANGUAGE.into()
        };

        parser.set_language(&tree_sitter_language)
            .map_err(|e| anyhow::anyhow!("Failed to set parser language: {}", e))?;

        let tree = parser.parse(content, None)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse file: {}", file_path))?;

        // Extract symbols
        let symbols = extractor.extract_symbols(&tree);
        let relationships = extractor.extract_relationships(&tree, &symbols);

        // Store results
        {
            let mut symbol_storage = handler.symbols.write().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
            symbol_storage.extend(symbols);
        }

        {
            let mut relationship_storage = handler.relationships.write().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
            relationship_storage.extend(relationships);
        }

        Ok(())
    }

    fn detect_language(&self, file_path: &Path) -> String {
        match file_path.extension().and_then(|s| s.to_str()) {
            Some("ts") | Some("tsx") | Some("mts") | Some("cts") => "typescript".to_string(),
            Some("js") | Some("jsx") => "javascript".to_string(),
            _ => "unknown".to_string(),
        }
    }
}

//******************//
//   Search Code    //
//******************//
#[mcp_tool(
    name = "search_code",
    description = "Search for code symbols, functions, classes across all supported languages with fuzzy matching.",
    title = "Code Search with Fuzzy Matching",
    idempotent_hint = true,
    destructive_hint = false,
    open_world_hint = false,
    read_only_hint = true,
    meta = r#"{"category": "search", "performance": "sub_10ms"}"#
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SearchCodeTool {
    /// Search query (symbol name, function name, etc.)
    pub query: String,
    /// Optional language filter
    #[serde(default)]
    pub language: Option<String>,
    /// Optional file path pattern filter
    #[serde(default)]
    pub file_pattern: Option<String>,
    /// Maximum number of results
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_limit() -> u32 { 50 }

impl SearchCodeTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!("🔍 Searching for: {}", self.query);

        // Check if workspace is indexed
        let is_indexed = *handler.is_indexed.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        if !is_indexed {
            let message = "❌ Workspace not indexed yet!\n💡 Run index_workspace first to enable search.";
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        // Perform search
        let results = self.search_symbols(handler)?;

        if results.is_empty() {
            let message = format!(
                "🔍 No results found for: '{}'\n\
                💡 Try a broader search term or check the spelling",
                self.query
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        // Format results
        let mut message = format!(
            "🔍 Found {} results for: '{}'\n\n",
            results.len().min(self.limit as usize),
            self.query
        );

        for (i, symbol) in results.iter().take(self.limit as usize).enumerate() {
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

        if results.len() > self.limit as usize {
            message.push_str(&format!("\n... and {} more results\n", results.len() - self.limit as usize));
        }

        Ok(CallToolResult::text_content(vec![TextContent::from(message)]))
    }

    fn search_symbols(&self, handler: &JulieServerHandler) -> Result<Vec<Symbol>> {
        let symbols = handler.symbols.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        let query_lower = self.query.to_lowercase();

        let mut results: Vec<Symbol> = symbols.iter()
            .filter(|symbol| {
                // Name matching (case insensitive)
                let name_match = symbol.name.to_lowercase().contains(&query_lower);

                // Language filter
                let language_match = self.language.as_ref()
                    .map(|lang| symbol.language.eq_ignore_ascii_case(lang))
                    .unwrap_or(true);

                // File pattern filter (basic implementation)
                let file_match = self.file_pattern.as_ref()
                    .map(|pattern| symbol.file_path.contains(pattern))
                    .unwrap_or(true);

                name_match && language_match && file_match
            })
            .cloned()
            .collect();

        // Sort by relevance (exact matches first, then by symbol kind)
        results.sort_by(|a, b| {
            let a_exact = a.name.eq_ignore_ascii_case(&self.query);
            let b_exact = b.name.eq_ignore_ascii_case(&self.query);

            match (a_exact, b_exact) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => {
                    // Sort by symbol kind priority
                    let a_priority = self.symbol_priority(&a.kind);
                    let b_priority = self.symbol_priority(&b.kind);
                    a_priority.cmp(&b_priority)
                }
            }
        });

        Ok(results)
    }

    fn symbol_priority(&self, kind: &SymbolKind) -> u8 {
        match kind {
            SymbolKind::Function => 1,
            SymbolKind::Class => 2,
            SymbolKind::Interface => 3,
            SymbolKind::Method => 4,
            SymbolKind::Variable => 5,
            SymbolKind::Type => 6,
            _ => 10,
        }
    }
}

//******************//
// Goto Definition  //
//******************//
#[mcp_tool(
    name = "goto_definition",
    description = "Navigate to the definition of a symbol with precise location information.",
    title = "Go to Definition",
    idempotent_hint = true,
    destructive_hint = false,
    open_world_hint = false,
    read_only_hint = true,
    meta = r#"{"category": "navigation", "precision": "line_level"}"#
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GotoDefinitionTool {
    /// Symbol name to find definition for
    pub symbol: String,
    /// Optional context file path for better resolution
    #[serde(default)]
    pub context_file: Option<String>,
    /// Optional line number for context
    #[serde(default)]
    pub line_number: Option<u32>,
}

impl GotoDefinitionTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!("🎯 Finding definition for: {}", self.symbol);

        // Check if workspace is indexed
        let is_indexed = *handler.is_indexed.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        if !is_indexed {
            let message = "❌ Workspace not indexed yet!\n💡 Run index_workspace first to enable navigation.";
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        // Find symbol definitions
        let definitions = self.find_definitions(handler)?;

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

    fn find_definitions(&self, handler: &JulieServerHandler) -> Result<Vec<Symbol>> {
        let symbols = handler.symbols.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        // Find exact name matches first, then partial matches
        let mut exact_matches: Vec<Symbol> = symbols.iter()
            .filter(|symbol| symbol.name == self.symbol)
            .cloned()
            .collect();

        // Sort exact matches by priority (prefer classes, functions over variables)
        exact_matches.sort_by_key(|s| self.definition_priority(&s.kind));

        // If we have context file, prioritize symbols from that file or nearby
        if let Some(context_file) = &self.context_file {
            exact_matches.sort_by(|a, b| {
                let a_in_context = a.file_path.contains(context_file);
                let b_in_context = b.file_path.contains(context_file);
                match (a_in_context, b_in_context) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    _ => std::cmp::Ordering::Equal,
                }
            });
        }

        Ok(exact_matches)
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

//******************//
// Find References  //
//******************//
#[mcp_tool(
    name = "find_references",
    description = "Find all references to a symbol across the codebase.",
    title = "Find All References",
    idempotent_hint = true,
    destructive_hint = false,
    open_world_hint = false,
    read_only_hint = true,
    meta = r#"{"category": "navigation", "scope": "workspace"}"#
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct FindReferencesTool {
    /// Symbol name to find references for
    pub symbol: String,
    /// Include definition in results
    #[serde(default = "default_true")]
    pub include_definition: bool,
    /// Maximum number of results
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_true() -> bool { true }

impl FindReferencesTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!("🔗 Finding references for: {}", self.symbol);

        // Check if workspace is indexed
        let is_indexed = *handler.is_indexed.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        if !is_indexed {
            let message = "❌ Workspace not indexed yet!\n💡 Run index_workspace first to enable navigation.";
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        // Find references
        let (definitions, references) = self.find_references_and_definitions(handler)?;

        if definitions.is_empty() && references.is_empty() {
            let message = format!(
                "🔍 No references found for: '{}'\n\
                💡 Check the symbol name and ensure it exists in the indexed files",
                self.symbol
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        // Format results
        let total_results = if self.include_definition { definitions.len() + references.len() } else { references.len() };
        let mut message = format!(
            "🔗 Found {} reference(s) for: '{}'\n\n",
            total_results,
            self.symbol
        );

        let mut count = 0;

        // Include definitions if requested
        if self.include_definition && !definitions.is_empty() {
            message.push_str("🎯 Definitions:\n");
            for symbol in &definitions {
                if count >= self.limit as usize { break; }
                message.push_str(&format!(
                    "  {} [{}] - {}:{}:{}\n",
                    symbol.name,
                    format!("{:?}", symbol.kind).to_lowercase(),
                    symbol.file_path,
                    symbol.start_line,
                    symbol.start_column
                ));
                count += 1;
            }
            message.push('\n');
        }

        // Include references
        if !references.is_empty() {
            message.push_str("🔗 References:\n");
            for relationship in references.iter().take((self.limit as usize).saturating_sub(count)) {
                message.push_str(&format!(
                    "  {} - {}:{} ({})",
                    format!("{:?}", relationship.kind),
                    relationship.file_path,
                    relationship.line_number,
                    relationship.kind
                ));

                // Add confidence if not 1.0
                if relationship.confidence < 1.0 {
                    message.push_str(&format!(" [confidence: {:.1}]", relationship.confidence));
                }
                message.push('\n');
                count += 1;
            }
        }

        if total_results > self.limit as usize {
            message.push_str(&format!("\n... and {} more references\n", total_results - self.limit as usize));
        }

        Ok(CallToolResult::text_content(vec![TextContent::from(message)]))
    }

    fn find_references_and_definitions(&self, handler: &JulieServerHandler) -> Result<(Vec<Symbol>, Vec<Relationship>)> {
        let symbols = handler.symbols.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        let relationships = handler.relationships.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        // Find symbol definitions
        let definitions: Vec<Symbol> = symbols.iter()
            .filter(|symbol| symbol.name == self.symbol)
            .cloned()
            .collect();

        // Get symbol IDs for reference search
        let symbol_ids: Vec<String> = definitions.iter().map(|s| s.id.clone()).collect();

        // Find relationships where this symbol is referenced
        let references: Vec<Relationship> = relationships.iter()
            .filter(|rel| {
                symbol_ids.iter().any(|id| rel.to_symbol_id == *id || rel.from_symbol_id == *id)
            })
            .cloned()
            .collect();

        Ok((definitions, references))
    }
}

//******************//
// Semantic Search  //
//******************//
#[mcp_tool(
    name = "semantic_search",
    description = "Search code by meaning and intent using AI embeddings for conceptual matches.",
    title = "Semantic Code Search",
    idempotent_hint = true,
    destructive_hint = false,
    open_world_hint = false,
    read_only_hint = true,
    meta = r#"{"category": "ai_search", "requires": "embeddings"}"#
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SemanticSearchTool {
    /// Natural language description of what you're looking for
    pub query: String,
    /// Search mode: hybrid (text + semantic), semantic_only, text_only
    #[serde(default = "default_hybrid")]
    pub mode: String,
    /// Maximum number of results
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_hybrid() -> String { "hybrid".to_string() }

impl SemanticSearchTool {
    pub async fn call_tool(&self, _handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!("🧠 Semantic search for: {}", self.query);

        // TODO: Implement semantic search with ONNX embeddings
        let message = format!(
            "🧠 Semantic Search for: '{}'\n\
            🔄 Mode: {}\n\
            📊 Limit: {}\n\n\
            🚧 Semantic search not yet implemented\n\
            🎯 Will use ONNX embeddings for meaning-based code search\n\
            💡 Use search_code for now for basic text-based search",
            self.query,
            self.mode,
            self.limit
        );

        Ok(CallToolResult::text_content(vec![TextContent::from(message)]))
    }
}

//******************//
//     Explore      //
//******************//
#[mcp_tool(
    name = "explore",
    description = "Explore codebase architecture, dependencies, and relationships.",
    title = "Explore Codebase Architecture",
    idempotent_hint = true,
    destructive_hint = false,
    open_world_hint = false,
    read_only_hint = true,
    meta = r#"{"category": "analysis", "scope": "architectural"}"#
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ExploreTool {
    /// Exploration type: overview, dependencies, trace, hotspots
    pub mode: String,
    /// Optional focus area (file, module, class)
    #[serde(default)]
    pub focus: Option<String>,
    /// Analysis depth: shallow, medium, deep
    #[serde(default = "default_medium")]
    pub depth: String,
}

fn default_medium() -> String { "medium".to_string() }

impl ExploreTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!("🧭 Exploring codebase: mode={}, focus={:?}", self.mode, self.focus);

        // Check if workspace is indexed
        let is_indexed = *handler.is_indexed.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        if !is_indexed {
            let message = "❌ Workspace not indexed yet!\n💡 Run index_workspace first to enable exploration.";
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        // Perform exploration based on mode
        let message = match self.mode.as_str() {
            "overview" => self.generate_overview(handler)?,
            "dependencies" => self.analyze_dependencies(handler)?,
            "hotspots" => self.find_hotspots(handler)?,
            "trace" => self.trace_relationships(handler)?,
            _ => format!(
                "❌ Unknown exploration mode: '{}'\n\
                💡 Supported modes: overview, dependencies, hotspots, trace",
                self.mode
            ),
        };

        Ok(CallToolResult::text_content(vec![TextContent::from(message)]))
    }

    fn generate_overview(&self, handler: &JulieServerHandler) -> Result<String> {
        let symbols = handler.symbols.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        let relationships = handler.relationships.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        // Count by symbol type
        let mut counts = std::collections::HashMap::new();
        let mut file_counts = std::collections::HashMap::new();
        let mut language_counts = std::collections::HashMap::new();

        for symbol in symbols.iter() {
            *counts.entry(&symbol.kind).or_insert(0) += 1;
            *file_counts.entry(&symbol.file_path).or_insert(0) += 1;
            *language_counts.entry(&symbol.language).or_insert(0) += 1;
        }

        let mut message = format!(
            "🧭 Codebase Overview\n\
            ========================\n\
            📊 Total Symbols: {}\n\
            📁 Total Files: {}\n\
            🔗 Total Relationships: {}\n\n",
            symbols.len(),
            file_counts.len(),
            relationships.len()
        );

        // Symbol breakdown
        message.push_str("🏷️ Symbol Types:\n");
        let mut sorted_counts: Vec<_> = counts.iter().collect();
        sorted_counts.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
        for (kind, count) in sorted_counts {
            message.push_str(&format!("  {:?}: {}\n", kind, count));
        }

        // Language breakdown
        message.push_str("\n💻 Languages:\n");
        let mut sorted_languages: Vec<_> = language_counts.iter().collect();
        sorted_languages.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
        for (lang, count) in sorted_languages {
            message.push_str(&format!("  {}: {} symbols\n", lang, count));
        }

        // Top files by symbol count
        if matches!(self.depth.as_str(), "medium" | "deep") {
            message.push_str("\n📁 Top Files by Symbol Count:\n");
            let mut sorted_files: Vec<_> = file_counts.iter().collect();
            sorted_files.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
            for (file, count) in sorted_files.iter().take(10) {
                let file_name = std::path::Path::new(file)
                    .file_name()
                    .unwrap_or_else(|| std::ffi::OsStr::new(file))
                    .to_string_lossy();
                message.push_str(&format!("  {}: {} symbols\n", file_name, count));
            }
        }

        Ok(message)
    }

    fn analyze_dependencies(&self, handler: &JulieServerHandler) -> Result<String> {
        let relationships = handler.relationships.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        let symbols = handler.symbols.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let mut relationship_counts = std::collections::HashMap::new();
        let mut symbol_references = std::collections::HashMap::new();

        for rel in relationships.iter() {
            *relationship_counts.entry(&rel.kind).or_insert(0) += 1;
            *symbol_references.entry(&rel.to_symbol_id).or_insert(0) += 1;
        }

        let mut message = format!(
            "🔗 Dependency Analysis\n\
            =====================\n\
            Total Relationships: {}\n\n",
            relationships.len()
        );

        // Relationship type breakdown
        message.push_str("🏷️ Relationship Types:\n");
        let mut sorted_rels: Vec<_> = relationship_counts.iter().collect();
        sorted_rels.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
        for (kind, count) in sorted_rels {
            message.push_str(&format!("  {}: {}\n", kind, count));
        }

        // Most referenced symbols
        if matches!(self.depth.as_str(), "medium" | "deep") {
            message.push_str("\n🔥 Most Referenced Symbols:\n");
            let mut sorted_refs: Vec<_> = symbol_references.iter().collect();
            sorted_refs.sort_by_key(|(_, count)| std::cmp::Reverse(**count));

            for (symbol_id, count) in sorted_refs.iter().take(10) {
                if let Some(symbol) = symbols.iter().find(|s| s.id == ***symbol_id) {
                    message.push_str(&format!("  {} [{}]: {} references\n", symbol.name, format!("{:?}", symbol.kind).to_lowercase(), count));
                }
            }
        }

        Ok(message)
    }

    fn find_hotspots(&self, handler: &JulieServerHandler) -> Result<String> {
        let symbols = handler.symbols.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        let relationships = handler.relationships.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        // Find files with most symbols (complexity hotspots)
        let mut file_symbol_counts = std::collections::HashMap::new();
        let mut file_relationship_counts = std::collections::HashMap::new();

        for symbol in symbols.iter() {
            *file_symbol_counts.entry(&symbol.file_path).or_insert(0) += 1;
        }

        for rel in relationships.iter() {
            *file_relationship_counts.entry(&rel.file_path).or_insert(0) += 1;
        }

        let mut message = "🔥 Complexity Hotspots\n=====================\n".to_string();

        message.push_str("📁 Files with Most Symbols:\n");
        let mut sorted_files: Vec<_> = file_symbol_counts.iter().collect();
        sorted_files.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
        for (file, count) in sorted_files.iter().take(10) {
            let file_name = std::path::Path::new(file)
                .file_name()
                .unwrap_or_else(|| std::ffi::OsStr::new(file))
                .to_string_lossy();
            message.push_str(&format!("  {}: {} symbols\n", file_name, count));
        }

        message.push_str("\n🔗 Files with Most Relationships:\n");
        let mut sorted_rel_files: Vec<_> = file_relationship_counts.iter().collect();
        sorted_rel_files.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
        for (file, count) in sorted_rel_files.iter().take(10) {
            let file_name = std::path::Path::new(file)
                .file_name()
                .unwrap_or_else(|| std::ffi::OsStr::new(file))
                .to_string_lossy();
            message.push_str(&format!("  {}: {} relationships\n", file_name, count));
        }

        Ok(message)
    }

    fn trace_relationships(&self, handler: &JulieServerHandler) -> Result<String> {
        let relationships = handler.relationships.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        let symbols = handler.symbols.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let mut message = "🔍 Relationship Tracing\n=====================\n".to_string();

        if let Some(focus) = &self.focus {
            // Find the focused symbol
            if let Some(target_symbol) = symbols.iter().find(|s| s.name == *focus) {
                message.push_str(&format!("Tracing relationships for: {}\n\n", focus));

                // Find incoming relationships (what references this symbol)
                let incoming: Vec<_> = relationships.iter()
                    .filter(|rel| rel.to_symbol_id == target_symbol.id)
                    .collect();

                // Find outgoing relationships (what this symbol references)
                let outgoing: Vec<_> = relationships.iter()
                    .filter(|rel| rel.from_symbol_id == target_symbol.id)
                    .collect();

                message.push_str(&format!("← Incoming ({} relationships):\n", incoming.len()));
                for rel in incoming.iter().take(10) {
                    if let Some(from_symbol) = symbols.iter().find(|s| s.id == rel.from_symbol_id) {
                        message.push_str(&format!("  {} {} this symbol\n", from_symbol.name, rel.kind));
                    }
                }

                message.push_str(&format!("\n→ Outgoing ({} relationships):\n", outgoing.len()));
                for rel in outgoing.iter().take(10) {
                    if let Some(to_symbol) = symbols.iter().find(|s| s.id == rel.to_symbol_id) {
                        message.push_str(&format!("  This symbol {} {}\n", rel.kind, to_symbol.name));
                    }
                }
            } else {
                message.push_str(&format!("❌ Symbol '{}' not found\n", focus));
            }
        } else {
            message.push_str("💡 Use focus parameter to trace a specific symbol\n");
            message.push_str("Example: { \"mode\": \"trace\", \"focus\": \"functionName\" }");
        }

        Ok(message)
    }
}

//******************//
//     Navigate     //
//******************//
#[mcp_tool(
    name = "navigate",
    description = "Navigate through code with surgical precision using various navigation modes.",
    title = "Precise Code Navigation",
    idempotent_hint = true,
    destructive_hint = false,
    open_world_hint = false,
    read_only_hint = true,
    meta = r#"{"category": "navigation", "precision": "surgical"}"#
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct NavigateTool {
    /// Navigation mode: definition, references, implementations, callers, callees
    pub mode: String,
    /// Symbol or identifier to navigate from
    pub target: String,
    /// Optional context for disambiguation
    #[serde(default)]
    pub context: Option<String>,
}

impl NavigateTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!("🚀 Navigating: mode={}, target={}", self.mode, self.target);

        // Check if workspace is indexed
        let is_indexed = *handler.is_indexed.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        if !is_indexed {
            let message = "❌ Workspace not indexed yet!\n💡 Run index_workspace first to enable navigation.";
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        // Perform navigation based on mode
        let message = match self.mode.as_str() {
            "definition" => self.navigate_to_definition(handler)?,
            "references" => self.navigate_to_references(handler)?,
            "implementations" => self.navigate_to_implementations(handler)?,
            "callers" => self.navigate_to_callers(handler)?,
            "callees" => self.navigate_to_callees(handler)?,
            _ => format!(
                "❌ Unknown navigation mode: '{}'\n\
                💡 Supported modes: definition, references, implementations, callers, callees",
                self.mode
            ),
        };

        Ok(CallToolResult::text_content(vec![TextContent::from(message)]))
    }

    fn navigate_to_definition(&self, handler: &JulieServerHandler) -> Result<String> {
        let symbols = handler.symbols.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        let definitions: Vec<_> = symbols.iter()
            .filter(|s| s.name == self.target)
            .collect();

        if definitions.is_empty() {
            return Ok(format!("❌ No definition found for: '{}'\n", self.target));
        }

        let mut message = format!("🎯 Definition of '{}':\n", self.target);
        for symbol in definitions {
            message.push_str(&format!(
                "📁 {}:{}:{} [{}]\n",
                symbol.file_path,
                symbol.start_line,
                symbol.start_column,
                format!("{:?}", symbol.kind).to_lowercase()
            ));
            if let Some(signature) = &symbol.signature {
                message.push_str(&format!("📝 {}", signature));
            }
            message.push('\n');
        }

        Ok(message)
    }

    fn navigate_to_references(&self, handler: &JulieServerHandler) -> Result<String> {
        let symbols = handler.symbols.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        let relationships = handler.relationships.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        // Find the target symbol
        let target_symbols: Vec<_> = symbols.iter()
            .filter(|s| s.name == self.target)
            .collect();

        if target_symbols.is_empty() {
            return Ok(format!("❌ Symbol '{}' not found\n", self.target));
        }

        let target_ids: Vec<_> = target_symbols.iter().map(|s| s.id.clone()).collect();

        // Find references in relationships
        let references: Vec<_> = relationships.iter()
            .filter(|rel| target_ids.iter().any(|id| rel.to_symbol_id == *id))
            .collect();

        let mut message = format!("🔗 References to '{}':\n", self.target);
        if references.is_empty() {
            message.push_str("ℹ️ No references found\n");
        } else {
            for rel in references.iter().take(20) {
                message.push_str(&format!(
                    "📁 {}:{} - {} relationship\n",
                    rel.file_path,
                    rel.line_number,
                    rel.kind
                ));
            }
            if references.len() > 20 {
                message.push_str(&format!("... and {} more references\n", references.len() - 20));
            }
        }

        Ok(message)
    }

    fn navigate_to_implementations(&self, handler: &JulieServerHandler) -> Result<String> {
        let relationships = handler.relationships.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        let symbols = handler.symbols.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        // Find symbols that implement the target (interfaces/abstract classes)
        let target_symbols: Vec<_> = symbols.iter()
            .filter(|s| s.name == self.target)
            .collect();

        if target_symbols.is_empty() {
            return Ok(format!("❌ Symbol '{}' not found\n", self.target));
        }

        let target_ids: Vec<_> = target_symbols.iter().map(|s| s.id.clone()).collect();

        let implementations: Vec<_> = relationships.iter()
            .filter(|rel| {
                matches!(rel.kind, crate::extractors::RelationshipKind::Implements) &&
                target_ids.iter().any(|id| rel.to_symbol_id == *id)
            })
            .collect();

        let mut message = format!("🛠️ Implementations of '{}':\n", self.target);
        if implementations.is_empty() {
            message.push_str("ℹ️ No implementations found\n");
        } else {
            for rel in implementations {
                if let Some(impl_symbol) = symbols.iter().find(|s| s.id == rel.from_symbol_id) {
                    message.push_str(&format!(
                        "📁 {} - {}:{}:{}\n",
                        impl_symbol.name,
                        impl_symbol.file_path,
                        impl_symbol.start_line,
                        impl_symbol.start_column
                    ));
                }
            }
        }

        Ok(message)
    }

    fn navigate_to_callers(&self, handler: &JulieServerHandler) -> Result<String> {
        let relationships = handler.relationships.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        let symbols = handler.symbols.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        // Find the target function
        let target_symbols: Vec<_> = symbols.iter()
            .filter(|s| s.name == self.target && matches!(s.kind, SymbolKind::Function | SymbolKind::Method))
            .collect();

        if target_symbols.is_empty() {
            return Ok(format!("❌ Function '{}' not found\n", self.target));
        }

        let target_ids: Vec<_> = target_symbols.iter().map(|s| s.id.clone()).collect();

        let callers: Vec<_> = relationships.iter()
            .filter(|rel| {
                matches!(rel.kind, crate::extractors::RelationshipKind::Calls) &&
                target_ids.iter().any(|id| rel.to_symbol_id == *id)
            })
            .collect();

        let mut message = format!("📞 Callers of '{}':\n", self.target);
        if callers.is_empty() {
            message.push_str("ℹ️ No callers found\n");
        } else {
            for rel in callers {
                if let Some(caller_symbol) = symbols.iter().find(|s| s.id == rel.from_symbol_id) {
                    message.push_str(&format!(
                        "📁 {} calls this at {}:{}\n",
                        caller_symbol.name,
                        rel.file_path,
                        rel.line_number
                    ));
                }
            }
        }

        Ok(message)
    }

    fn navigate_to_callees(&self, handler: &JulieServerHandler) -> Result<String> {
        let relationships = handler.relationships.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        let symbols = handler.symbols.read().map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        // Find the target function
        let target_symbols: Vec<_> = symbols.iter()
            .filter(|s| s.name == self.target && matches!(s.kind, SymbolKind::Function | SymbolKind::Method))
            .collect();

        if target_symbols.is_empty() {
            return Ok(format!("❌ Function '{}' not found\n", self.target));
        }

        let target_ids: Vec<_> = target_symbols.iter().map(|s| s.id.clone()).collect();

        let callees: Vec<_> = relationships.iter()
            .filter(|rel| {
                matches!(rel.kind, crate::extractors::RelationshipKind::Calls) &&
                target_ids.iter().any(|id| rel.from_symbol_id == *id)
            })
            .collect();

        let mut message = format!("📤 Functions called by '{}':\n", self.target);
        if callees.is_empty() {
            message.push_str("ℹ️ No function calls found\n");
        } else {
            for rel in callees {
                if let Some(callee_symbol) = symbols.iter().find(|s| s.id == rel.to_symbol_id) {
                    message.push_str(&format!(
                        "📁 calls {} at {}:{}\n",
                        callee_symbol.name,
                        rel.file_path,
                        rel.line_number
                    ));
                }
            }
        }

        Ok(message)
    }
}

//******************//
//   JulieTools     //
//******************//
// Generates the JulieTools enum with all tool variants
tool_box!(JulieTools, [
    IndexWorkspaceTool,
    SearchCodeTool,
    GotoDefinitionTool,
    FindReferencesTool,
    SemanticSearchTool,
    ExploreTool,
    NavigateTool
]);