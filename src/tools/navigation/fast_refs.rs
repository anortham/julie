//! FastRefsTool - Find all references to a symbol
//!
//! This tool finds all usages and references across the codebase using:
//! 1. SQLite symbols table for O(log n) exact name matching
//! 2. Cross-language naming convention variants (snake_case, camelCase, etc.)
//! 3. Relationships table for caller→callee connections
//! 4. Identifiers table for usage sites (calls, type usages, member access, imports)

use anyhow::Result;
use schemars::JsonSchema;
use crate::mcp_compat::{CallToolResult, Content, CallToolResultExt};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::extractors::{Relationship, RelationshipKind, Symbol, SymbolKind};
use crate::handler::JulieServerHandler;
use crate::utils::cross_language_intelligence::generate_naming_variants;
use std::collections::{HashMap, HashSet};

use super::formatting::format_lean_refs_results;
use super::reference_workspace;
use super::resolution::resolve_workspace_filter;

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
    #[serde(default = "default_true")]
    pub include_definition: bool,
    /// Maximum references (default: 10, range: 1-500)
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// Workspace filter: "primary" (default) or workspace ID
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
    ) -> Result<CallToolResult> {
        let lean_output = format_lean_refs_results(&self.symbol, &definitions, &references);
        Ok(CallToolResult::text_content(vec![Content::text(lean_output)]))
    }

    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!("🔗 Finding references for: {}", self.symbol);

        // Find references (workspace resolution happens in find_references_and_definitions)
        let (definitions, references) = self.find_references_and_definitions(handler).await?;

        if definitions.is_empty() && references.is_empty() {
            return self.create_result(vec![], vec![]);
        }

        // Respect include_definition parameter
        let defs = if self.include_definition {
            definitions
        } else {
            vec![]
        };

        self.create_result(defs, references)
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
        let workspace_filter = resolve_workspace_filter(self.workspace.as_deref(), handler).await?;

        // If reference workspace is specified, open that workspace's DB and search it
        if let Some(ref_workspace_id) = workspace_filter {
            debug!("🎯 Searching reference workspace: {}", ref_workspace_id);
            return self
                .database_find_references_in_reference(handler, ref_workspace_id)
                .await;
        }

        // Primary workspace search - use handler.get_workspace().db
        // Strategy 1: Use SQLite for O(log n) indexed name lookup
        let mut definitions = Vec::new();

        // Use SQLite for exact name lookup (indexed)
        if let Some(workspace) = handler.get_workspace().await? {
            if let Some(db) = workspace.db.as_ref() {
                // spawn_blocking to avoid blocking tokio runtime during DB I/O
                let symbol = self.symbol.clone();
                let db_arc = db.clone();

                definitions = tokio::task::spawn_blocking(move || {
                    let db_lock = super::lock_db(&db_arc, "fast_refs exact lookup");
                    db_lock.get_symbols_by_name(&symbol)
                })
                .await
                .map_err(|e| anyhow::anyhow!("spawn_blocking join error: {}", e))??;

                debug!("⚡ SQLite found {} exact matches", definitions.len());
            }
        }

        // ✨ INTELLIGENCE: Cross-language naming convention matching
        // Use our shared utility to generate variants (snake_case, camelCase, PascalCase)
        let variants = generate_naming_variants(&self.symbol);
        debug!("🔍 Cross-language search variants: {:?}", variants);

        if let Ok(Some(workspace)) = handler.get_workspace().await {
            if let Some(db) = workspace.db.as_ref() {
                // spawn_blocking to avoid blocking tokio runtime during DB I/O
                let symbol = self.symbol.clone();
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
        let mut references: Vec<Relationship> = import_refs;

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
                        db_lock.get_relationships_to_symbols_filtered_by_kind(&definition_ids, &kind)
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
                let symbol = self.symbol.clone();
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
                let first_def_id = definitions.first().map(|d| d.id.clone()).unwrap_or_default();

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

        debug!(
            "✅ Found {} definitions and {} references for '{}'",
            definitions.len(),
            references.len(),
            self.symbol
        );

        Ok((definitions, references))
    }

    /// Format lean text summary for AI agents
    pub fn format_optimized_results(
        &self,
        symbols: &[Symbol],
        relationships: &[Relationship],
    ) -> String {
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
        )
        .await
    }
}
