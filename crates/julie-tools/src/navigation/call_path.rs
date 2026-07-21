use std::collections::{HashMap, HashSet, VecDeque};

use anyhow::{anyhow, Result};
use julie_core::mcp_compat::{CallToolResult, Content};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::deep_dive::data::find_symbol;
use julie_context::ToolContext;
use julie_core::database::SymbolDatabase;
use julie_core::mcp_compat::CallToolResultExt;
use julie_extractors::{Relationship, RelationshipKind, Symbol};

use super::resolution::{file_path_matches_suffix, WorkspaceTarget};

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
/// BFS traverses Calls, Instantiates, and Overrides relationships only.
/// Extends/Implements/TypeUsage/Reference edges are not followed.
/// Set `mode = "web"` to additionally follow derived `http_call` (and, in
/// Phase 2, `sql_query`) web edges so a frontend client-call traces through
/// to its backend handler.
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
    /// Traversal mode. `default` (omitted) follows Calls/Instantiates/Overrides
    /// only — output is byte-identical to the legacy tool. `web` additionally
    /// follows derived `http_call` edges (client-call symbol -> route handler)
    /// and reports external endpoints reached by unmatched client calls.
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

#[derive(Clone)]
struct ResolvedEndpoints {
    from: Symbol,
    targets: HashSet<String>,
}

#[derive(Clone)]
struct PathSearchResult {
    target_id: Option<String>,
    predecessor: HashMap<String, Relationship>,
}

fn find_matching_symbols(
    db: &SymbolDatabase,
    name: &str,
    file_path: Option<&str>,
) -> Result<Vec<Symbol>> {
    let all_matches = find_symbol(db, name, None)?;
    Ok(if let Some(filter) = file_path {
        all_matches
            .into_iter()
            .filter(|s| file_path_matches_suffix(&s.file_path, filter))
            .collect()
    } else {
        all_matches
    })
}

fn relationship_priority(kind: &RelationshipKind) -> u8 {
    match kind {
        RelationshipKind::Calls => 0,
        RelationshipKind::Instantiates => 1,
        RelationshipKind::Overrides => 2,
        _ => unreachable!("BFS only traverses Calls, Instantiates, and Overrides"),
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
    db: &SymbolDatabase,
    name: &str,
    role: &str,
    file_path: Option<&str>,
) -> Result<Symbol> {
    let matches = find_matching_symbols(db, name, file_path)?;
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
            .map(|symbol| {
                format!(
                    "  {} at {}:{}-{}",
                    symbol.name, symbol.file_path, symbol.start_line, symbol.end_line
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
    Ok(matches.into_iter().next().expect("one symbol"))
}

fn resolve_target_ids(
    db: &SymbolDatabase,
    name: &str,
    file_path: Option<&str>,
) -> Result<HashSet<String>> {
    let matches = find_matching_symbols(db, name, file_path)?;
    if matches.is_empty() {
        return Err(anyhow!(
            "Symbol '{}' for 'to' was not found. Use fast_search or deep_dive to verify the name.",
            name
        ));
    }

    Ok(matches.into_iter().map(|symbol| symbol.id).collect())
}

fn resolve_endpoints(
    db: &SymbolDatabase,
    from: &str,
    to: &str,
    from_file_path: Option<&str>,
    to_file_path: Option<&str>,
) -> Result<ResolvedEndpoints> {
    let from_symbol = resolve_unique_symbol(db, from, "from", from_file_path)?;
    let targets = resolve_target_ids(db, to, to_file_path)?;
    Ok(ResolvedEndpoints {
        from: from_symbol,
        targets,
    })
}

fn bfs_shortest_path(
    db: &SymbolDatabase,
    start_id: &str,
    targets: &HashSet<String>,
    max_hops: u32,
) -> Result<PathSearchResult> {
    if targets.contains(start_id) {
        return Ok(PathSearchResult {
            target_id: Some(start_id.to_string()),
            predecessor: HashMap::new(),
        });
    }

    let mut visited = HashSet::from([start_id.to_string()]);
    let mut frontier = vec![start_id.to_string()];
    let mut predecessor = HashMap::new();

    for _depth in 0..max_hops {
        if frontier.is_empty() {
            break;
        }

        let frontier_ids = frontier.clone();
        let mut relationships = db.get_outgoing_relationships_for_symbols(&frontier_ids)?;
        relationships.retain(|rel| {
            matches!(
                rel.kind,
                RelationshipKind::Calls
                    | RelationshipKind::Instantiates
                    | RelationshipKind::Overrides
            )
        });
        relationships.sort_by(|left, right| {
            let source_cmp = left.from_symbol_id.cmp(&right.from_symbol_id);
            if source_cmp != std::cmp::Ordering::Equal {
                return source_cmp;
            }
            let kind_cmp =
                relationship_priority(&left.kind).cmp(&relationship_priority(&right.kind));
            if kind_cmp != std::cmp::Ordering::Equal {
                return kind_cmp;
            }
            let confidence_cmp = right
                .confidence
                .partial_cmp(&left.confidence)
                .unwrap_or(std::cmp::Ordering::Equal);
            if confidence_cmp != std::cmp::Ordering::Equal {
                return confidence_cmp;
            }
            let line_cmp = left.line_number.cmp(&right.line_number);
            if line_cmp != std::cmp::Ordering::Equal {
                return line_cmp;
            }
            let target_cmp = left.to_symbol_id.cmp(&right.to_symbol_id);
            if target_cmp != std::cmp::Ordering::Equal {
                return target_cmp;
            }
            left.id.cmp(&right.id)
        });

        let mut next_frontier = Vec::new();
        for relationship in relationships {
            if !visited.insert(relationship.to_symbol_id.clone()) {
                continue;
            }

            predecessor.insert(relationship.to_symbol_id.clone(), relationship.clone());
            if targets.contains(&relationship.to_symbol_id) {
                return Ok(PathSearchResult {
                    target_id: Some(relationship.to_symbol_id.clone()),
                    predecessor,
                });
            }
            next_frontier.push(relationship.to_symbol_id);
        }

        frontier = next_frontier;
    }

    Ok(PathSearchResult {
        target_id: None,
        predecessor,
    })
}

fn build_hops(
    db: &SymbolDatabase,
    start_symbol: &Symbol,
    target_id: &str,
    predecessor: &HashMap<String, Relationship>,
) -> Result<Vec<CallPathHop>> {
    let mut chain = VecDeque::new();
    let mut current_id = target_id.to_string();

    while current_id != start_symbol.id {
        let relationship = predecessor
            .get(&current_id)
            .ok_or_else(|| anyhow!("Path reconstruction failed at '{}'", current_id))?;
        chain.push_front(relationship.clone());
        current_id = relationship.from_symbol_id.clone();
    }

    let mut symbol_ids = vec![start_symbol.id.clone()];
    for relationship in &chain {
        symbol_ids.push(relationship.to_symbol_id.clone());
        symbol_ids.push(relationship.from_symbol_id.clone());
    }
    symbol_ids.sort();
    symbol_ids.dedup();

    let symbol_map = db
        .get_symbols_by_ids(&symbol_ids)?
        .into_iter()
        .map(|symbol| (symbol.id.clone(), symbol))
        .collect::<HashMap<_, _>>();

    let mut hops = Vec::new();
    for relationship in chain {
        let from_symbol = symbol_map
            .get(&relationship.from_symbol_id)
            .ok_or_else(|| anyhow!("Missing symbol '{}'", relationship.from_symbol_id))?;
        let to_symbol = symbol_map
            .get(&relationship.to_symbol_id)
            .ok_or_else(|| anyhow!("Missing symbol '{}'", relationship.to_symbol_id))?;

        hops.push(CallPathHop {
            from: from_symbol.name.clone(),
            to: to_symbol.name.clone(),
            edge: edge_label(&relationship.kind).to_string(),
            file: format!("{}:{}", relationship.file_path, relationship.line_number),
            target_file: to_symbol.file_path.clone(),
            target_start_line: to_symbol.start_line,
        });
    }

    Ok(hops)
}

// ---------------------------------------------------------------------------
// Web mode: BFS that additionally follows derived `http_call`/`sql_query` web
// edges. Extracted into `call_path_web.rs` for file-size hygiene.
// ---------------------------------------------------------------------------
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

    async fn resolve_workspace_target(&self, handler: &dyn ToolContext) -> Result<SymbolDatabase> {
        match handler
            .resolve_workspace_target(self.workspace.as_deref())
            .await?
        {
            WorkspaceTarget::Primary => handler.primary_pooled_database().await,
            WorkspaceTarget::Target(workspace_id) => {
                handler
                    .get_pooled_database_for_workspace(&workspace_id)
                    .await
            }
        }
    }

    pub async fn call_tool(&self, handler: &dyn ToolContext) -> Result<CallToolResult> {
        if self.from.is_empty() || self.to.is_empty() {
            return Self::response_result(&Self::diagnostic_response(
                "both 'from' and 'to' are required",
            ));
        }
        if !(1..=MAX_HOPS).contains(&self.max_hops) {
            return Self::response_result(&Self::diagnostic_response(format!(
                "max_hops must be in the range 1..={MAX_HOPS}"
            )));
        }
        match self.mode.as_deref() {
            None | Some("default") | Some("web") => {}
            Some(other) => {
                return Self::response_result(&Self::diagnostic_response(format!(
                    "mode must be 'default' or 'web'; got '{other}'"
                )));
            }
        }

        let db = match self.resolve_workspace_target(handler).await {
            Ok(db) => db,
            Err(error) => {
                return Self::response_result(&Self::diagnostic_response(format!(
                    "Workspace resolution failed: {error}"
                )));
            }
        };
        let from = self.from.clone();
        let to = self.to.clone();
        let max_hops = self.max_hops;
        let from_file_path = self.from_file_path.clone();
        let to_file_path = self.to_file_path.clone();
        let web_mode = self.mode.as_deref() == Some("web");

        let response = tokio::task::spawn_blocking(move || -> Result<CallPathResponse> {
            let endpoints = resolve_endpoints(
                &db,
                &from,
                &to,
                from_file_path.as_deref(),
                to_file_path.as_deref(),
            )?;

            if web_mode {
                return call_path_web::run_web_call_path(&db, &endpoints, max_hops, &from, &to);
            }

            let search = bfs_shortest_path(&db, &endpoints.from.id, &endpoints.targets, max_hops)?;

            if endpoints.targets.contains(&endpoints.from.id) {
                return Ok(CallPathResponse {
                    found: true,
                    hops: 0,
                    path: Vec::new(),
                    diagnostic: None,
                    external_endpoints: Vec::new(),
                });
            }

            if let Some(target_id) = search.target_id.as_deref() {
                let hops = build_hops(&db, &endpoints.from, target_id, &search.predecessor)?;
                return Ok(CallPathResponse {
                    found: true,
                    hops: hops.len() as u32,
                    path: hops,
                    diagnostic: None,
                    external_endpoints: Vec::new(),
                });
            }

            Ok(CallPathResponse {
                found: false,
                hops: 0,
                path: Vec::new(),
                diagnostic: Some(format!(
                    "No path found from '{}' to '{}' within {} hops.",
                    from, to, max_hops
                )),
                external_endpoints: Vec::new(),
            })
        })
        .await;

        let response = match response {
            Ok(Ok(response)) => response,
            Ok(Err(error)) => Self::diagnostic_response(error.to_string()),
            Err(error) => Self::diagnostic_response(format!("call_path worker failed: {error}")),
        };

        debug!(
            "call_path {} -> {} found={} hops={}",
            self.from, self.to, response.found, response.hops
        );

        Self::response_result(&response)
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
