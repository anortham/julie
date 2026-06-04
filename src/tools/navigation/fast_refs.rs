//! FastRefsTool - Find all references to a symbol
//!
//! This tool finds all usages and references across the codebase using:
//! 1. SQLite symbols table for O(log n) exact name matching
//! 2. Cross-language naming convention variants (snake_case, camelCase, etc.)
//! 3. Relationships table for caller→callee connections
//! 4. Identifiers table for usage sites (calls, type usages, member access, imports)

use crate::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use anyhow::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::formatting::format_lean_refs_results;
use super::resolution::{WorkspaceTarget, parse_qualified_name};
use super::target_workspace;
use crate::extractors::{Relationship, RelationshipKind, Symbol, SymbolKind};
use crate::utils::cross_language_intelligence::generate_naming_variants;
use julie_context::ToolContext;
use std::collections::{HashMap, HashSet};

fn default_true() -> bool {
    true
}

fn default_limit() -> u32 {
    10 // Reduced from 50 for Julie 2.0 token efficiency (80% reduction)
}

fn default_workspace() -> Option<String> {
    Some("primary".to_string())
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct FastRefsTool {
    /// Symbol name (supports qualified names)
    pub symbol: String,
    /// Include definition in results (default: true)
    #[serde(
        default = "default_true",
        deserialize_with = "crate::utils::serde_lenient::deserialize_bool_lenient"
    )]
    pub include_definition: bool,
    /// Maximum references (default: 10, range: 1-500)
    #[serde(
        default = "default_limit",
        deserialize_with = "crate::utils::serde_lenient::deserialize_u32_lenient"
    )]
    pub limit: u32,
    /// Workspace filter: "primary" (default) or a workspace ID
    #[serde(default = "default_workspace")]
    pub workspace: Option<String>,
    /// Narrow by reference kind: "call", "variable_ref", "type_usage", "member_access", "import". Omit to see all reference types
    #[serde(default)]
    pub reference_kind: Option<String>,
}

impl FastRefsTool {
    /// Create lean text result for references
    fn create_result(
        &self,
        definitions: Vec<Symbol>,
        references: Vec<Relationship>,
        source_names: &HashMap<String, String>,
    ) -> Result<CallToolResult> {
        let lean_output =
            format_lean_refs_results(&self.symbol, &definitions, &references, source_names);
        Ok(CallToolResult::text_content(vec![Content::text(
            lean_output,
        )]))
    }

    /// When zero references are found, try semantic similarity as a fallback.
    /// Embeds the symbol name on the fly and finds similar symbols by vector distance.
    /// Returns formatted semantic results or empty string.
    /// Skips for some explicit workspace queries when embeddings are unavailable.
    async fn try_semantic_fallback(
        &self,
        handler: &dyn ToolContext,
        workspace_target: &WorkspaceTarget,
    ) -> String {
        use super::formatting::format_semantic_fallback;
        use crate::search::similarity;

        // Embedding provider: prefer daemon shared service, fall back to workspace
        let provider = match handler.embedding_provider().await {
            Some(p) => p,
            None => return String::new(),
        };

        // Embed the symbol name on the fly — no need for it to exist in the DB
        let query_vector = match provider.embed_query(&self.symbol) {
            Ok(vec) => vec,
            Err(_) => return String::new(),
        };

        // Use a lower threshold than MIN_SIMILARITY_SCORE (0.5) because we're
        // comparing a raw symbol name against rich metadata embeddings (kind +
        // name + signature + docstring). Different input domains = lower scores.
        const QUERY_SIMILARITY_THRESHOLD: f32 = 0.2;

        // Pooled DB: read-only, no mutation gate required.
        let pooled_db = match workspace_target {
            WorkspaceTarget::Target(target_workspace_id) => {
                debug!("Semantic fallback: workspace '{}'", target_workspace_id);
                match handler
                    .get_pooled_database_for_workspace(target_workspace_id)
                    .await
                {
                    Ok(db) => db,
                    Err(e) => {
                        debug!(
                            "Semantic fallback: DB error for '{}': {}",
                            target_workspace_id, e
                        );
                        return String::new();
                    }
                }
            }
            WorkspaceTarget::Primary => match handler.primary_pooled_database().await {
                Ok(db) => db,
                Err(_) => return String::new(),
            },
        };
        let similar = match similarity::find_similar_by_query(
            &pooled_db,
            &query_vector,
            5,
            QUERY_SIMILARITY_THRESHOLD,
        ) {
            Ok(results) => results,
            Err(e) => {
                debug!("Semantic fallback: KNN error: {}", e);
                return String::new();
            }
        };
        format_semantic_fallback(&self.symbol, &similar)
    }

    pub async fn call_tool(&self, handler: &dyn ToolContext) -> Result<CallToolResult> {
        // Resolve workspace target (primary or explicit workspace). The helpers
        // below each acquire their own pooled DB internally — there's no longer
        // a shared Arc<Mutex<>> passed around (see A2.2c follow-up).
        let workspace_target = handler.resolve_workspace_target(self.workspace.as_deref()).await?;
        self.call_tool_with_target(handler, &workspace_target).await
    }

    /// Same as `call_tool`, but uses a workspace target that the caller has
    /// already resolved. Tool wrappers in `src/handler/tools/` call this so the
    /// workspace is resolved exactly once per request (used for both metrics
    /// attribution and the actual tool call).
    pub async fn call_tool_with_target(
        &self,
        handler: &dyn ToolContext,
        workspace_target: &WorkspaceTarget,
    ) -> Result<CallToolResult> {
        debug!("Finding references for: {}", self.symbol);

        // Find references (workspace resolution is handled by workspace_target)
        let (definitions, references) = self
            .find_references_and_definitions(handler, workspace_target.clone())
            .await?;

        if definitions.is_empty() && references.is_empty() {
            // Attempt semantic fallback (works for both primary and explicit workspaces)
            let semantic_section = self.try_semantic_fallback(handler, workspace_target).await;

            let empty_names = HashMap::new();
            let mut result_text = format_lean_refs_results(&self.symbol, &[], &[], &empty_names);
            result_text.push_str(&semantic_section);
            return Ok(CallToolResult::text_content(vec![Content::text(
                result_text,
            )]));
        }

        // Resolve from_symbol_id → name for each reference so the formatter
        // can show the calling symbol's name (e.g., "format_definition_search_results (Calls)")
        let source_names = self
            .resolve_source_names(handler, &references, workspace_target)
            .await;

        // Respect include_definition parameter
        let defs = if self.include_definition {
            definitions
        } else {
            vec![]
        };

        self.create_result(defs, references, &source_names)
    }

    /// Batch-resolve from_symbol_id values to symbol names for reference display.
    ///
    /// Routes to the correct workspace DB via the pooled accessor: explicit
    /// workspaces use `get_pooled_database_for_workspace`; primary uses
    /// `primary_pooled_database`.
    async fn resolve_source_names(
        &self,
        handler: &dyn ToolContext,
        references: &[Relationship],
        workspace_target: &WorkspaceTarget,
    ) -> HashMap<String, String> {
        let ids: Vec<String> = references
            .iter()
            .map(|r| r.from_symbol_id.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();

        if ids.is_empty() {
            return HashMap::new();
        }

        // Pooled DB: read-only, no mutation gate required.
        let pooled_db = match workspace_target {
            WorkspaceTarget::Target(target_workspace_id) => {
                match handler
                    .get_pooled_database_for_workspace(target_workspace_id)
                    .await
                {
                    Ok(db) => db,
                    Err(_) => return HashMap::new(),
                }
            }
            WorkspaceTarget::Primary => match handler.primary_pooled_database().await {
                Ok(db) => db,
                Err(_) => return HashMap::new(),
            },
        };

        tokio::task::spawn_blocking(move || match pooled_db.get_symbols_by_ids(&ids) {
            Ok(symbols) => symbols
                .into_iter()
                .map(|s| (s.id.clone(), s.name.clone()))
                .collect(),
            Err(_) => HashMap::new(),
        })
        .await
        .unwrap_or_default()
    }

    pub async fn find_references_and_definitions(
        &self,
        handler: &dyn ToolContext,
        workspace_target: WorkspaceTarget,
    ) -> Result<(Vec<Symbol>, Vec<Relationship>)> {
        debug!(
            "Searching for references to '{}' using indexed search",
            self.symbol
        );

        match workspace_target {
            WorkspaceTarget::Target(target_workspace_id) => {
                debug!("Searching target workspace: {}", target_workspace_id);
                return self
                    .database_find_references_in_target_workspace(handler, target_workspace_id)
                    .await;
            }
            WorkspaceTarget::Primary => {
                // Fall through to primary workspace search below
            }
        }

        // Resolve qualified names: "SearchIndex::search_symbols" → search "search_symbols" filtered by parent
        let (effective_symbol, parent_filter) = match parse_qualified_name(&self.symbol) {
            Some((parent, child)) => {
                debug!("Qualified name: parent='{}', child='{}'", parent, child);
                (child.to_string(), Some(parent.to_string()))
            }
            None => (self.symbol.clone(), None),
        };

        // Pooled DB: read-only, no mutation gate required. The five separate
        // spawn_blocking calls of the prior Arc<Mutex<>> implementation are
        // consolidated here into one, since the owned pooled SymbolDatabase
        // can't be cloned across spawn_blocking boundaries.
        let pooled_db = handler.primary_pooled_database().await?;
        let symbol_owned = effective_symbol.clone();
        let parent_filter_owned = parent_filter.clone();
        let reference_kind_filter = self.reference_kind.clone();
        let limit = self.limit as usize;
        let self_symbol = self.symbol.clone();

        let (definitions, references) =
            tokio::task::spawn_blocking(move || -> Result<(Vec<Symbol>, Vec<Relationship>)> {
                // Strategy 1: exact-name lookup via SQLite (O(log n))
                let mut definitions = pooled_db.get_symbols_by_name(&symbol_owned)?;

                // Apply parent filter for qualified names like Foo::bar
                if let Some(ref parent_name) = parent_filter_owned {
                    let parent_ids: Vec<String> = definitions
                        .iter()
                        .filter_map(|s| s.parent_id.clone())
                        .collect::<HashSet<_>>()
                        .into_iter()
                        .collect();

                    if !parent_ids.is_empty() {
                        let parents = pooled_db.get_symbols_by_ids(&parent_ids)?;
                        let matching_parent_ids: HashSet<String> = parents
                            .into_iter()
                            .filter(|p| p.name == *parent_name)
                            .map(|p| p.id)
                            .collect();

                        definitions.retain(|s| {
                            s.parent_id
                                .as_deref()
                                .map(|pid| matching_parent_ids.contains(pid))
                                .unwrap_or(false)
                        });
                    } else {
                        definitions.clear();
                    }
                }

                debug!("⚡ SQLite found {} exact matches", definitions.len());

                // Strategy 2: Cross-language naming convention variants
                let variants = generate_naming_variants(&symbol_owned);
                debug!("🔍 Cross-language search variants: {:?}", variants);

                if definitions.is_empty() {
                    for variant in &variants {
                        if *variant != symbol_owned {
                            if let Ok(variant_symbols) = pooled_db.get_symbols_by_name(variant) {
                                for s in variant_symbols {
                                    if s.name == *variant {
                                        debug!(
                                            "✨ Found cross-language match: {} (variant: {})",
                                            s.name, variant
                                        );
                                        definitions.push(s);
                                    }
                                }
                            }
                        }
                    }
                }

                // Dedup definitions
                definitions.sort_by(|a, b| a.id.cmp(&b.id));
                definitions.dedup_by(|a, b| a.id == b.id);

                // Separate imports from true definitions
                let mut import_refs: Vec<Relationship> = Vec::new();
                definitions.retain(|sym| {
                    if sym.kind == SymbolKind::Import {
                        import_refs.push(Relationship {
                            id: format!("import_{}_{}", sym.file_path, sym.start_line),
                            from_symbol_id: sym.id.clone(),
                            to_symbol_id: String::new(),
                            kind: RelationshipKind::Imports,
                            file_path: sym.file_path.clone(),
                            line_number: sym.start_line,
                            confidence: 1.0,
                            metadata: None,
                        });
                        false
                    } else {
                        true
                    }
                });

                // Filter synthetic import refs if reference_kind is set and isn't "import"
                let mut references: Vec<Relationship> = match reference_kind_filter.as_deref() {
                    Some(kind) if kind != "import" => Vec::new(),
                    _ => import_refs,
                };

                // Strategy 3: relationships table — direct REFERENCES TO these symbols
                let definition_ids: Vec<String> =
                    definitions.iter().map(|d| d.id.clone()).collect();

                let rel_results = match reference_kind_filter.as_deref() {
                    Some(kind) => pooled_db
                        .get_relationships_to_symbols_filtered_by_kind(&definition_ids, kind),
                    None => pooled_db.get_relationships_to_symbols(&definition_ids),
                };
                if let Ok(refs) = rel_results {
                    references.extend(refs);
                }

                // Strategy 4: identifiers table — catches usages that relationships miss
                let mut all_names = vec![symbol_owned.clone()];
                for v in &variants {
                    if *v != symbol_owned {
                        all_names.push(v.clone());
                    }
                }

                let first_def_id = definitions
                    .first()
                    .map(|d| d.id.clone())
                    .unwrap_or_default();
                let resolved_definition_ids: HashSet<String> =
                    definitions.iter().map(|d| d.id.clone()).collect();
                let qualified_lookup = parent_filter_owned.is_some();

                let identifier_refs = match reference_kind_filter.as_deref() {
                    Some(kind) => pooled_db
                        .get_identifiers_by_names_and_kind(&all_names, kind)
                        .unwrap_or_default(),
                    None => pooled_db
                        .get_identifiers_by_names(&all_names)
                        .unwrap_or_default(),
                };

                // Build dedup set from existing relationships AND definitions
                // so identifier entries at definition sites don't create duplicates
                let mut existing_refs: HashSet<(String, u32)> = references
                    .iter()
                    .map(|r| (r.file_path.clone(), r.line_number))
                    .collect();
                for def in &definitions {
                    existing_refs.insert((def.file_path.clone(), def.start_line));
                }

                let mut added = 0;
                for ident in identifier_refs {
                    let key = (ident.file_path.clone(), ident.start_line);
                    if existing_refs.contains(&key) {
                        continue;
                    }

                    if qualified_lookup
                        && !ident
                            .target_symbol_id
                            .as_deref()
                            .map(|target_id| resolved_definition_ids.contains(target_id))
                            .unwrap_or(false)
                    {
                        continue;
                    }

                    let rel_kind = match ident.kind.as_str() {
                        "call" => RelationshipKind::Calls,
                        "import" => RelationshipKind::Imports,
                        "type_usage" => RelationshipKind::Uses,
                        "member_access" => RelationshipKind::References,
                        _ => RelationshipKind::References,
                    };

                    references.push(Relationship {
                        id: format!("ident_{}_{}", ident.file_path, ident.start_line),
                        from_symbol_id: ident.containing_symbol_id.unwrap_or_default(),
                        to_symbol_id: first_def_id.clone(),
                        kind: rel_kind,
                        file_path: ident.file_path,
                        line_number: ident.start_line,
                        confidence: ident.confidence,
                        metadata: None,
                    });
                    existing_refs.insert(key);
                    added += 1;
                }

                debug!(
                    "🔓 Identifiers added {} new references (deduped from existing relationships)",
                    added
                );

                Ok((definitions, references))
            })
            .await
            .map_err(|e| anyhow::anyhow!("spawn_blocking join error: {}", e))??;

        let mut references = references;

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

        // Apply user-specified limit to prevent massive responses
        references.truncate(limit);

        // Cap definitions — large counts signal cross-language naming collisions
        const MAX_DEFINITIONS: usize = 50;
        if definitions.len() > MAX_DEFINITIONS {
            tracing::debug!(
                "⚠️  {} definitions for '{}' — capping at {}",
                definitions.len(),
                self_symbol,
                MAX_DEFINITIONS
            );
        }
        let definitions: Vec<Symbol> = definitions.into_iter().take(MAX_DEFINITIONS).collect();

        debug!(
            "✅ Found {} definitions and {} references for '{}'",
            definitions.len(),
            references.len(),
            self_symbol
        );

        Ok((definitions, references))
    }

    /// Find references in a target workspace by delegating to the target_workspace module.
    async fn database_find_references_in_target_workspace(
        &self,
        handler: &dyn ToolContext,
        target_workspace_id: String,
    ) -> Result<(Vec<Symbol>, Vec<Relationship>)> {
        target_workspace::find_references_in_target_workspace(
            handler,
            target_workspace_id,
            &self.symbol,
            self.limit,
            self.reference_kind.as_deref(),
        )
        .await
    }
}
