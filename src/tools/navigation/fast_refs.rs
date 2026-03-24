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

use crate::extractors::{Relationship, RelationshipKind, Symbol, SymbolKind};
use crate::handler::JulieServerHandler;
use crate::utils::cross_language_intelligence::generate_naming_variants;
use std::collections::{HashMap, HashSet};

use super::formatting::format_lean_refs_results;
use super::reference_workspace;
use super::resolution::{WorkspaceTarget, parse_qualified_name, resolve_workspace_filter};

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
    /// Workspace filter: "primary" (default) or a reference workspace ID
    #[serde(default = "default_workspace")]
    pub workspace: Option<String>,
    /// Reference kind filter: "call", "variable_ref", "type_usage", "member_access", "import"
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
    /// Skips for reference workspace queries (may lack embeddings).
    async fn try_semantic_fallback(&self, handler: &JulieServerHandler) -> String {
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

        // Use the correct DB: reference workspace DB if specified, primary otherwise
        let is_reference = self.workspace.is_some() && self.workspace.as_deref() != Some("primary");

        // Use a lower threshold than MIN_SIMILARITY_SCORE (0.5) because we're
        // comparing a raw symbol name against rich metadata embeddings (kind +
        // name + signature + docstring). Different input domains = lower scores.
        const QUERY_SIMILARITY_THRESHOLD: f32 = 0.2;

        if is_reference {
            let ref_id = self.workspace.as_deref().unwrap_or("primary");
            debug!("Semantic fallback: reference workspace '{}'", ref_id);
            let db_arc = match handler.get_database_for_workspace(ref_id).await {
                Ok(db) => db,
                Err(e) => {
                    debug!("Semantic fallback: DB error for '{}': {}", ref_id, e);
                    return String::new();
                }
            };
            let db_guard = match db_arc.lock() {
                Ok(guard) => guard,
                Err(_) => return String::new(),
            };
            let similar = match similarity::find_similar_by_query(
                &db_guard,
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
        } else {
            let workspace = match handler.get_workspace().await {
                Ok(Some(w)) => w,
                _ => return String::new(),
            };
            let db = match workspace.db.as_ref() {
                Some(db) => db,
                None => return String::new(),
            };
            let db_guard = match db.lock() {
                Ok(guard) => guard,
                Err(_) => return String::new(),
            };
            let similar = match similarity::find_similar_by_query(
                &db_guard,
                &query_vector,
                5,
                QUERY_SIMILARITY_THRESHOLD,
            ) {
                Ok(results) => results,
                Err(_) => return String::new(),
            };
            format_semantic_fallback(&self.symbol, &similar)
        }
    }

    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!("Finding references for: {}", self.symbol);

        // Resolve workspace target (primary or reference workspace)
        let workspace_target = resolve_workspace_filter(self.workspace.as_deref(), handler).await?;

        // Find references (workspace resolution is handled by workspace_target)
        let (definitions, references) = self
            .find_references_and_definitions(handler, workspace_target.clone())
            .await?;

        if definitions.is_empty() && references.is_empty() {
            // Attempt semantic fallback (works for both primary and reference workspaces)
            let semantic_section = self.try_semantic_fallback(handler).await;

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
            .resolve_source_names(handler, &references, &workspace_target)
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
    /// Routes to the correct workspace DB: reference workspaces use
    /// `get_database_for_workspace`; primary uses `get_workspace().db`.
    async fn resolve_source_names(
        &self,
        handler: &JulieServerHandler,
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

        match workspace_target {
            WorkspaceTarget::Reference(ref_workspace_id) => {
                let db_arc = match handler.get_database_for_workspace(ref_workspace_id).await {
                    Ok(db) => db,
                    Err(_) => return HashMap::new(),
                };
                tokio::task::spawn_blocking(move || {
                    let db_lock = db_arc
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    match db_lock.get_symbols_by_ids(&ids) {
                        Ok(symbols) => symbols
                            .into_iter()
                            .map(|s| (s.id.clone(), s.name.clone()))
                            .collect(),
                        Err(_) => HashMap::new(),
                    }
                })
                .await
                .unwrap_or_default()
            }
            WorkspaceTarget::Primary => {
                if let Ok(Some(workspace)) = handler.get_workspace().await {
                    if let Some(db) = workspace.db.as_ref() {
                        let db_arc = db.clone();
                        tokio::task::spawn_blocking(move || {
                            let db_lock =
                                super::lock_db(&db_arc, "fast_refs source name resolution");
                            match db_lock.get_symbols_by_ids(&ids) {
                                Ok(symbols) => symbols
                                    .into_iter()
                                    .map(|s| (s.id.clone(), s.name.clone()))
                                    .collect(),
                                Err(_) => HashMap::new(),
                            }
                        })
                        .await
                        .unwrap_or_default()
                    } else {
                        HashMap::new()
                    }
                } else {
                    HashMap::new()
                }
            }
        }
    }

    pub async fn find_references_and_definitions(
        &self,
        handler: &JulieServerHandler,
        workspace_target: WorkspaceTarget,
    ) -> Result<(Vec<Symbol>, Vec<Relationship>)> {
        debug!(
            "Searching for references to '{}' using indexed search",
            self.symbol
        );

        match workspace_target {
            WorkspaceTarget::Reference(ref_workspace_id) => {
                debug!("Searching reference workspace: {}", ref_workspace_id);
                return self
                    .database_find_references_in_reference(handler, ref_workspace_id)
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

        // Primary workspace search - use handler.get_workspace().db
        // Strategy 1: Use SQLite for O(log n) indexed name lookup
        let mut definitions = Vec::new();

        // Use SQLite for exact name lookup (indexed)
        if let Some(workspace) = handler.get_workspace().await? {
            if let Some(db) = workspace.db.as_ref() {
                // spawn_blocking to avoid blocking tokio runtime during DB I/O
                let symbol = effective_symbol.clone();
                let parent_filter_clone = parent_filter.clone();
                let db_arc = db.clone();

                definitions =
                    tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<Symbol>> {
                        let db_lock = super::lock_db(&db_arc, "fast_refs exact lookup");
                        let mut defs = db_lock.get_symbols_by_name(&symbol)?;

                        // If a parent filter is specified, filter definitions to those
                        // whose parent symbol has the matching name
                        if let Some(ref parent_name) = parent_filter_clone {
                            let parent_ids: Vec<String> = defs
                                .iter()
                                .filter_map(|s| s.parent_id.clone())
                                .collect::<std::collections::HashSet<_>>()
                                .into_iter()
                                .collect();

                            if !parent_ids.is_empty() {
                                let parents = db_lock.get_symbols_by_ids(&parent_ids)?;
                                let matching_parent_ids: std::collections::HashSet<String> =
                                    parents
                                        .into_iter()
                                        .filter(|p| p.name == *parent_name)
                                        .map(|p| p.id)
                                        .collect();

                                defs.retain(|s| {
                                    s.parent_id
                                        .as_deref()
                                        .map(|pid| matching_parent_ids.contains(pid))
                                        .unwrap_or(false)
                                });
                            } else {
                                // No definitions have parent_id — qualified search finds nothing
                                defs.clear();
                            }
                        }

                        Ok(defs)
                    })
                    .await
                    .map_err(|e| anyhow::anyhow!("spawn_blocking join error: {}", e))??;

                debug!("⚡ SQLite found {} exact matches", definitions.len());
            }
        }

        // ✨ INTELLIGENCE: Cross-language naming convention matching
        // Use our shared utility to generate variants (snake_case, camelCase, PascalCase)
        let variants = generate_naming_variants(&effective_symbol);
        debug!("🔍 Cross-language search variants: {:?}", variants);

        if let Ok(Some(workspace)) = handler.get_workspace().await {
            if let Some(db) = workspace.db.as_ref() {
                // spawn_blocking to avoid blocking tokio runtime during DB I/O
                let symbol = effective_symbol.clone();
                let db_arc = db.clone();

                let variant_matches = tokio::task::spawn_blocking(move || {
                    let db_lock = super::lock_db(&db_arc, "fast_refs variant lookup");
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

        // Separate imports from true definitions.
        // Imports (use/require/include) are REFERENCES to a symbol, not definitions of it.
        // An agent searching for "CodeTokenizer" wants to see struct definition separate from
        // the 6 files that import it.
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
                false // Remove from definitions
            } else {
                true // Keep as definition
            }
        });

        // Strategy 2: Find direct relationships - REFERENCES TO this symbol (not FROM it)
        // PERFORMANCE FIX: Use targeted queries instead of loading ALL relationships
        // This changes from O(n) linear scan to O(k * log n) indexed queries where k = definitions.len()
        //
        // Filter synthetic import refs if reference_kind is set and isn't "import"
        let mut references: Vec<Relationship> = match &self.reference_kind {
            Some(kind) if kind != "import" => Vec::new(),
            _ => import_refs,
        };

        if let Ok(Some(workspace)) = handler.get_workspace().await {
            if let Some(db) = workspace.db.as_ref() {
                // spawn_blocking to avoid blocking tokio runtime during DB I/O
                // Collect definition IDs before moving into spawn_blocking
                let definition_ids: Vec<String> =
                    definitions.iter().map(|d| d.id.clone()).collect();
                let db_arc = db.clone();

                let reference_kind_filter = self.reference_kind.clone();
                let symbol_references = tokio::task::spawn_blocking(move || {
                    let db_lock = super::lock_db(&db_arc, "fast_refs relationships");
                    // Single batch query, optionally filtered by identifier kind
                    if let Some(kind) = reference_kind_filter {
                        db_lock
                            .get_relationships_to_symbols_filtered_by_kind(&definition_ids, &kind)
                    } else {
                        db_lock.get_relationships_to_symbols(&definition_ids)
                    }
                })
                .await
                .map_err(|e| anyhow::anyhow!("spawn_blocking join error: {}", e))?;

                if let Ok(refs) = symbol_references {
                    references.extend(refs);
                }
            }
        }

        // Strategy 3: Identifier-based reference discovery
        // The identifiers table stores every usage site extracted by all 31 language extractors.
        // This catches references that relationships miss (struct type usages, function calls
        // without extracted relationships, member accesses, etc.)
        if let Ok(Some(workspace)) = handler.get_workspace().await {
            if let Some(db) = workspace.db.as_ref() {
                let db_arc = db.clone();
                let symbol = effective_symbol.clone();
                let reference_kind_for_ident = self.reference_kind.clone();

                // Collect all name variants for batch query
                let mut all_names = vec![symbol.clone()];
                let variants = generate_naming_variants(&symbol);
                for v in variants {
                    if v != symbol {
                        all_names.push(v);
                    }
                }

                // First definition ID for to_symbol_id in converted Relationships
                let first_def_id = definitions
                    .first()
                    .map(|d| d.id.clone())
                    .unwrap_or_default();

                let identifier_refs = tokio::task::spawn_blocking(move || {
                    let db_lock = super::lock_db(&db_arc, "fast_refs identifiers");
                    if let Some(kind) = reference_kind_for_ident {
                        db_lock.get_identifiers_by_names_and_kind(&all_names, &kind)
                    } else {
                        db_lock.get_identifiers_by_names(&all_names)
                    }
                })
                .await
                .map_err(|e| anyhow::anyhow!("spawn_blocking join error: {}", e))?;

                if let Ok(ident_refs) = identifier_refs {
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
                    for ident in ident_refs {
                        let key = (ident.file_path.clone(), ident.start_line);
                        if existing_refs.contains(&key) {
                            continue; // Prefer existing relationship (richer data)
                        }

                        // Convert IdentifierKind string to RelationshipKind
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
                        added += 1;
                    }

                    debug!(
                        "🔓 Identifiers added {} new references (deduped from existing relationships)",
                        added
                    );
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

        // Apply user-specified limit to prevent massive responses
        // Truncate AFTER sorting to return the top N most relevant references
        references.truncate(self.limit as usize);

        // Cap definitions — a symbol should rarely have more than a handful of
        // definition sites (one per language variant or overload). Large counts
        // signal cross-language naming collisions; cap to keep output usable.
        const MAX_DEFINITIONS: usize = 50;
        if definitions.len() > MAX_DEFINITIONS {
            tracing::debug!(
                "⚠️  {} definitions for '{}' — capping at {}",
                definitions.len(),
                self.symbol,
                MAX_DEFINITIONS
            );
        }
        let definitions: Vec<Symbol> = definitions.into_iter().take(MAX_DEFINITIONS).collect();

        debug!(
            "✅ Found {} definitions and {} references for '{}'",
            definitions.len(),
            references.len(),
            self.symbol
        );

        Ok((definitions, references))
    }

    /// Find references in a reference workspace by delegating to the reference_workspace module
    async fn database_find_references_in_reference(
        &self,
        handler: &JulieServerHandler,
        ref_workspace_id: String,
    ) -> Result<(Vec<Symbol>, Vec<Relationship>)> {
        reference_workspace::find_references_in_reference_workspace(
            handler,
            ref_workspace_id,
            &self.symbol,
            self.limit,
            self.reference_kind.as_deref(),
        )
        .await
    }
}
