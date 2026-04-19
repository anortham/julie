//! Reusable comparison harness for Tantivy upgrade snapshots.

use super::helpers::{search_with_metadata, setup_handler_with_fixture};
use crate::handler::JulieServerHandler;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QueryTarget {
    Content,
    Definitions,
}

impl QueryTarget {
    fn as_str(self) -> &'static str {
        match self {
            QueryTarget::Content => "content",
            QueryTarget::Definitions => "definitions",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TantivyUpgradeQuerySet {
    pub suite: String,
    pub version: u32,
    pub default_top_n: usize,
    pub queries: Vec<TantivyUpgradeQuery>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TantivyUpgradeQuery {
    pub id: String,
    pub category: String,
    pub description: String,
    pub target: QueryTarget,
    pub query: String,
    #[serde(default)]
    pub top_n: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SnapshotMetadata {
    pub suite: String,
    pub version: u32,
    pub default_top_n: usize,
    pub query_count: usize,
    pub capture_source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchSnapshot {
    pub metadata: SnapshotMetadata,
    pub queries: Vec<QuerySnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QuerySnapshot {
    pub query_id: String,
    pub category: String,
    pub description: String,
    pub query: String,
    pub target: QueryTarget,
    pub relaxed: bool,
    pub total_hits: usize,
    pub top_results: Vec<SearchHitSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchHitSnapshot {
    pub rank: usize,
    pub file_path: String,
    pub symbol_name: Option<String>,
    pub kind: String,
    pub language: String,
    pub start_line: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SnapshotDiff {
    pub compared_queries: usize,
    pub changed_top_results: Vec<TopResultChange>,
    pub additions: Vec<QueryResultDelta>,
    pub removals: Vec<QueryResultDelta>,
    pub rank_jumps: Vec<RankJump>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TopResultChange {
    pub query_id: String,
    pub before: Option<SearchHitSnapshot>,
    pub after: Option<SearchHitSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueryResultDelta {
    pub query_id: String,
    pub entries: Vec<SearchHitSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RankJump {
    pub query_id: String,
    pub file_path: String,
    pub symbol_name: Option<String>,
    pub kind: String,
    pub before_rank: usize,
    pub after_rank: usize,
    pub delta: isize,
}

pub fn load_query_set(path: &Path) -> Result<TantivyUpgradeQuerySet> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("Failed to read query fixture at {}", path.display()))?;
    let query_set: TantivyUpgradeQuerySet = serde_json::from_str(&raw)
        .with_context(|| format!("Failed to parse query fixture at {}", path.display()))?;

    validate_query_set(&query_set)?;

    Ok(query_set)
}

pub async fn capture_fixture_snapshot_from_file(path: &Path) -> Result<SearchSnapshot> {
    let query_set = load_query_set(path)?;
    let handler = setup_handler_with_fixture().await;
    capture_snapshot_for_query_set(&handler, &query_set).await
}

pub async fn capture_snapshot_for_query_set(
    handler: &JulieServerHandler,
    query_set: &TantivyUpgradeQuerySet,
) -> Result<SearchSnapshot> {
    validate_query_set(query_set)?;

    let mut query_snapshots = Vec::with_capacity(query_set.queries.len());
    for query in &query_set.queries {
        let top_n = query.top_n.unwrap_or(query_set.default_top_n);
        if top_n == 0 {
            bail!("Query '{}' configured top_n=0", query.id);
        }

        let limit = u32::try_from(top_n)
            .with_context(|| format!("Query '{}' uses top_n above u32 range", query.id))?;

        let run = search_with_metadata(handler, &query.query, limit, query.target.as_str())
            .await
            .with_context(|| format!("Query '{}' failed", query.id))?;

        let top_results = run
            .symbols
            .into_iter()
            .take(top_n)
            .enumerate()
            .map(|(index, symbol)| SearchHitSnapshot {
                rank: index + 1,
                file_path: symbol.file_path,
                symbol_name: if symbol.name.is_empty() {
                    None
                } else {
                    Some(symbol.name)
                },
                kind: symbol.kind.to_string(),
                language: symbol.language,
                start_line: symbol.start_line,
            })
            .collect::<Vec<_>>();

        query_snapshots.push(QuerySnapshot {
            query_id: query.id.clone(),
            category: query.category.clone(),
            description: query.description.clone(),
            query: query.query.clone(),
            target: query.target,
            relaxed: run.relaxed,
            total_hits: run.total_hits,
            top_results,
        });
    }

    Ok(SearchSnapshot {
        metadata: SnapshotMetadata {
            suite: query_set.suite.clone(),
            version: query_set.version,
            default_top_n: query_set.default_top_n,
            query_count: query_set.queries.len(),
            capture_source: "setup_handler_with_fixture".to_string(),
        },
        queries: query_snapshots,
    })
}

pub fn write_snapshot_to_path(snapshot: &SearchSnapshot, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "Failed to create snapshot output directory {}",
                parent.display()
            )
        })?;
    }

    let body = serde_json::to_string_pretty(snapshot)
        .context("Failed to serialize search snapshot as pretty JSON")?;
    fs::write(path, body)
        .with_context(|| format!("Failed to write snapshot to {}", path.display()))?;
    Ok(())
}

#[allow(dead_code)]
pub fn load_snapshot_from_path(path: &Path) -> Result<SearchSnapshot> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("Failed to read snapshot at {}", path.display()))?;
    let snapshot = serde_json::from_str(&raw)
        .with_context(|| format!("Failed to parse snapshot JSON at {}", path.display()))?;
    Ok(snapshot)
}

pub fn diff_snapshots(
    before: &SearchSnapshot,
    after: &SearchSnapshot,
    min_rank_jump: usize,
) -> SnapshotDiff {
    let before_by_id = before
        .queries
        .iter()
        .map(|query| (query.query_id.as_str(), query))
        .collect::<BTreeMap<_, _>>();
    let after_by_id = after
        .queries
        .iter()
        .map(|query| (query.query_id.as_str(), query))
        .collect::<BTreeMap<_, _>>();

    let query_ids = before_by_id
        .keys()
        .chain(after_by_id.keys())
        .map(|query_id| (*query_id).to_string())
        .collect::<BTreeSet<_>>();

    let mut changed_top_results = Vec::new();
    let mut additions = Vec::new();
    let mut removals = Vec::new();
    let mut rank_jumps = Vec::new();

    for query_id in &query_ids {
        let before_query = before_by_id.get(query_id.as_str()).copied();
        let after_query = after_by_id.get(query_id.as_str()).copied();

        let before_top = before_query.and_then(|query| query.top_results.first().cloned());
        let after_top = after_query.and_then(|query| query.top_results.first().cloned());
        if before_top != after_top {
            changed_top_results.push(TopResultChange {
                query_id: query_id.clone(),
                before: before_top,
                after: after_top,
            });
        }

        let before_hits = before_query
            .map(|query| query.top_results.as_slice())
            .unwrap_or(&[]);
        let after_hits = after_query
            .map(|query| query.top_results.as_slice())
            .unwrap_or(&[]);

        let before_map = index_hits(before_hits);
        let after_map = index_hits(after_hits);

        let mut added = after_map
            .iter()
            .filter(|(key, _)| !before_map.contains_key(*key))
            .map(|(_, hit)| hit.clone())
            .collect::<Vec<_>>();
        if !added.is_empty() {
            added.sort_by_key(|hit| hit.rank);
            additions.push(QueryResultDelta {
                query_id: query_id.clone(),
                entries: added,
            });
        }

        let mut removed = before_map
            .iter()
            .filter(|(key, _)| !after_map.contains_key(*key))
            .map(|(_, hit)| hit.clone())
            .collect::<Vec<_>>();
        if !removed.is_empty() {
            removed.sort_by_key(|hit| hit.rank);
            removals.push(QueryResultDelta {
                query_id: query_id.clone(),
                entries: removed,
            });
        }

        for key in before_map.keys() {
            let Some(before_hit) = before_map.get(key) else {
                continue;
            };
            let Some(after_hit) = after_map.get(key) else {
                continue;
            };

            let delta = after_hit.rank as isize - before_hit.rank as isize;
            if delta == 0 || delta.abs() < min_rank_jump as isize {
                continue;
            }

            rank_jumps.push(RankJump {
                query_id: query_id.clone(),
                file_path: after_hit.file_path.clone(),
                symbol_name: after_hit.symbol_name.clone(),
                kind: after_hit.kind.clone(),
                before_rank: before_hit.rank,
                after_rank: after_hit.rank,
                delta,
            });
        }
    }

    additions.sort_by(|left, right| left.query_id.cmp(&right.query_id));
    removals.sort_by(|left, right| left.query_id.cmp(&right.query_id));
    rank_jumps.sort_by(|left, right| {
        left.query_id
            .cmp(&right.query_id)
            .then(left.after_rank.cmp(&right.after_rank))
    });

    SnapshotDiff {
        compared_queries: query_ids.len(),
        changed_top_results,
        additions,
        removals,
        rank_jumps,
    }
}

fn validate_query_set(query_set: &TantivyUpgradeQuerySet) -> Result<()> {
    if query_set.suite.trim().is_empty() {
        bail!("Query fixture suite must not be empty");
    }
    if query_set.default_top_n == 0 {
        bail!("Query fixture default_top_n must be greater than zero");
    }
    if query_set.queries.is_empty() {
        bail!("Query fixture must include at least one query");
    }

    let mut seen_query_ids = BTreeSet::new();
    for query in &query_set.queries {
        if query.id.trim().is_empty() {
            bail!("Query fixture has an empty query id");
        }
        if !seen_query_ids.insert(query.id.clone()) {
            bail!("Duplicate query id '{}' in query fixture", query.id);
        }
        if query.category.trim().is_empty() {
            bail!("Query '{}' has an empty category", query.id);
        }
        if query.query.trim().is_empty() {
            bail!("Query '{}' has an empty search string", query.id);
        }
        if let Some(top_n) = query.top_n {
            if top_n == 0 {
                bail!("Query '{}' configured top_n=0", query.id);
            }
        }
    }

    Ok(())
}

fn index_hits(hits: &[SearchHitSnapshot]) -> BTreeMap<String, SearchHitSnapshot> {
    hits.iter()
        .map(|hit| (hit_identity_key(hit), hit.clone()))
        .collect::<BTreeMap<_, _>>()
}

fn hit_identity_key(hit: &SearchHitSnapshot) -> String {
    format!(
        "{}|{}|{}|{}",
        hit.file_path,
        hit.symbol_name.as_deref().unwrap_or_default(),
        hit.kind,
        hit.start_line
    )
}
