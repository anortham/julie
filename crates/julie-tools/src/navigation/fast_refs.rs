//! FastRefsTool - Find all references to a symbol
//!
//! Walks the snapshot graph:
//! 1. Definitions by exact name, then cross-language naming variants
//! 2. Every symbol with an edge into a definition
//! 3. The identifier and relationship rows of that symbol that name the target,
//!    one reference per source line

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use julie_context::ToolContext;
use julie_core::Symbol;
use julie_core::cross_language_intelligence::generate_naming_variants;
use julie_core::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use julie_extractors::{Relationship, RelationshipKind, SymbolKind};
use julie_index::graph::{EdgeKind, Graph, SymbolId};
use julie_index::snapshot::Snapshot;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::formatting::format_lean_refs_results;
use super::resolution::{WorkspaceTarget, has_parent_named, parse_qualified_name, to_symbol};
use super::sites::{Site, identifier_kind_name, reference_sites};

const MAX_DEFINITIONS: usize = 50;

fn default_true() -> bool {
    true
}

fn default_limit() -> u32 {
    10
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
        deserialize_with = "julie_core::serde_lenient::deserialize_bool_lenient"
    )]
    pub include_definition: bool,
    /// Maximum references (default: 10, range: 1-500)
    #[serde(
        default = "default_limit",
        deserialize_with = "julie_core::serde_lenient::deserialize_u32_lenient"
    )]
    pub limit: u32,
    /// Skip this many references before keeping `limit` rows (default: 0)
    #[serde(
        default,
        deserialize_with = "julie_core::serde_lenient::deserialize_u32_lenient"
    )]
    pub offset: u32,
    /// Required workspace ID or absolute workspace path for MCP calls
    #[serde(default = "default_workspace")]
    pub workspace: Option<String>,
    /// Narrow by reference kind: "call", "variable_ref", "type_usage", "member_access", "import". Omit to see all reference types
    #[serde(default)]
    pub reference_kind: Option<String>,
    /// Optional semantic mode override (Auto, Off, Required)
    #[serde(default)]
    pub semantics: Option<julie_core::embeddings_contract::SemanticMode>,
}

/// Definitions and references of one symbol, plus the referencing symbols' names.
#[derive(Debug, Default)]
pub struct FoundReferences {
    pub definitions: Vec<Symbol>,
    pub references: Vec<Relationship>,
    /// `from_symbol_id` -> name, for the reference listing.
    pub source_names: HashMap<String, String>,
}

impl FastRefsTool {
    pub async fn call_tool(&self, handler: &dyn ToolContext) -> Result<CallToolResult> {
        let workspace_target = handler
            .resolve_workspace_target(self.workspace.as_deref())
            .await?;
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
        self.call_tool_with_target_and_budget(handler, workspace_target, None)
            .await
    }

    pub async fn call_tool_with_target_and_budget(
        &self,
        handler: &dyn ToolContext,
        workspace_target: &WorkspaceTarget,
        budget: Option<julie_core::embeddings_contract::EmbeddingRequestBudget>,
    ) -> Result<CallToolResult> {
        self.call_tool_counted(handler, workspace_target, budget)
            .await
            .map(|(result, _)| result)
    }

    pub async fn call_tool_counted(
        &self,
        handler: &dyn ToolContext,
        workspace_target: &WorkspaceTarget,
        budget: Option<julie_core::embeddings_contract::EmbeddingRequestBudget>,
    ) -> Result<(CallToolResult, u32)> {
        debug!("Finding references for: {}", self.symbol);

        let snapshot = handler.snapshot(workspace_target).await?;
        let fetch_limit = self.limit.saturating_add(self.offset).saturating_add(1);
        let mut found = find_references(
            &snapshot,
            &self.symbol,
            fetch_limit,
            self.reference_kind.as_deref(),
        );

        if found.definitions.is_empty() && found.references.is_empty() {
            let semantic_mode = self
                .semantics
                .unwrap_or(julie_core::embeddings_contract::SemanticMode::Auto);
            let semantic_section = super::fast_refs_semantic::try_semantic_fallback(
                &self.symbol,
                handler,
                &snapshot,
                budget,
                semantic_mode,
            )
            .await?;

            let mut result_text = format_lean_refs_results(&self.symbol, &[], &[], &HashMap::new());
            result_text.push_str(&semantic_section);
            let mut result = CallToolResult::text_content(vec![Content::text(result_text)]);
            result.structured_content = Some(serde_json::json!({
                "symbol": self.symbol,
                "definitions": [],
                "references": [],
                "offset": self.offset,
                "limit": self.limit.max(1),
                "has_more": false,
                "next_offset": null,
            }));
            return Ok((result, 0));
        }

        let definitions = if self.include_definition {
            found.definitions
        } else {
            Vec::new()
        };
        let offset = self.offset as usize;
        let page_limit = self.limit.max(1) as usize;
        let more = found.references.len() > offset + page_limit;
        found.references = found
            .references
            .into_iter()
            .skip(offset)
            .take(page_limit)
            .collect();
        let kept = found.references.len();
        let count = kept as u32;
        let mut lean_output = format_lean_refs_results(
            &self.symbol,
            &definitions,
            &found.references,
            &found.source_names,
        );
        if more {
            if !lean_output.ends_with('\n') {
                lean_output.push('\n');
            }
            lean_output.push_str(&crate::shared::next_line(
                "fast_refs",
                &[("symbol", &self.symbol)],
                offset + kept,
            ));
        }
        let mut result = CallToolResult::text_content(vec![Content::text(lean_output)]);
        result.structured_content = Some(serde_json::json!({
            "symbol": self.symbol,
            "definitions": definitions,
            "references": found.references,
            "offset": self.offset,
            "limit": page_limit,
            "has_more": more,
            "next_offset": more.then_some(offset + kept),
        }));
        Ok((result, count))
    }

    /// Definitions and references for callers that edit every site.
    pub async fn find_references_and_definitions(
        &self,
        handler: &dyn ToolContext,
        workspace_target: WorkspaceTarget,
    ) -> Result<(Vec<Symbol>, Vec<Relationship>)> {
        let snapshot = handler.snapshot(&workspace_target).await?;
        let found = find_references(
            &snapshot,
            &self.symbol,
            self.limit,
            self.reference_kind.as_deref(),
        );
        Ok((found.definitions, found.references))
    }
}

/// Symbols named `symbol`: exact name, else its naming variants, narrowed to
/// children of the parent when the name is qualified.
fn definition_ids(graph: &Graph, symbol: &str) -> Vec<SymbolId> {
    let (effective, parent) = match parse_qualified_name(symbol) {
        Some((parent, child)) => (child, Some(parent)),
        None => (symbol, None),
    };
    let mut ids = graph.find_by_name(effective).to_vec();
    if ids.is_empty() {
        let variants = generate_naming_variants(effective);
        debug!("Cross-language search variants: {:?}", variants);
        for variant in variants.iter().filter(|v| v.as_str() != effective) {
            ids.extend_from_slice(graph.find_by_name(variant));
        }
    }
    if let Some(parent) = parent {
        ids.retain(|id| has_parent_named(graph, *id, parent));
    }
    ids.sort_unstable();
    ids.dedup();
    ids
}

fn import_reference(graph: &Graph, id: SymbolId) -> Relationship {
    let row = graph.symbol(id);
    Relationship {
        id: format!("import_{}_{}", row.path, row.span.start_line),
        from_symbol_id: row.id.clone(),
        to_symbol_id: String::new(),
        kind: RelationshipKind::Imports,
        file_path: row.path.clone(),
        line_number: row.span.start_line,
        span: None,
        reference_site_is_exact: false,
        confidence: 1.0,
        metadata: None,
    }
}

fn edge_relationship_kind(kind: EdgeKind) -> RelationshipKind {
    match kind {
        EdgeKind::Calls => RelationshipKind::Calls,
        EdgeKind::Imports => RelationshipKind::Imports,
        EdgeKind::Implements => RelationshipKind::Implements,
        EdgeKind::Extends => RelationshipKind::Extends,
        EdgeKind::References | EdgeKind::Contains | EdgeKind::WebRoute | EdgeKind::SqlQuery => {
            RelationshipKind::References
        }
    }
}

/// Symbols with a reference edge into `to`, once each, with the first edge kind.
fn referencing_symbols(graph: &Graph, to: SymbolId) -> Vec<(SymbolId, EdgeKind)> {
    let mut sources: Vec<(SymbolId, EdgeKind)> = graph
        .incoming(to)
        .iter()
        .filter(|(_, kind)| {
            !matches!(
                kind,
                EdgeKind::Contains | EdgeKind::WebRoute | EdgeKind::SqlQuery
            )
        })
        .copied()
        .collect();
    sources.dedup_by_key(|(from, _)| *from);
    sources
}

/// Every definition named `symbol` and the sites that reference it, sorted by
/// confidence then location and cut to `limit`. Import symbols become import
/// references. `reference_kind` keeps only sites whose identifier has that kind
/// (`import` keeps only the import references).
pub fn find_references(
    snapshot: &Snapshot,
    symbol: &str,
    limit: u32,
    reference_kind: Option<&str>,
) -> FoundReferences {
    let graph = snapshot.graph();
    let mut found = FoundReferences::default();

    let mut definitions = Vec::new();
    for id in definition_ids(graph, symbol) {
        if graph.symbol(id).kind == SymbolKind::Import {
            if reference_kind.is_none_or(|kind| kind == "import") {
                found.references.push(import_reference(graph, id));
            }
        } else {
            definitions.push(id);
        }
    }

    let mut seen: HashSet<(String, String)> = found
        .references
        .iter()
        .map(|reference| (reference.file_path.clone(), reference.id.clone()))
        .collect();

    for &to in &definitions {
        let to_row = graph.symbol(to);
        for (from, edge) in referencing_symbols(graph, to) {
            let from_row = graph.symbol(from);
            let mut sites = reference_sites(graph, from, to);
            if sites.is_empty() {
                sites.push(Site {
                    id: format!("graph_{}_{}", from_row.id, to_row.id),
                    line: from_row.span.start_line,
                    span: None,
                    exact: false,
                    kind: edge_relationship_kind(edge),
                    identifier_kind: None,
                    confidence: 1.0,
                    metadata: Some(HashMap::from([(
                        "reference_site_provenance".into(),
                        serde_json::json!("graph"),
                    )])),
                });
            }
            for site in sites {
                let site_kind = site.identifier_kind.as_ref().map(identifier_kind_name);
                if reference_kind.is_some_and(|kind| site_kind != Some(kind)) {
                    continue;
                }
                if !seen.insert((from_row.path.clone(), site.id.clone())) {
                    continue;
                }
                found.references.push(Relationship {
                    id: site.id,
                    from_symbol_id: from_row.id.clone(),
                    to_symbol_id: to_row.id.clone(),
                    kind: site.kind,
                    file_path: from_row.path.clone(),
                    line_number: site.line,
                    span: site.span,
                    reference_site_is_exact: site.exact,
                    confidence: site.confidence,
                    metadata: site.metadata,
                });
                found
                    .source_names
                    .insert(from_row.id.clone(), from_row.name.clone());
            }
        }
    }

    found.references.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.file_path.cmp(&b.file_path))
            .then_with(|| a.line_number.cmp(&b.line_number))
            .then_with(|| {
                a.span
                    .as_ref()
                    .map(|span| (span.start_byte, span.end_byte))
                    .cmp(&b.span.as_ref().map(|span| (span.start_byte, span.end_byte)))
            })
            .then_with(|| a.id.cmp(&b.id))
    });
    found.references.truncate(limit as usize);

    if definitions.len() > MAX_DEFINITIONS {
        debug!(
            "⚠️  {} definitions for '{}' — capping at {}",
            definitions.len(),
            symbol,
            MAX_DEFINITIONS
        );
    }
    found.definitions = definitions
        .iter()
        .take(MAX_DEFINITIONS)
        .map(|id| to_symbol(graph, *id))
        .collect();

    debug!(
        "✅ Found {} definitions and {} references for '{}'",
        found.definitions.len(),
        found.references.len(),
        symbol
    );
    found
}
