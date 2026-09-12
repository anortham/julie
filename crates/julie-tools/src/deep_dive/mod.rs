//! Deep dive tool — progressive-depth, kind-aware symbol context
//!
//! Given a symbol, returns everything an agent needs to understand it in a single call.
//! Replaces the common 3-4 tool chain of fast_search → get_symbols → fast_refs → Read.

pub mod data;
pub mod formatting;

use anyhow::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::debug;

use julie_context::ToolContext;
use julie_core::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use julie_extractors::SymbolKind;
use julie_index::graph::{Graph, SymbolId};
use julie_index::snapshot::Snapshot;

const DISAMBIGUATION_THRESHOLD: usize = 5;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DeepDiveDepth {
    Overview,
    Context,
    Full,
}

impl Default for DeepDiveDepth {
    fn default() -> Self {
        Self::Overview
    }
}

impl DeepDiveDepth {
    fn as_str(self) -> &'static str {
        match self {
            Self::Overview => "overview",
            Self::Context => "context",
            Self::Full => "full",
        }
    }
}

fn default_workspace() -> Option<String> {
    Some("primary".to_string())
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
/// Investigate a symbol with progressive depth. Returns definition, references, children,
/// and type info in a single call — tailored to the symbol's kind.
///
/// **Always use BEFORE modifying or extending a symbol.** Replaces the common chain of
/// fast_search → get_symbols → fast_refs → Read with a single call.
pub struct DeepDiveTool {
    /// Symbol name to investigate (supports qualified names like `Processor::process`)
    #[serde(alias = "symbol_name")]
    pub symbol: String,

    /// Investigation depth: "overview" (default, ~200 tokens: signature + caller/callee list), "context" (~600 tokens: adds code body), "full" (~1500 tokens: all refs, test locations, bodies). Use overview for orientation, context when you need implementation details, full for complete investigation
    #[serde(default)]
    pub depth: DeepDiveDepth,

    /// Disambiguate when multiple symbols share a name (partial file path match)
    #[serde(default)]
    pub context_file: Option<String>,

    /// Required workspace ID or absolute workspace path for MCP calls
    #[serde(default = "default_workspace")]
    pub workspace: Option<String>,

    /// Optional semantic mode override (e.g. Off, Required, Auto)
    #[serde(default)]
    pub semantics: Option<julie_core::embeddings_contract::SemanticMode>,
    /// Zero-based source-line offset within the canonical symbol span.
    #[serde(default)]
    pub body_offset: u32,
    /// Maximum source lines returned for the selected body.
    #[serde(default)]
    pub body_limit: Option<u32>,
    /// Source hash from the previous body page; rejects changed source.
    #[serde(default)]
    pub source_hash: Option<String>,
}

/// Reference caps by depth level
fn ref_caps(depth: &str) -> (usize, usize) {
    match depth {
        "context" => (15, 15),
        "full" => (50, 50),
        _ => (10, 10), // overview
    }
}

impl DeepDiveTool {
    pub async fn call_tool(&self, handler: &dyn ToolContext) -> Result<CallToolResult> {
        self.call_tool_counted(handler)
            .await
            .map(|(result, _)| result)
    }

    pub async fn call_tool_counted(
        &self,
        handler: &dyn ToolContext,
    ) -> Result<(CallToolResult, u32)> {
        let depth = self.depth.as_str();
        debug!("Deep dive: {} (depth: {})", self.symbol, depth);

        let workspace_target = handler
            .resolve_workspace_target(self.workspace.as_deref())
            .await?;
        let snapshot = handler.snapshot(&workspace_target).await?;
        let (incoming_cap, outgoing_cap) = ref_caps(depth);
        let graph = snapshot.graph();
        let exact_path_id = self.context_file.as_deref().and_then(|path| {
            graph
                .symbols_in_path(path)
                .iter()
                .copied()
                .find(|id| graph.symbol(*id).id == self.symbol)
        });
        let exact_id = exact_path_id.or_else(|| {
            graph.symbol_by_row_id(&self.symbol).filter(|id| {
                data::find_symbol(graph, &graph.symbol(*id).name, self.context_file.as_deref())
                    .contains(id)
            })
        });
        let selected = exact_id.map_or_else(
            || data::find_symbol(graph, &self.symbol, self.context_file.as_deref()),
            |id| vec![id],
        );
        let body_selected = if exact_id.is_some() || selected.len() <= DISAMBIGUATION_THRESHOLD {
            selected.clone()
        } else {
            auto_select_same_file_overload(graph, &selected)
                .into_iter()
                .collect()
        };
        let query_symbol = exact_id
            .map(|id| graph.symbol(id).name.as_str())
            .unwrap_or(&self.symbol);
        let query_file = exact_id
            .map(|id| graph.symbol(id).path.as_str())
            .or(self.context_file.as_deref());
        let resolved = !selected.is_empty();
        if !resolved && self.source_hash.is_some() {
            anyhow::bail!(
                "source changed since the previous body page; refresh and restart with body_offset=0"
            );
        }

        let explicit_body_window =
            self.body_limit.is_some() || self.body_offset > 0 || self.source_hash.is_some();
        let body_requested = explicit_body_window || depth != "overview";
        let mut body_pages = Vec::new();
        if body_requested {
            let default_limit = if depth == "context" { 30 } else { 100 };
            let body_limit = self.body_limit.unwrap_or(default_limit).max(1);
            let workspace = crate::shared::resolved_workspace(handler, &workspace_target)?;
            for id in &body_selected {
                let symbol = crate::snapshot_rows::to_symbol(graph, *id);
                let mut page = crate::symbols::body_extraction::body_page(
                    &snapshot,
                    &symbol,
                    self.body_offset,
                    body_limit,
                    self.source_hash.as_deref(),
                )?;
                if !page.end_reached {
                    let mut next = self.clone();
                    next.symbol = symbol.id.clone();
                    next.context_file = Some(symbol.file_path.clone());
                    next.workspace = Some(workspace.clone());
                    next.body_limit = Some(body_limit);
                    next.source_hash = Some(page.source_hash.clone());
                    page.continuation = Some(crate::shared::request_line(
                        "deep_dive",
                        &next,
                        "body_offset",
                        self.body_offset as usize + body_limit as usize,
                    ));
                }
                body_pages.push(page);
            }
        }

        let mut text = if explicit_body_window && !body_selected.is_empty() {
            let allow_similarity =
                self.semantics != Some(julie_core::embeddings_contract::SemanticMode::Off);
            let mut sections = Vec::new();
            for id in &body_selected {
                let mut context = data::build_symbol_context_with_semantics(
                    &snapshot,
                    *id,
                    depth,
                    incoming_cap,
                    outgoing_cap,
                    allow_similarity,
                )?;
                context.symbol.code_context = None;
                sections.push(formatting::format_symbol_context(&context, depth));
            }
            sections.join("\n\n---\n\n")
        } else if let Some(id) = exact_id {
            let allow_similarity =
                self.semantics != Some(julie_core::embeddings_contract::SemanticMode::Off);
            let context = data::build_symbol_context_with_semantics(
                &snapshot,
                id,
                depth,
                incoming_cap,
                outgoing_cap,
                allow_similarity,
            )?;
            formatting::format_symbol_context(&context, depth)
        } else {
            deep_dive_query_with_semantics(
                &snapshot,
                query_symbol,
                query_file,
                depth,
                incoming_cap,
                outgoing_cap,
                self.semantics,
            )?
        };

        if body_requested {
            if !body_pages.is_empty() {
                text.push_str(
                    "\n\nBody page status (exact text is in structured_content.body_pages):\n",
                );
                for page in &body_pages {
                    if explicit_body_window {
                        text.push_str(if page.canonical_span_exact {
                            "Body page (exact):\n"
                        } else {
                            "Body page (inferred):\n"
                        });
                        text.push_str(&page.text);
                        if !page.text.ends_with('\n') {
                            text.push('\n');
                        }
                    }
                    let page_location = if explicit_body_window {
                        "above-and-structuredContent"
                    } else {
                        "structuredContent.body_pages"
                    };
                    text.push_str(&format!(
                        "body {} lines [{}, {})/{}; status={}; complete={}; end_reached={}; page_text={}",
                        page.symbol,
                        page.returned_range[0],
                        page.returned_range[1],
                        page.total_lines,
                        page.status,
                        page.complete,
                        page.end_reached,
                        page_location
                    ));
                    if let Some(next) = &page.continuation {
                        text.push('\n');
                        text.push_str(next);
                    }
                    text.push('\n');
                }
                text = text.trim_end().to_string();
            }
        }

        let mut result = CallToolResult::text_content(vec![Content::text(text)]);
        result.structured_content = Some(serde_json::json!({
            "symbol": query_symbol,
            "body_pages": body_pages,
        }));

        Ok((result, if resolved { 1 } else { 0 }))
    }
}

/// Shared query logic for both primary and target-workspace deep dives
pub fn deep_dive_query(
    snapshot: &Snapshot,
    symbol_name: &str,
    context_file: Option<&str>,
    depth: &str,
    incoming_cap: usize,
    outgoing_cap: usize,
) -> Result<String> {
    deep_dive_query_with_semantics(
        snapshot,
        symbol_name,
        context_file,
        depth,
        incoming_cap,
        outgoing_cap,
        None,
    )
}

/// Shared query logic supporting semantic mode gating
pub fn deep_dive_query_with_semantics(
    snapshot: &Snapshot,
    symbol_name: &str,
    context_file: Option<&str>,
    depth: &str,
    incoming_cap: usize,
    outgoing_cap: usize,
    semantics: Option<julie_core::embeddings_contract::SemanticMode>,
) -> Result<String> {
    let graph = snapshot.graph();
    let symbols = data::find_symbol(graph, symbol_name, context_file);

    if symbols.is_empty() {
        return Ok(format!(
            "No symbol found: '{}'\nTry fast_search(query=\"{}\") for fuzzy matching.",
            symbol_name, symbol_name
        ));
    }

    let mut output = String::new();

    let allow_similarity = semantics != Some(julie_core::embeddings_contract::SemanticMode::Off);
    if symbols.len() > DISAMBIGUATION_THRESHOLD {
        if let Some(selected) = auto_select_same_file_overload(graph, &symbols) {
            let ctx = data::build_symbol_context_with_semantics(
                snapshot,
                selected,
                depth,
                incoming_cap,
                outgoing_cap,
                allow_similarity,
            )?;
            let row = graph.symbol(selected);
            let kind = format!("{:?}", row.kind).to_lowercase();
            output.push_str(&format!(
                "Auto-selected {} from {} definitions in {} (prefer class/struct over overloads)\n\n",
                kind,
                symbols.len(),
                row.path,
            ));
            output.push_str(&formatting::format_symbol_context(&ctx, depth));
            return Ok(output);
        }

        output.push_str(&format!(
            "Found {} definitions of '{}'. Use context_file to disambiguate.\n\n",
            symbols.len(),
            symbol_name
        ));
        for id in &symbols {
            let row = graph.symbol(*id);
            let kind = format!("{:?}", row.kind).to_lowercase();
            let vis = format!("{:?}", row.visibility).to_lowercase();
            output.push_str(&format!(
                "  {}:{} ({}, {})\n",
                row.path, row.span.start_line, kind, vis
            ));
        }
        return Ok(output);
    }

    if symbols.len() > 1 {
        output.push_str(&format!(
            "Found {} definitions of '{}'. Use context_file to disambiguate.\n\n",
            symbols.len(),
            symbol_name
        ));
    }

    for id in &symbols {
        let ctx = data::build_symbol_context_with_semantics(
            snapshot,
            *id,
            depth,
            incoming_cap,
            outgoing_cap,
            allow_similarity,
        )?;
        output.push_str(&formatting::format_symbol_context(&ctx, depth));

        if symbols.len() > 1 {
            output.push_str("\n---\n\n");
        }
    }

    Ok(output)
}

/// When the disambiguation threshold is exceeded and all results are in the same file,
/// auto-select the best match instead of asking for disambiguation.
///
/// This handles the C++ pattern where searching for "basic_json" finds the class AND
/// its 10 constructors all in the same header — the class definition is clearly the
/// intended target.
///
/// Selection priority:
/// 1. Class/Struct/Interface definition (the type itself, not constructors/methods)
/// 2. Highest centrality (reference_score) among remaining matches
/// 3. First match (fallback)
///
/// Returns None when results span multiple files (real disambiguation needed).
fn auto_select_same_file_overload(graph: &Graph, symbols: &[SymbolId]) -> Option<SymbolId> {
    let first = graph.symbol(*symbols.first()?);
    if symbols
        .iter()
        .any(|id| graph.symbol(*id).path != first.path)
    {
        return None;
    }

    let type_defs: Vec<SymbolId> = symbols
        .iter()
        .copied()
        .filter(|id| {
            matches!(
                graph.symbol(*id).kind,
                SymbolKind::Class | SymbolKind::Struct | SymbolKind::Interface
            )
        })
        .collect();
    let pool = if type_defs.is_empty() {
        symbols
    } else {
        &type_defs
    };
    if pool.len() == 1 {
        return Some(pool[0]);
    }
    pool.iter().copied().max_by(|a, b| {
        graph
            .reference_score(*a)
            .partial_cmp(&graph.reference_score(*b))
            .unwrap_or(std::cmp::Ordering::Equal)
    })
}
