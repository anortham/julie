use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use rmcp::model::{CallToolResult, Content};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::database::SymbolDatabase;
use crate::extractors::{Relationship, RelationshipKind, Symbol};
use crate::handler::JulieServerHandler;
use crate::mcp_compat::CallToolResultExt;
use crate::tools::deep_dive::data::find_symbol;

use super::lock_db;
use super::resolution::{WorkspaceTarget, file_path_matches_suffix, resolve_workspace_filter};

fn default_max_hops() -> u32 {
    6
}

fn default_workspace() -> Option<String> {
    Some("primary".to_string())
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
/// BFS traverses Calls, Instantiates, and Overrides relationships only.
/// Extends/Implements/TypeUsage/Reference edges are not followed.
pub struct CallPathTool {
    pub from: String,
    pub to: String,
    #[serde(
        default = "default_max_hops",
        deserialize_with = "crate::utils::serde_lenient::deserialize_u32_lenient"
    )]
    pub max_hops: u32,
    #[serde(default = "default_workspace")]
    pub workspace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_file_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_file_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CallPathResponse {
    pub found: bool,
    pub hops: u32,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub path: Vec<CallPathHop>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CallPathHop {
    pub from: String,
    pub to: String,
    pub edge: String,
    pub file: String,
}

struct WorkspaceQueryTarget {
    db: Arc<Mutex<SymbolDatabase>>,
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

fn relationship_priority(kind: &RelationshipKind) -> u8 {
    match kind {
        RelationshipKind::Calls => 0,
        RelationshipKind::Instantiates => 1,
        RelationshipKind::Overrides => 2,
        _ => unreachable!("BFS only traverses Calls, Instantiates, and Overrides"),
    }
}

pub(crate) fn edge_label(kind: &RelationshipKind) -> &'static str {
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
    let all_matches = find_symbol(db, name, None)?;
    let matches: Vec<Symbol> = if let Some(filter) = file_path {
        all_matches
            .into_iter()
            .filter(|s| file_path_matches_suffix(&s.file_path, filter))
            .collect()
    } else {
        all_matches
    };
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

fn resolve_endpoints(
    db: &SymbolDatabase,
    from: &str,
    to: &str,
    from_file_path: Option<&str>,
    to_file_path: Option<&str>,
) -> Result<ResolvedEndpoints> {
    let from_symbol = resolve_unique_symbol(db, from, "from", from_file_path)?;
    let to_symbol = resolve_unique_symbol(db, to, "to", to_file_path)?;
    let mut targets = HashSet::new();
    targets.insert(to_symbol.id);
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
            left.line_number.cmp(&right.line_number)
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
        });
    }

    Ok(hops)
}

impl CallPathTool {
    async fn resolve_workspace_target(
        &self,
        handler: &JulieServerHandler,
    ) -> Result<WorkspaceQueryTarget> {
        match resolve_workspace_filter(self.workspace.as_deref(), handler).await? {
            WorkspaceTarget::Primary => {
                let snapshot = handler.primary_workspace_snapshot().await?;
                Ok(WorkspaceQueryTarget {
                    db: snapshot.database.clone(),
                })
            }
            WorkspaceTarget::Target(workspace_id) => Ok(WorkspaceQueryTarget {
                db: handler.get_database_for_workspace(&workspace_id).await?,
            }),
        }
    }

    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        if self.from.is_empty() || self.to.is_empty() {
            return Ok(CallToolResult::text_content(vec![Content::text(
                "Error: both 'from' and 'to' are required".to_string(),
            )]));
        }
        if self.max_hops == 0 {
            return Ok(CallToolResult::text_content(vec![Content::text(
                "Error: max_hops must be at least 1".to_string(),
            )]));
        }

        let target = self.resolve_workspace_target(handler).await?;
        let from = self.from.clone();
        let to = self.to.clone();
        let max_hops = self.max_hops;
        let from_file_path = self.from_file_path.clone();
        let to_file_path = self.to_file_path.clone();

        let response = tokio::task::spawn_blocking(move || -> Result<CallPathResponse> {
            let db = lock_db(&target.db, "call_path");
            let endpoints =
                resolve_endpoints(&db, &from, &to, from_file_path.as_deref(), to_file_path.as_deref())?;
            let search = bfs_shortest_path(&db, &endpoints.from.id, &endpoints.targets, max_hops)?;

            if endpoints.targets.contains(&endpoints.from.id) {
                return Ok(CallPathResponse {
                    found: true,
                    hops: 0,
                    path: Vec::new(),
                    diagnostic: None,
                });
            }

            if let Some(target_id) = search.target_id.as_deref() {
                let hops = build_hops(&db, &endpoints.from, target_id, &search.predecessor)?;
                return Ok(CallPathResponse {
                    found: true,
                    hops: hops.len() as u32,
                    path: hops,
                    diagnostic: None,
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
            })
        })
        .await;

        let response = match response {
            Ok(Ok(response)) => response,
            Ok(Err(error)) => {
                return Ok(CallToolResult::text_content(vec![Content::text(format!(
                    "Error: {}",
                    error
                ))]));
            }
            Err(error) => {
                return Ok(CallToolResult::text_content(vec![Content::text(format!(
                    "Error: call_path worker failed: {}",
                    error
                ))]));
            }
        };

        debug!(
            "call_path {} -> {} found={} hops={}",
            self.from, self.to, response.found, response.hops
        );

        Ok(CallToolResult::text_content(vec![Content::text(
            serde_json::to_string_pretty(&response)?,
        )]))
    }
}
