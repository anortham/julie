use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use anyhow::{Result, anyhow};
use julie_context::ToolContext;
use julie_core::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use julie_extractors::RelationshipKind;
use julie_index::graph::{Graph, SymbolId};
use julie_index::snapshot::Snapshot;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::resolution::{file_path_matches_suffix, find_symbols};
use super::sites::reference_sites;

const DEFAULT_MAX_HOPS: u32 = 6;
const MAX_HOPS: u32 = 32;

fn default_max_hops() -> u32 {
    DEFAULT_MAX_HOPS
}

fn default_workspace() -> Option<String> {
    Some("primary".to_string())
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
/// BFS traverses `Calls` edges only (calls and instantiations). Implements,
/// Extends, and plain references are not followed. Set `mode = "web"` to
/// additionally follow derived `http_call` web edges so a frontend client-call
/// traces through to its backend handler.
pub struct CallPathTool {
    /// Source symbol name to start from. Use a qualified name when shared names are ambiguous.
    pub from: String,
    /// Target symbol name to reach. Multiple target matches are allowed and searched together.
    pub to: String,
    /// Maximum relationship hops to traverse. Accepted range: 1 through 32.
    #[schemars(range(min = 1, max = 32))]
    #[serde(
        default = "default_max_hops",
        deserialize_with = "julie_core::serde_lenient::deserialize_u32_lenient"
    )]
    pub max_hops: u32,
    /// Workspace target. Use `primary` or a workspace id opened through `manage_workspace`.
    #[serde(default = "default_workspace")]
    pub workspace: Option<String>,
    /// Optional source file hint used to disambiguate the `from` symbol.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_file_path: Option<String>,
    /// Optional target file hint used to disambiguate the `to` symbol.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_file_path: Option<String>,
    /// Traversal mode. `default` (omitted) follows call edges only. `web`
    /// additionally follows derived `http_call` edges (client-call symbol ->
    /// route handler) and reports external endpoints reached by unmatched
    /// client calls.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
}

impl Default for CallPathTool {
    fn default() -> Self {
        Self {
            from: String::new(),
            to: String::new(),
            max_hops: DEFAULT_MAX_HOPS,
            workspace: default_workspace(),
            from_file_path: None,
            to_file_path: None,
            mode: None,
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CallPathResponse {
    pub found: bool,
    pub hops: u32,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub path: Vec<CallPathHop>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<String>,
    /// External endpoints reached by unmatched client calls during a `web`
    /// mode traversal (e.g. `"GET /api/foo"`). Empty in `default` mode, so
    /// the default response stays byte-identical to the legacy tool.
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub external_endpoints: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CallPathHop {
    pub from: String,
    pub to: String,
    pub edge: String,
    pub file: String,
    #[serde(default)]
    pub target_file: String,
    #[serde(default)]
    pub target_start_line: u32,
}

struct ResolvedEndpoints {
    from: SymbolId,
    targets: HashSet<SymbolId>,
}

/// `to -> (from, edge label)` for every symbol the search reached.
type Predecessors = HashMap<SymbolId, (SymbolId, &'static str)>;

struct PathSearchResult {
    target: Option<SymbolId>,
    predecessors: Predecessors,
}

fn find_matching_symbols(graph: &Graph, name: &str, file_path: Option<&str>) -> Vec<SymbolId> {
    let all_matches = find_symbols(graph, name, None);
    match file_path {
        Some(filter) => all_matches
            .into_iter()
            .filter(|id| file_path_matches_suffix(&graph.symbol(*id).path, filter))
            .collect(),
        None => all_matches,
    }
}

pub fn edge_label(kind: &RelationshipKind) -> &'static str {
    match kind {
        RelationshipKind::Calls => "call",
        RelationshipKind::Instantiates => "construct",
        RelationshipKind::Overrides => "dispatch",
        _ => unreachable!("BFS only traverses Calls, Instantiates, and Overrides"),
    }
}

fn resolve_unique_symbol(
    graph: &Graph,
    name: &str,
    role: &str,
    file_path: Option<&str>,
) -> Result<SymbolId> {
    let matches = find_matching_symbols(graph, name, file_path);
    if matches.is_empty() {
        return Err(anyhow!(
            "Symbol '{}' for '{}' was not found. Use fast_search or deep_dive to verify the name.",
            name,
            role
        ));
    }
    if matches.len() > 1 {
        let locations = matches
            .iter()
            .map(|id| {
                let symbol = graph.symbol(*id);
                format!(
                    "  {} at {}:{}-{}",
                    symbol.name, symbol.path, symbol.span.start_line, symbol.span.end_line
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        return Err(anyhow!(
            "Symbol '{}' for '{}' is ambiguous. Use a qualified name or set '{}_file_path' to disambiguate. Matches:\n{}",
            name,
            role,
            role,
            locations
        ));
    }
    Ok(matches[0])
}

fn resolve_target_ids(
    graph: &Graph,
    name: &str,
    file_path: Option<&str>,
) -> Result<HashSet<SymbolId>> {
    let matches = find_matching_symbols(graph, name, file_path);
    if matches.is_empty() {
        return Err(anyhow!(
            "Symbol '{}' for 'to' was not found. Use fast_search or deep_dive to verify the name.",
            name
        ));
    }
    Ok(matches.into_iter().collect())
}

fn resolve_endpoints(
    graph: &Graph,
    from: &str,
    to: &str,
    from_file_path: Option<&str>,
    to_file_path: Option<&str>,
) -> Result<ResolvedEndpoints> {
    Ok(ResolvedEndpoints {
        from: resolve_unique_symbol(graph, from, "from", from_file_path)?,
        targets: resolve_target_ids(graph, to, to_file_path)?,
    })
}

/// Breadth-first over `expand`, at most `max_hops` levels deep. Neighbours are
/// visited in the order `expand` returns them, so ties follow graph order.
fn bfs_shortest_path(
    start: SymbolId,
    targets: &HashSet<SymbolId>,
    max_hops: u32,
    mut expand: impl FnMut(SymbolId) -> Vec<(SymbolId, &'static str)>,
) -> PathSearchResult {
    let mut predecessors = HashMap::new();
    if targets.contains(&start) {
        return PathSearchResult {
            target: Some(start),
            predecessors,
        };
    }

    let mut visited = HashSet::from([start]);
    let mut frontier = vec![start];
    for _depth in 0..max_hops {
        if frontier.is_empty() {
            break;
        }
        let mut next_frontier = Vec::new();
        for from in frontier {
            for (to, label) in expand(from) {
                if !visited.insert(to) {
                    continue;
                }
                predecessors.insert(to, (from, label));
                if targets.contains(&to) {
                    return PathSearchResult {
                        target: Some(to),
                        predecessors,
                    };
                }
                next_frontier.push(to);
            }
        }
        frontier = next_frontier;
    }

    PathSearchResult {
        target: None,
        predecessors,
    }
}

/// Line in `from` where it calls `to`; the start of `from` when no row names the call.
fn call_site_line(graph: &Graph, from: SymbolId, to: SymbolId) -> u32 {
    let sites = reference_sites(graph, from, to);
    sites
        .iter()
        .find(|site| {
            matches!(
                site.kind,
                RelationshipKind::Calls
                    | RelationshipKind::Instantiates
                    | RelationshipKind::Overrides
            )
        })
        .or(sites.first())
        .map_or(graph.symbol(from).span.start_line, |site| site.line)
}

fn build_hops(
    graph: &Graph,
    start: SymbolId,
    target: SymbolId,
    predecessors: &Predecessors,
    site_line: &dyn Fn(SymbolId, SymbolId, &str) -> u32,
) -> Result<Vec<CallPathHop>> {
    let mut chain = VecDeque::new();
    let mut current = target;
    while current != start {
        let (from, label) = *predecessors.get(&current).ok_or_else(|| {
            anyhow!(
                "Path reconstruction failed at '{}'",
                graph.symbol(current).id
            )
        })?;
        chain.push_front((from, current, label));
        current = from;
    }

    Ok(chain
        .into_iter()
        .map(|(from, to, label)| {
            let (from_symbol, to_symbol) = (graph.symbol(from), graph.symbol(to));
            CallPathHop {
                from: from_symbol.name.clone(),
                to: to_symbol.name.clone(),
                edge: label.to_string(),
                file: format!("{}:{}", from_symbol.path, site_line(from, to, label)),
                target_file: to_symbol.path.clone(),
                target_start_line: to_symbol.span.start_line,
            }
        })
        .collect())
}

fn found_response(path: Vec<CallPathHop>, external_endpoints: Vec<String>) -> CallPathResponse {
    CallPathResponse {
        found: true,
        hops: path.len() as u32,
        path,
        diagnostic: None,
        external_endpoints,
    }
}

fn not_found_response(
    from: &str,
    to: &str,
    max_hops: u32,
    external_endpoints: Vec<String>,
) -> CallPathResponse {
    CallPathResponse {
        found: false,
        hops: 0,
        path: Vec::new(),
        diagnostic: Some(format!(
            "No path found from '{}' to '{}' within {} hops.",
            from, to, max_hops
        )),
        external_endpoints,
    }
}

fn run_call_path(
    graph: &Graph,
    endpoints: &ResolvedEndpoints,
    max_hops: u32,
    from: &str,
    to: &str,
) -> Result<CallPathResponse> {
    if endpoints.targets.contains(&endpoints.from) {
        return Ok(found_response(Vec::new(), Vec::new()));
    }
    let search = bfs_shortest_path(endpoints.from, &endpoints.targets, max_hops, |id| {
        graph.callees(id).map(|callee| (callee, "call")).collect()
    });
    match search.target {
        Some(target) => {
            let hops = build_hops(
                graph,
                endpoints.from,
                target,
                &search.predecessors,
                &|from, to, _| call_site_line(graph, from, to),
            )?;
            Ok(found_response(hops, Vec::new()))
        }
        None => Ok(not_found_response(from, to, max_hops, Vec::new())),
    }
}

#[path = "call_path_web.rs"]
mod call_path_web;
pub use call_path_web::web_call_path_by_name;

impl CallPathTool {
    fn response_result(response: &CallPathResponse) -> Result<CallToolResult> {
        Ok(CallToolResult::text_content(vec![Content::text(
            format_call_path_response(response),
        )]))
    }

    fn diagnostic_response(diagnostic: impl Into<String>) -> CallPathResponse {
        CallPathResponse {
            found: false,
            hops: 0,
            path: Vec::new(),
            diagnostic: Some(diagnostic.into()),
            external_endpoints: Vec::new(),
        }
    }

    async fn resolve_snapshot(&self, handler: &dyn ToolContext) -> Result<Arc<Snapshot>> {
        let target = handler
            .resolve_workspace_target(self.workspace.as_deref())
            .await?;
        handler.snapshot(&target).await
    }

    fn search(&self, snapshot: &Snapshot) -> Result<CallPathResponse> {
        let graph = snapshot.graph();
        let endpoints = resolve_endpoints(
            graph,
            &self.from,
            &self.to,
            self.from_file_path.as_deref(),
            self.to_file_path.as_deref(),
        )?;
        if self.mode.as_deref() == Some("web") {
            return call_path_web::run_web_call_path(
                snapshot,
                &endpoints,
                self.max_hops,
                &self.from,
                &self.to,
            );
        }
        run_call_path(graph, &endpoints, self.max_hops, &self.from, &self.to)
    }

    pub async fn call_tool(&self, handler: &dyn ToolContext) -> Result<CallToolResult> {
        self.call_tool_counted(handler)
            .await
            .map(|(result, _)| result)
    }

    pub async fn call_tool_counted(
        &self,
        handler: &dyn ToolContext,
    ) -> Result<(CallToolResult, u32)> {
        if self.from.is_empty() || self.to.is_empty() {
            return Self::counted_response(&Self::diagnostic_response(
                "both 'from' and 'to' are required",
            ));
        }
        if !(1..=MAX_HOPS).contains(&self.max_hops) {
            return Self::counted_response(&Self::diagnostic_response(format!(
                "max_hops must be in the range 1..={MAX_HOPS}"
            )));
        }
        match self.mode.as_deref() {
            None | Some("default") | Some("web") => {}
            Some(other) => {
                return Self::counted_response(&Self::diagnostic_response(format!(
                    "mode must be 'default' or 'web'; got '{other}'"
                )));
            }
        }

        let snapshot = match self.resolve_snapshot(handler).await {
            Ok(snapshot) => snapshot,
            Err(error) => {
                return Self::counted_response(&Self::diagnostic_response(format!(
                    "Workspace resolution failed: {error}"
                )));
            }
        };

        let response = match self.search(&snapshot) {
            Ok(response) => response,
            Err(error) => Self::diagnostic_response(error.to_string()),
        };

        debug!(
            "call_path {} -> {} found={} hops={}",
            self.from, self.to, response.found, response.hops
        );

        Self::counted_response(&response)
    }

    fn counted_response(response: &CallPathResponse) -> Result<(CallToolResult, u32)> {
        let result = Self::response_result(response)?;
        Ok((result, u32::from(response.found)))
    }
}

fn format_call_path_response(response: &CallPathResponse) -> String {
    let mut out = format!("found={} hops={}", response.found, response.hops);

    if let Some(diagnostic) = &response.diagnostic {
        out.push_str(&format!("\ndiagnostic: {diagnostic}"));
    }

    for (index, hop) in response.path.iter().enumerate() {
        out.push_str(&format!(
            "\n{}. {} --{}--> {} at {}",
            index + 1,
            hop.from,
            hop.edge,
            hop.to,
            hop.file
        ));
        if !hop.target_file.is_empty() {
            out.push_str(&format!(
                " -> {}:{}",
                hop.target_file, hop.target_start_line
            ));
        }
    }

    for endpoint in &response.external_endpoints {
        out.push_str(&format!("\nexternal_endpoint: {endpoint}"));
    }

    out
}
