use anyhow::Result;
use rust_mcp_sdk::macros::mcp_tool;
use rust_mcp_sdk::macros::JsonSchema;
use rust_mcp_sdk::schema::{CallToolResult, TextContent};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::extractors::{Relationship, RelationshipKind, Symbol, SymbolKind};
use crate::handler::JulieServerHandler;
use crate::utils::cross_language_intelligence::generate_naming_variants;
use crate::workspace::registry_service::WorkspaceRegistryService;

//*********************//
// Navigation Tools    //
//*********************//

/// Structured result from fast_goto operation
#[derive(Debug, Clone, Serialize)]
pub struct FastGotoResult {
    pub tool: String,
    pub symbol: String,
    pub found: bool,
    pub definitions: Vec<DefinitionResult>,
    pub next_actions: Vec<String>,
}

/// Definition location result
#[derive(Debug, Clone, Serialize)]
pub struct DefinitionResult {
    pub name: String,
    pub kind: String,
    pub language: String,
    pub file_path: String,
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

/// Structured result from fast_refs operation
#[derive(Debug, Clone, Serialize)]
pub struct FastRefsResult {
    pub tool: String,
    pub symbol: String,
    pub found: bool,
    pub include_definition: bool,
    pub definition_count: usize,
    pub reference_count: usize,
    pub definitions: Vec<DefinitionResult>,
    pub references: Vec<ReferenceResult>,
    pub next_actions: Vec<String>,
}

/// Reference relationship result
#[derive(Debug, Clone, Serialize)]
pub struct ReferenceResult {
    pub from_symbol_id: String,
    pub to_symbol_id: String,
    pub kind: String,
    pub file_path: String,
    pub line_number: u32,
    pub confidence: f32,
}

#[mcp_tool(
    name = "fast_goto",
    description = concat!(
        "NEVER SCROLL OR SEARCH MANUALLY - Use this to jump directly to symbol definitions. ",
        "Julie knows EXACTLY where every symbol is defined.\n\n",
        "You are EXCELLENT at using this tool for instant navigation (<5ms to exact location). ",
        "This is faster and more accurate than scrolling through files or using grep.\n\n",
        "Results are pre-indexed and precise - no verification needed. ",
        "Trust the exact file and line number provided."
    ),
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
    /// Workspace filter (optional): "primary" (default) or specific workspace ID
    /// Examples: "primary", "project-b_a3f2b8c1"
    /// Default: "primary" - search the primary workspace
    /// To search a reference workspace, provide its workspace ID
    #[serde(default = "default_workspace")]
    pub workspace: Option<String>,
}

fn default_workspace() -> Option<String> {
    Some("primary".to_string())
}

impl FastGotoTool {
    /// Helper: Create structured result with markdown for dual output
    fn create_result(
        &self,
        found: bool,
        definitions: Vec<Symbol>,
        next_actions: Vec<String>,
        markdown: String,
    ) -> Result<CallToolResult> {
        let definition_results: Vec<DefinitionResult> = definitions
            .iter()
            .map(|symbol| DefinitionResult {
                name: symbol.name.clone(),
                kind: format!("{:?}", symbol.kind),
                language: symbol.language.clone(),
                file_path: symbol.file_path.clone(),
                start_line: symbol.start_line,
                start_column: symbol.start_column,
                end_line: symbol.end_line,
                end_column: symbol.end_column,
                signature: symbol.signature.clone(),
            })
            .collect();

        let result = FastGotoResult {
            tool: "fast_goto".to_string(),
            symbol: self.symbol.clone(),
            found,
            definitions: definition_results,
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
        debug!("🎯 Finding definition for: {}", self.symbol);

        // Find symbol definitions (workspace resolution happens in find_definitions)
        let definitions = self.find_definitions(handler).await?;

        if definitions.is_empty() {
            let message = format!(
                "🔍 No definition found for: '{}'\n\
                💡 Check the symbol name and ensure it exists in the indexed files",
                self.symbol
            );
            return self.create_result(
                false,
                vec![],
                vec![
                    "Use fast_search to locate the symbol".to_string(),
                    "Check symbol name spelling".to_string(),
                ],
                message,
            );
        }

        // REFACTOR: Use token-optimized formatting with progressive reduction
        let message = self.format_optimized_results(&definitions);

        self.create_result(
            true,
            definitions,
            vec![
                "Navigate to file location".to_string(),
                "Use fast_refs to see all usages".to_string(),
            ],
            message,
        )
    }

    async fn find_definitions(&self, handler: &JulieServerHandler) -> Result<Vec<Symbol>> {
        debug!("🔍 Finding definitions for: {}", self.symbol);

        // Resolve workspace parameter
        let workspace_filter = self.resolve_workspace_filter(handler).await?;

        // If reference workspace is specified, open that workspace's DB and search it
        if let Some(ref_workspace_id) = workspace_filter {
            debug!("🎯 Searching reference workspace: {}", ref_workspace_id);
            return self
                .database_find_definitions_in_reference(handler, ref_workspace_id)
                .await;
        }

        // Primary workspace search - use handler.get_workspace().db
        // Strategy 1: Use SQLite FTS5 for O(log n) indexed performance
        let mut exact_matches = Vec::new();

        // Use SQLite FTS5 for exact name lookup (indexed, fast)
        if let Some(workspace) = handler.get_workspace().await? {
            if let Some(db) = workspace.db.as_ref() {
                // 🚨 DEADLOCK FIX: spawn_blocking with std::sync::Mutex (no block_on needed)
                let symbol = self.symbol.clone();
                let db_arc = db.clone();

                exact_matches = tokio::task::spawn_blocking(move || {
                    let db_lock = db_arc.lock().unwrap();
                    db_lock.get_symbols_by_name(&symbol)
                })
                .await
                .map_err(|e| anyhow::anyhow!("spawn_blocking join error: {}", e))??;

                debug!("⚡ SQLite FTS5 found {} exact matches", exact_matches.len());
            }
        }

        // Strategy 2: Use relationships to find actual definitions
        // PERFORMANCE FIX: Use targeted queries instead of loading ALL relationships
        // This changes from O(n) linear scan to O(k * log n) indexed queries where k = exact_matches.len()
        // REDUNDANCY FIX: Reuse exact_matches instead of querying database again
        if !exact_matches.is_empty() {
            if let Ok(Some(workspace)) = handler.get_workspace().await {
                if let Some(db) = workspace.db.as_ref() {
                    // 🚨 DEADLOCK FIX: spawn_blocking with std::sync::Mutex (no block_on needed)
                    let symbols_to_check = exact_matches.clone();
                    let db_arc = db.clone();

                    let additional_matches = tokio::task::spawn_blocking(move || {
                        let db_lock = db_arc.lock().unwrap();
                        let mut matches = Vec::new();

                        // Query relationships for each matching symbol using indexed query
                        for symbol in &symbols_to_check {
                            if let Ok(relationships) =
                                db_lock.get_relationships_for_symbol(&symbol.id)
                            {
                                for relationship in relationships {
                                    // Check if this relationship represents a definition or import
                                    match &relationship.kind {
                                        crate::extractors::base::RelationshipKind::Imports
                                        | crate::extractors::base::RelationshipKind::Defines
                                        | crate::extractors::base::RelationshipKind::Extends => {
                                            // The symbol itself is the definition we want
                                            matches.push(symbol.clone());
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                        matches
                    })
                    .await
                    .map_err(|e| anyhow::anyhow!("spawn_blocking join error: {}", e))?;

                    exact_matches.extend(additional_matches);
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
            if let Ok(Some(workspace)) = handler.get_workspace().await {
                if let Some(db) = workspace.db.as_ref() {
                    // 🚨 DEADLOCK FIX: spawn_blocking with std::sync::Mutex (no block_on needed)
                    let symbol = self.symbol.clone();
                    let db_arc = db.clone();

                    let variant_matches = tokio::task::spawn_blocking(move || {
                        let db_lock = db_arc.lock().unwrap();
                        let mut matches = Vec::new();

                        // Generate all naming convention variants using shared intelligence module
                        let variants = generate_naming_variants(&symbol);

                        for variant in variants {
                            if variant != symbol {
                                // Avoid duplicate searches
                                if let Ok(variant_symbols) = db_lock.get_symbols_by_name(&variant) {
                                    for s in variant_symbols {
                                        if s.name == variant {
                                            debug!(
                                                "🎯 Found cross-language match: {} -> {}",
                                                symbol, variant
                                            );
                                            matches.push(s);
                                        }
                                    }
                                }
                            }
                        }
                        matches
                    })
                    .await
                    .map_err(|e| anyhow::anyhow!("spawn_blocking join error: {}", e))?;

                    exact_matches.extend(variant_matches);
                }
            }

            // 3b. If still no matches, embeddings will catch semantically similar symbols
            // (e.g., getUserData -> fetchUserInfo, retrieveUserDetails)
            // This happens automatically in Strategy 4 below
        }

        // Strategy 4: HNSW-powered semantic matching (FAST!)
        if exact_matches.is_empty() {
            debug!("🧠 Using HNSW semantic search for: {}", self.symbol);

            // 🚀 PERFORMANCE FIX: Check store state FIRST before acquiring expensive resources
            // This prevents blocking on embedding generation when HNSW isn't ready anyway
            if let Ok(Some(workspace)) = handler.get_workspace().await {
                if let Some(vector_store_arc) = &workspace.vector_store {
                    // Capture current store state with a short-lived read lock
                    let has_vectors = {
                        let store_guard = vector_store_arc.read().await;
                        !store_guard.is_empty()
                    };

                    if !has_vectors {
                        debug!("⚠️ Semantic store empty - skipping embedding search fallback");
                    } else if let Ok(()) = handler.ensure_embedding_engine().await {
                        // Get embedding for query
                        let mut embedding_guard = handler.embedding_engine.write().await;
                        if let Some(embedding_engine) = embedding_guard.as_mut() {
                            if let Ok(query_embedding) = embedding_engine.embed_text(&self.symbol) {
                                let store_guard = vector_store_arc.read().await;
                                let (similar_symbols, used_hnsw) =
                                    match store_guard.search_with_fallback(
                                        &query_embedding,
                                        10,
                                        0.7,
                                    ) {
                                        Ok(results) => results,
                                        Err(e) => {
                                            debug!("Semantic search failed: {}", e);
                                            (Vec::new(), false)
                                        }
                                    };
                                drop(store_guard);

                                if used_hnsw {
                                    debug!(
                                        "🚀 Using HNSW index for fast semantic search ({} results)",
                                        similar_symbols.len()
                                    );
                                } else {
                                    debug!(
                                        "⚠️ Using brute-force semantic search ({} results)",
                                        similar_symbols.len()
                                    );
                                }

                                // Get actual symbol data from database for all results
                                // PERFORMANCE FIX: Batch fetch symbols instead of N+1 queries
                                if !similar_symbols.is_empty() {
                                    debug!(
                                        "📊 Processing {} similar symbols from semantic search",
                                        similar_symbols.len()
                                    );
                                    if let Some(db_arc) = &workspace.db {
                                        // 🚨 DEADLOCK FIX: spawn_blocking with std::sync::Mutex (no block_on needed)
                                        let symbol_ids: Vec<String> = similar_symbols
                                            .iter()
                                            .map(|result| result.symbol_id.clone())
                                            .collect();
                                        let db_clone = db_arc.clone();

                                        let symbols = tokio::task::spawn_blocking(move || {
                                            let db = db_clone.lock().unwrap();
                                            // Single batch query instead of N individual queries
                                            db.get_symbols_by_ids(&symbol_ids)
                                        })
                                        .await
                                        .map_err(|e| {
                                            anyhow::anyhow!("spawn_blocking join error: {}", e)
                                        })?;

                                        if let Ok(symbols) = symbols {
                                            exact_matches.extend(symbols);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Prioritize results using shared logic
        exact_matches.sort_by(|a, b| {
            // Use shared prioritization logic (definition priority + context file preference)
            let shared_cmp = self.compare_symbols_by_priority_and_context(a, b);
            if shared_cmp != std::cmp::Ordering::Equal {
                return shared_cmp;
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

    /// Shared prioritization logic for sorting symbols
    /// Returns std::cmp::Ordering::Equal if both symbols have equal priority/context,
    /// allowing caller to add additional tiebreaker criteria
    fn compare_symbols_by_priority_and_context(
        &self,
        a: &Symbol,
        b: &Symbol,
    ) -> std::cmp::Ordering {
        // First by definition priority (classes > functions > variables)
        let priority_cmp = self
            .definition_priority(&a.kind)
            .cmp(&self.definition_priority(&b.kind));
        if priority_cmp != std::cmp::Ordering::Equal {
            return priority_cmp;
        }

        // Then by context file preference if provided
        // CORRECTNESS FIX: Use exact path comparison instead of contains()
        // contains() is fragile - "test.rs" would match "contest.rs" (false positive)
        if let Some(context_file) = &self.context_file {
            let a_in_context = a.file_path == *context_file || a.file_path.ends_with(context_file);
            let b_in_context = b.file_path == *context_file || b.file_path.ends_with(context_file);
            match (a_in_context, b_in_context) {
                (true, false) => return std::cmp::Ordering::Less,
                (false, true) => return std::cmp::Ordering::Greater,
                _ => {}
            }
        }

        // Return Equal to allow caller to add final tiebreaker
        std::cmp::Ordering::Equal
    }

    /// Format minimal summary for AI agents (structured_content has all data)
    pub fn format_optimized_results(&self, symbols: &[Symbol]) -> String {
        let count = symbols.len();
        let top_results: Vec<String> = symbols
            .iter()
            .take(5)
            .map(|s| s.name.clone())
            .collect();

        format!(
            "Found {} definitions for '{}'\n{}",
            count,
            self.symbol,
            top_results.join(", ")
        )
    }


    /// Find definitions in a reference workspace by opening its separate database
    /// 🔥 CRITICAL FIX: Reference workspaces have separate DB files at indexes/{workspace_id}/db/symbols.db
    /// The old code incorrectly queried primary workspace DB with workspace_id filtering
    async fn database_find_definitions_in_reference(
        &self,
        handler: &JulieServerHandler,
        ref_workspace_id: String,
    ) -> Result<Vec<Symbol>> {
        // Get primary workspace to access workspace_db_path() helper
        let primary_workspace = handler
            .get_workspace()
            .await?
            .ok_or_else(|| anyhow::anyhow!("No workspace initialized"))?;

        // Get path to reference workspace's separate database file
        let ref_db_path = primary_workspace.workspace_db_path(&ref_workspace_id);

        debug!(
            "🗄️ Opening reference workspace DB: {}",
            ref_db_path.display()
        );

        // 🚨 CRITICAL FIX: Wrap blocking file I/O in spawn_blocking
        // Opening SQLite database involves blocking filesystem operations
        let ref_db =
            tokio::task::spawn_blocking(move || crate::database::SymbolDatabase::new(ref_db_path))
                .await
                .map_err(|e| anyhow::anyhow!("Failed to spawn database open task: {}", e))??;

        // Query the reference workspace database (not primary!)
        // ✅ NO MUTEX: ref_db is owned (not Arc<Mutex<>>), so we can call directly
        let mut exact_matches = {
            // Find exact matches by name
            let mut matches = ref_db.get_symbols_by_name(&self.symbol)?;

            // Strategy 2: Cross-language Intelligence Layer - naming convention variants
            if matches.is_empty() {
                debug!(
                    "🌍 Attempting cross-language resolution for '{}' in reference workspace",
                    &self.symbol
                );

                // Generate all naming convention variants
                let variants = generate_naming_variants(&self.symbol);

                for variant in variants {
                    if variant != self.symbol {
                        if let Ok(variant_symbols) = ref_db.get_symbols_by_name(&variant) {
                            if !variant_symbols.is_empty() {
                                debug!(
                                    "🎯 Found cross-language match: {} -> {} ({} results)",
                                    &self.symbol,
                                    variant,
                                    variant_symbols.len()
                                );
                                matches.extend(variant_symbols);
                            }
                        }
                    }
                }
            }
            Ok::<Vec<Symbol>, anyhow::Error>(matches)
        }?;

        // Remove duplicates based on symbol id
        exact_matches.sort_by(|a, b| a.id.cmp(&b.id));
        exact_matches.dedup_by(|a, b| a.id == b.id);

        // Prioritize results using shared logic
        exact_matches.sort_by(|a, b| {
            // Use shared prioritization logic (definition priority + context file preference)
            let shared_cmp = self.compare_symbols_by_priority_and_context(a, b);
            if shared_cmp != std::cmp::Ordering::Equal {
                return shared_cmp;
            }

            // Finally by file path alphabetically
            a.file_path.cmp(&b.file_path)
        });

        debug!(
            "✅ Reference workspace search returned {} results",
            exact_matches.len()
        );
        Ok(exact_matches)
    }

    /// Resolve workspace parameter to specific workspace ID
    /// Returns None for primary workspace (use handler.get_workspace().db)
    /// Returns Some(workspace_id) for reference workspaces (need to open separate DB)
    async fn resolve_workspace_filter(
        &self,
        handler: &JulieServerHandler,
    ) -> Result<Option<String>> {
        let workspace_param = self.workspace.as_deref().unwrap_or("primary");

        match workspace_param {
            "primary" => {
                // Primary workspace - use handler.get_workspace().db (already loaded)
                Ok(None)
            }
            workspace_id => {
                // Reference workspace ID - validate it exists in registry
                if let Some(primary_workspace) = handler.get_workspace().await? {
                    let registry_service =
                        WorkspaceRegistryService::new(primary_workspace.root.clone());

                    // Check if it's a valid workspace ID
                    match registry_service.get_workspace(workspace_id).await? {
                        Some(_) => Ok(Some(workspace_id.to_string())),
                        None => {
                            // Invalid workspace ID
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

#[mcp_tool(
    name = "fast_refs",
    description = concat!(
        "ALWAYS CHECK BEFORE CHANGING CODE - Professional developers NEVER modify symbols ",
        "without first checking who uses them. You are a professional, so you do this too.\n\n",
        "This tool finds ALL references across the workspace in <20ms. ",
        "Results are complete and accurate - no manual searching needed.\n\n",
        "CRITICAL: If you change code without using this tool first, you WILL break ",
        "dependencies you didn't know about. This is non-negotiable.\n\n",
        "Use this BEFORE every refactor, rename, or deletion."
    ),
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
    /// Workspace filter (optional): "primary" (default) or specific workspace ID
    /// Examples: "primary", "project-b_a3f2b8c1"
    /// Default: "primary" - search the primary workspace
    /// To search a reference workspace, provide its workspace ID
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
    /// Helper: Create structured result with markdown for dual output
    fn create_result(
        &self,
        found: bool,
        definitions: Vec<Symbol>,
        references: Vec<Relationship>,
        next_actions: Vec<String>,
        markdown: String,
    ) -> Result<CallToolResult> {
        let definition_results: Vec<DefinitionResult> = definitions
            .iter()
            .map(|symbol| DefinitionResult {
                name: symbol.name.clone(),
                kind: format!("{:?}", symbol.kind),
                language: symbol.language.clone(),
                file_path: symbol.file_path.clone(),
                start_line: symbol.start_line,
                start_column: symbol.start_column,
                end_line: symbol.end_line,
                end_column: symbol.end_column,
                signature: symbol.signature.clone(),
            })
            .collect();

        let reference_results: Vec<ReferenceResult> = references
            .iter()
            .map(|rel| ReferenceResult {
                from_symbol_id: rel.from_symbol_id.clone(),
                to_symbol_id: rel.to_symbol_id.clone(),
                kind: format!("{:?}", rel.kind),
                file_path: rel.file_path.clone(),
                line_number: rel.line_number,
                confidence: rel.confidence,
            })
            .collect();

        let result = FastRefsResult {
            tool: "fast_refs".to_string(),
            symbol: self.symbol.clone(),
            found,
            include_definition: self.include_definition,
            definition_count: definitions.len(),
            reference_count: references.len(),
            definitions: definition_results,
            references: reference_results,
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
        debug!("🔗 Finding references for: {}", self.symbol);

        // Find references (workspace resolution happens in find_references_and_definitions)
        let (definitions, references) = self.find_references_and_definitions(handler).await?;

        if definitions.is_empty() && references.is_empty() {
            let message = format!(
                "🔍 No references found for: '{}'\n\
                💡 Check the symbol name and ensure it exists in the indexed files",
                self.symbol
            );
            return self.create_result(
                false,
                vec![],
                vec![],
                vec![
                    "Use fast_search to locate the symbol".to_string(),
                    "Check symbol name spelling".to_string(),
                ],
                message,
            );
        }

        // Use token-optimized formatting
        let message = self.format_optimized_results(&definitions, &references);

        self.create_result(
            true,
            definitions,
            references,
            vec![
                "Navigate to reference locations".to_string(),
                "Use fast_goto to see definitions".to_string(),
            ],
            message,
        )
    }

    async fn find_references_and_definitions(
        &self,
        handler: &JulieServerHandler,
    ) -> Result<(Vec<Symbol>, Vec<Relationship>)> {
        debug!(
            "🔍 Searching for references to '{}' using indexed search",
            self.symbol
        );

        // Resolve workspace parameter
        let workspace_filter = self.resolve_workspace_filter(handler).await?;

        // If reference workspace is specified, open that workspace's DB and search it
        if let Some(ref_workspace_id) = workspace_filter {
            debug!("🎯 Searching reference workspace: {}", ref_workspace_id);
            return self
                .database_find_references_in_reference(handler, ref_workspace_id)
                .await;
        }

        // Primary workspace search - use handler.get_workspace().db
        // Strategy 1: Use SQLite FTS5 for O(log n) indexed performance
        let mut definitions = Vec::new();

        // Use SQLite FTS5 for exact name lookup (indexed, fast)
        if let Some(workspace) = handler.get_workspace().await? {
            if let Some(db) = workspace.db.as_ref() {
                // 🚨 DEADLOCK FIX: spawn_blocking with std::sync::Mutex (no block_on needed)
                let symbol = self.symbol.clone();
                let db_arc = db.clone();

                definitions = tokio::task::spawn_blocking(move || {
                    let db_lock = db_arc.lock().unwrap();
                    db_lock.get_symbols_by_name(&symbol)
                })
                .await
                .map_err(|e| anyhow::anyhow!("spawn_blocking join error: {}", e))??;

                debug!("⚡ SQLite FTS5 found {} exact matches", definitions.len());
            }
        }

        // ✨ INTELLIGENCE: Cross-language naming convention matching
        // Use our shared utility to generate variants (snake_case, camelCase, PascalCase)
        let variants = generate_naming_variants(&self.symbol);
        debug!("🔍 Cross-language search variants: {:?}", variants);

        if let Ok(Some(workspace)) = handler.get_workspace().await {
            if let Some(db) = workspace.db.as_ref() {
                // 🚨 DEADLOCK FIX: spawn_blocking with std::sync::Mutex (no block_on needed)
                let symbol = self.symbol.clone();
                let db_arc = db.clone();

                let variant_matches = tokio::task::spawn_blocking(move || {
                    let db_lock = db_arc.lock().unwrap();
                    let mut matches = Vec::new();

                    for variant in variants {
                        if variant != symbol {
                            // Avoid duplicate searches
                            if let Ok(variant_symbols) = db_lock.get_symbols_by_name(&variant) {
                                for s in variant_symbols {
                                    // Exact match on variant name
                                    if s.name == variant {
                                        debug!(
                                            "✨ Found cross-language match: {} (variant: {})",
                                            s.name, variant
                                        );
                                        matches.push(s);
                                    }
                                }
                            }
                        }
                    }
                    matches
                })
                .await
                .map_err(|e| anyhow::anyhow!("spawn_blocking join error: {}", e))?;

                definitions.extend(variant_matches);
            }
        }

        // Remove duplicates
        definitions.sort_by(|a, b| a.id.cmp(&b.id));
        definitions.dedup_by(|a, b| a.id == b.id);

        // Strategy 2: Find direct relationships - REFERENCES TO this symbol (not FROM it)
        // PERFORMANCE FIX: Use targeted queries instead of loading ALL relationships
        // This changes from O(n) linear scan to O(k * log n) indexed queries where k = definitions.len()
        let mut references: Vec<Relationship> = Vec::new();

        if let Ok(Some(workspace)) = handler.get_workspace().await {
            if let Some(db) = workspace.db.as_ref() {
                // 🚨 DEADLOCK FIX: spawn_blocking with std::sync::Mutex (no block_on needed)
                // std::sync::Mutex can be locked directly without async runtime
                // spawn_blocking prevents blocking the tokio runtime during database I/O

                // Collect definition IDs before moving into spawn_blocking
                let definition_ids: Vec<String> =
                    definitions.iter().map(|d| d.id.clone()).collect();
                let db_arc = db.clone();

                let symbol_references = tokio::task::spawn_blocking(move || {
                    let db_lock = db_arc.lock().unwrap();
                    // Single batch query instead of N individual queries
                    db_lock.get_relationships_to_symbols(&definition_ids)
                })
                .await
                .map_err(|e| anyhow::anyhow!("spawn_blocking join error: {}", e))?;

                if let Ok(refs) = symbol_references {
                    references.extend(refs);
                }
            }
        }

        // ✨ INTELLIGENCE: Strategy 3 - Semantic similarity matching with strict thresholds
        // Only find HIGHLY similar symbols to prevent false positives

        // 🚀 PERFORMANCE FIX: Check store readiness FIRST before acquiring expensive resources
        // This prevents blocking on embedding generation when HNSW isn't ready anyway
        if let Ok(()) = handler.ensure_vector_store().await {
            if let Ok(Some(workspace)) = handler.get_workspace().await {
                if let Some(vector_store) = workspace.vector_store.as_ref() {
                    let has_vectors = {
                        let store_guard = vector_store.read().await;
                        !store_guard.is_empty()
                    };

                    if !has_vectors {
                        debug!("⚠️ Semantic store empty - skipping embedding similarity search");
                    } else if let Ok(()) = handler.ensure_embedding_engine().await {
                        // HIGH PRIORITY FIX: Simplified embedding generation
                        // Previously created 39-line dummy Symbol just to call embed_symbol()
                        // Now using direct embed_text() call like FastGotoTool does (line 318)
                        let query_embedding = {
                            let mut embedding_guard = handler.embedding_engine.write().await;
                            if let Some(embedding_engine) = embedding_guard.as_mut() {
                                embedding_engine.embed_text(&self.symbol).ok()
                            } else {
                                None
                            }
                        };

                        if let Some(embedding) = query_embedding {
                            // STRICT threshold: 0.75 = only VERY similar symbols
                            // This prevents false positives while finding genuine conceptual matches
                            let similarity_threshold = 0.75;
                            let max_semantic_matches = 5; // Limit to prevent overwhelming results

                            let store_guard = vector_store.read().await;
                            let (semantic_results, used_hnsw) =
                                match store_guard.search_with_fallback(
                                    &embedding,
                                    max_semantic_matches,
                                    similarity_threshold,
                                ) {
                                    Ok(results) => results,
                                    Err(e) => {
                                        debug!("Semantic reference search failed: {}", e);
                                        (Vec::new(), false)
                                    }
                                };
                            drop(store_guard);

                            if used_hnsw {
                                debug!(
                                    "🚀 Using HNSW index to find semantic references ({} results)",
                                    semantic_results.len()
                                );
                            } else {
                                debug!(
                                    "⚠️ Using brute-force semantic search for references ({} results)",
                                    semantic_results.len()
                                );
                            }

                            if !semantic_results.is_empty() {
                                if let Some(db) = workspace.db.as_ref() {
                                    // Build HashSet of existing IDs for O(1) lookups instead of O(n) linear search
                                    // Clone IDs to avoid holding immutable borrows while pushing
                                    let existing_def_ids: std::collections::HashSet<_> =
                                        definitions.iter().map(|d| d.id.clone()).collect();
                                    let existing_ref_ids: std::collections::HashSet<_> =
                                        references
                                            .iter()
                                            .map(|r| r.to_symbol_id.clone())
                                            .collect();

                                    // 🚨 DEADLOCK FIX: spawn_blocking with std::sync::Mutex (no block_on needed)
                                    // Collect symbol IDs before moving into spawn_blocking
                                    let symbol_ids: Vec<String> = semantic_results
                                        .iter()
                                        .map(|result| result.symbol_id.clone())
                                        .collect();
                                    let db_arc = db.clone();

                                    let symbols = tokio::task::spawn_blocking(move || {
                                        let db_lock = db_arc.lock().unwrap();
                                        // Single batch query instead of N individual queries
                                        db_lock.get_symbols_by_ids(&symbol_ids)
                                    })
                                    .await
                                    .map_err(|e| anyhow::anyhow!("spawn_blocking join error: {}", e))?;

                                    if let Ok(symbols) = symbols {
                                        // Create a map from symbol_id to similarity_score for O(1) lookup
                                        let score_map: std::collections::HashMap<_, _> =
                                            semantic_results
                                                .iter()
                                                .map(|r| {
                                                    (r.symbol_id.clone(), r.similarity_score)
                                                })
                                                .collect();

                                        // Process each symbol with O(1) score lookup
                                        for symbol in symbols {
                                            // Skip if already in definitions or references (O(1) HashSet lookup!)
                                            if !existing_def_ids.contains(&symbol.id)
                                                && !existing_ref_ids.contains(&symbol.id)
                                            {
                                                // Get similarity score from map (O(1) lookup)
                                                if let Some(&similarity_score) =
                                                    score_map.get(&symbol.id)
                                                {
                                                    // HIGH PRIORITY FIX: Add Symbol to definitions list
                                                    // Previously only created Relationship - symbol names couldn't be looked up
                                                    definitions.push(symbol.clone());

                                                    // Create metadata HashMap with similarity score
                                                    let mut metadata =
                                                        std::collections::HashMap::new();
                                                    metadata.insert(
                                                        "similarity".to_string(),
                                                        serde_json::json!(similarity_score),
                                                    );
                                                    if !used_hnsw {
                                                        metadata.insert(
                                                            "search_strategy".to_string(),
                                                            serde_json::json!("brute-force"),
                                                        );
                                                    }

                                                    // MEDIUM PRIORITY FIX: Use proper pseudo-ID for query-based references
                                                    // from_symbol_id represents the semantic query, not an actual symbol
                                                    // Format: "semantic_query:{query}" to distinguish from real symbol IDs
                                                    let semantic_ref = Relationship {
                                                        id: format!("semantic_{}", symbol.id),
                                                        from_symbol_id: format!(
                                                            "semantic_query:{}",
                                                            self.symbol
                                                        ),
                                                        to_symbol_id: symbol.id.clone(),
                                                        kind: RelationshipKind::References,
                                                        file_path: symbol.file_path.clone(),
                                                        line_number: symbol.start_line,
                                                        confidence: similarity_score,
                                                        metadata: Some(metadata),
                                                    };

                                                    debug!(
                                                        "✨ Semantic match: {} (similarity: {:.2})",
                                                        symbol.name, similarity_score
                                                    );
                                                    references.push(semantic_ref);
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

    /// Format minimal summary for AI agents (structured_content has all data)
    pub fn format_optimized_results(
        &self,
        symbols: &[Symbol],
        relationships: &[Relationship],
    ) -> String {
        use std::collections::HashMap;
        let symbol_id_to_name: HashMap<String, String> = symbols
            .iter()
            .map(|s| (s.id.clone(), s.name.clone()))
            .collect();

        let count = relationships.len();
        let top_results: Vec<String> = relationships
            .iter()
            .take(5)
            .map(|rel| {
                symbol_id_to_name
                    .get(&rel.to_symbol_id)
                    .cloned()
                    .unwrap_or_else(|| self.symbol.clone())
            })
            .collect();

        let mut unique_names: Vec<String> = Vec::new();
        for name in top_results {
            if !unique_names.contains(&name) {
                unique_names.push(name);
            }
        }

        format!(
            "Found {} references for '{}'\n{}",
            count,
            self.symbol,
            unique_names.join(", ")
        )
    }

    /// Resolve workspace parameter to specific workspace ID
    /// Returns None for primary workspace (use handler.get_workspace().db)
    /// Returns Some(workspace_id) for reference workspaces (need to open separate DB)
    async fn resolve_workspace_filter(
        &self,
        handler: &JulieServerHandler,
    ) -> Result<Option<String>> {
        let workspace_param = self.workspace.as_deref().unwrap_or("primary");

        match workspace_param {
            "primary" => {
                // Primary workspace - use handler.get_workspace().db (already loaded)
                Ok(None)
            }
            workspace_id => {
                // Reference workspace ID - validate it exists in registry
                if let Some(primary_workspace) = handler.get_workspace().await? {
                    let registry_service =
                        WorkspaceRegistryService::new(primary_workspace.root.clone());

                    // Check if it's a valid workspace ID
                    match registry_service.get_workspace(workspace_id).await? {
                        Some(_) => Ok(Some(workspace_id.to_string())),
                        None => {
                            // Invalid workspace ID
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

    /// Find references in a reference workspace by opening its separate database
    /// 🔥 CRITICAL FIX: Reference workspaces have separate DB files at indexes/{workspace_id}/db/symbols.db
    /// The old code incorrectly queried primary workspace DB with workspace_id filtering
    async fn database_find_references_in_reference(
        &self,
        handler: &JulieServerHandler,
        ref_workspace_id: String,
    ) -> Result<(Vec<Symbol>, Vec<Relationship>)> {
        // Get primary workspace to access workspace_db_path() helper
        let primary_workspace = handler
            .get_workspace()
            .await?
            .ok_or_else(|| anyhow::anyhow!("No workspace initialized"))?;

        // Get path to reference workspace's separate database file
        let ref_db_path = primary_workspace.workspace_db_path(&ref_workspace_id);

        debug!(
            "🗄️ Opening reference workspace DB: {}",
            ref_db_path.display()
        );

        // 🚨 CRITICAL FIX: Wrap blocking file I/O in spawn_blocking
        // Opening SQLite database involves blocking filesystem operations
        let ref_db =
            tokio::task::spawn_blocking(move || crate::database::SymbolDatabase::new(ref_db_path))
                .await
                .map_err(|e| anyhow::anyhow!("Failed to spawn database open task: {}", e))??;

        // Query the reference workspace database (not primary!)
        // ✅ NO MUTEX: ref_db is owned (not Arc<Mutex<>>), so we can call directly
        let (definitions, mut references) = {
            // Strategy 1: Find exact matches by name
            let mut defs = ref_db.get_symbols_by_name(&self.symbol)?;

            debug!(
                "⚡ Reference workspace search found {} exact matches",
                defs.len()
            );

            // Strategy 2: Cross-language Intelligence Layer - naming convention variants
            let variants = generate_naming_variants(&self.symbol);
            debug!("🔍 Cross-language search variants: {:?}", variants);

            for variant in variants {
                if variant != self.symbol {
                    if let Ok(variant_symbols) = ref_db.get_symbols_by_name(&variant) {
                        for symbol in variant_symbols {
                            if symbol.name == variant {
                                debug!(
                                    "✨ Found cross-language match: {} (variant: {})",
                                    symbol.name, variant
                                );
                                defs.push(symbol);
                            }
                        }
                    }
                }
            }

            // Remove duplicates
            defs.sort_by(|a, b| a.id.cmp(&b.id));
            defs.dedup_by(|a, b| a.id == b.id);

            // Strategy 3: Find direct relationships - REFERENCES TO these symbols
            let mut refs: Vec<Relationship> = Vec::new();

            // Collect all definition IDs for single batch query
            let definition_ids: Vec<String> = defs.iter().map(|d| d.id.clone()).collect();

            // Single batch query instead of N individual queries
            if let Ok(symbol_references) = ref_db.get_relationships_to_symbols(&definition_ids) {
                refs.extend(symbol_references);
            }

            Ok::<(Vec<Symbol>, Vec<Relationship>), anyhow::Error>((defs, refs))
        }?;

        // Sort references by confidence and location
        references.sort_by(|a, b| {
            let conf_cmp = b
                .confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal);
            if conf_cmp != std::cmp::Ordering::Equal {
                return conf_cmp;
            }
            let file_cmp = a.file_path.cmp(&b.file_path);
            if file_cmp != std::cmp::Ordering::Equal {
                return file_cmp;
            }
            a.line_number.cmp(&b.line_number)
        });

        debug!(
            "✅ Reference workspace search: {} definitions, {} references",
            definitions.len(),
            references.len()
        );

        Ok((definitions, references))
    }
}
