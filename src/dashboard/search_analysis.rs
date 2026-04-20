use serde::Serialize;
use serde_json::Value;

use crate::daemon::database::SearchToolCallRow;

const USEFUL_ACTIONS: &[&str] = &[
    "deep_dive",
    "get_symbols",
    "fast_refs",
    "call_path",
    "get_context",
    "edit_file",
    "rewrite_symbol",
    "rename_symbol",
];

#[derive(Debug, Clone, Serialize)]
pub struct SearchEpisodeQuery {
    pub timestamp: i64,
    pub query: String,
    pub normalized_query: String,
    pub intent: String,
    pub search_target: String,
    pub top_hit_name: Option<String>,
    pub top_hit_file: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchEpisode {
    pub session_id: String,
    pub workspace_id: String,
    pub start_ts: i64,
    pub end_ts: i64,
    pub search_count: usize,
    pub queries: Vec<SearchEpisodeQuery>,
    pub downstream_tool: Option<String>,
    pub target_symbol_name: Option<String>,
    pub target_file_path: Option<String>,
    pub outcome: String,
    pub suspicious: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct EpisodeStats {
    pub total_episodes: usize,
    pub convergence_rate: f64,
    pub stall_rate: f64,
}

pub fn analyze_tool_calls(rows: &[SearchToolCallRow]) -> Vec<SearchEpisode> {
    let mut episodes = Vec::new();
    let mut current: Option<EpisodeBuilder> = None;

    for row in rows {
        if row.tool_name == "fast_search" {
            let search = parse_search_query(row);
            let should_start_new = current.as_ref().is_none_or(|episode| {
                episode.session_id != row.session_id
                    || row.timestamp - episode.last_search_ts > 10
                    || episode.closed
            });

            if should_start_new {
                if let Some(episode) = current.take() {
                    episodes.push(episode.finish());
                }
                current = Some(EpisodeBuilder::new(row, search));
            } else if let Some(episode) = current.as_mut() {
                episode.push_search(row, search);
            }
            continue;
        }

        if let Some(episode) = current.as_mut() {
            if episode.session_id != row.session_id {
                let finished = current.take().expect("episode");
                episodes.push(finished.finish());
            } else {
                episode.closed = true;
                if USEFUL_ACTIONS.contains(&row.tool_name.as_str()) {
                    episode.downstream_tool = Some(row.tool_name.clone());
                    let metadata = parse_metadata(row.metadata.as_deref());
                    episode.target_symbol_name = metadata["target"]["target_symbol_name"]
                        .as_str()
                        .map(ToOwned::to_owned);
                    episode.target_file_path = metadata["target"]["target_file_path"]
                        .as_str()
                        .map(ToOwned::to_owned);
                }
                let finished = current.take().expect("episode");
                episodes.push(finished.finish());
            }
        }
    }

    if let Some(episode) = current.take() {
        episodes.push(episode.finish());
    }

    episodes
}

pub fn episode_stats(episodes: &[SearchEpisode]) -> EpisodeStats {
    let total = episodes.len().max(1) as f64;
    let converged = episodes
        .iter()
        .filter(|episode| episode.outcome == "reformulation_converged")
        .count() as f64;
    let stalled = episodes
        .iter()
        .filter(|episode| episode.outcome == "stalled")
        .count() as f64;

    EpisodeStats {
        total_episodes: episodes.len(),
        convergence_rate: converged / total,
        stall_rate: stalled / total,
    }
}

struct EpisodeBuilder {
    session_id: String,
    workspace_id: String,
    start_ts: i64,
    end_ts: i64,
    last_search_ts: i64,
    queries: Vec<SearchEpisodeQuery>,
    downstream_tool: Option<String>,
    target_symbol_name: Option<String>,
    target_file_path: Option<String>,
    closed: bool,
}

impl EpisodeBuilder {
    fn new(row: &SearchToolCallRow, query: SearchEpisodeQuery) -> Self {
        Self {
            session_id: row.session_id.clone(),
            workspace_id: row.workspace_id.clone(),
            start_ts: row.timestamp,
            end_ts: row.timestamp,
            last_search_ts: row.timestamp,
            queries: vec![query],
            downstream_tool: None,
            target_symbol_name: None,
            target_file_path: None,
            closed: false,
        }
    }

    fn push_search(&mut self, row: &SearchToolCallRow, query: SearchEpisodeQuery) {
        self.end_ts = row.timestamp;
        self.last_search_ts = row.timestamp;
        self.queries.push(query);
    }

    fn finish(self) -> SearchEpisode {
        let search_count = self.queries.len();
        let overlapping_queries = queries_overlap(&self.queries);
        let outcome = if self.downstream_tool.is_none() {
            "stalled".to_string()
        } else if search_count == 1 {
            "one_shot_success".to_string()
        } else if overlapping_queries
            && (self.target_symbol_name.is_some() || self.target_file_path.is_some())
        {
            "reformulation_converged".to_string()
        } else {
            "exploratory_success".to_string()
        };
        let suspicious = matches!(outcome.as_str(), "stalled" | "reformulation_converged");

        SearchEpisode {
            session_id: self.session_id,
            workspace_id: self.workspace_id,
            start_ts: self.start_ts,
            end_ts: self.end_ts.max(self.last_search_ts),
            search_count,
            queries: self.queries,
            downstream_tool: self.downstream_tool,
            target_symbol_name: self.target_symbol_name,
            target_file_path: self.target_file_path,
            outcome,
            suspicious,
        }
    }
}

fn parse_search_query(row: &SearchToolCallRow) -> SearchEpisodeQuery {
    let metadata = parse_metadata(row.metadata.as_deref());
    let trace = &metadata["trace"];

    SearchEpisodeQuery {
        timestamp: row.timestamp,
        query: metadata["query"].as_str().unwrap_or_default().to_string(),
        normalized_query: metadata["normalized_query"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        intent: metadata["intent"].as_str().unwrap_or("unknown").to_string(),
        search_target: metadata["search_target"]
            .as_str()
            .unwrap_or("definitions")
            .to_string(),
        top_hit_name: trace["top_hits"][0]["name"].as_str().map(ToOwned::to_owned),
        top_hit_file: trace["top_hits"][0]["file"].as_str().map(ToOwned::to_owned),
    }
}

fn parse_metadata(metadata: Option<&str>) -> Value {
    metadata
        .and_then(|text| serde_json::from_str::<Value>(text).ok())
        .unwrap_or(Value::Null)
}

fn queries_overlap(queries: &[SearchEpisodeQuery]) -> bool {
    for (idx, left) in queries.iter().enumerate() {
        for right in queries.iter().skip(idx + 1) {
            if left.normalized_query == right.normalized_query {
                return true;
            }
            let left_tokens = token_set(&left.normalized_query);
            let right_tokens = token_set(&right.normalized_query);
            let overlap = left_tokens.intersection(&right_tokens).count();
            if overlap > 0 && overlap * 2 >= left_tokens.len().max(right_tokens.len()) {
                return true;
            }
        }
    }
    false
}

fn token_set(text: &str) -> std::collections::BTreeSet<&str> {
    text.split_whitespace().collect()
}
