use std::cmp::Ordering;

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
    pub top_hit_score: Option<f32>,
    pub result_count: Option<usize>,
    pub strategy: Option<String>,
    pub relaxed: Option<bool>,
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
    pub flags: Vec<String>,
    pub best_score: Option<f32>,
    pub min_result_count: Option<usize>,
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
                    || episode.workspace_id != row.workspace_id
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
            if episode.session_id != row.session_id
                || episode.workspace_id != row.workspace_id
            {
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
    let reformulated = episodes
        .iter()
        .filter(|e| e.outcome == "reformulation_converged")
        .count() as f64;
    let stalled = episodes
        .iter()
        .filter(|e| e.outcome == "stalled")
        .count() as f64;

    EpisodeStats {
        total_episodes: episodes.len(),
        convergence_rate: reformulated / total,
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
        let best_score = self
            .queries
            .iter()
            .filter_map(|q| q.top_hit_score)
            .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let min_result_count = self.queries.iter().filter_map(|q| q.result_count).min();

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
            flags: Vec::new(),
            best_score,
            min_result_count,
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
        top_hit_score: trace["top_hits"][0]["score"].as_f64().map(|v| v as f32),
        result_count: trace["result_count"].as_u64().map(|v| v as usize),
        strategy: trace["strategy"].as_str().map(ToOwned::to_owned),
        relaxed: trace["relaxed"].as_bool(),
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

// ---------------------------------------------------------------------------
// Friction flags + summary (observability workbench)
// ---------------------------------------------------------------------------

const LOW_SCORE_THRESHOLD: f32 = 5.0;

pub fn has_trace_data(episode: &SearchEpisode) -> bool {
    episode.queries.iter().any(|q| q.strategy.is_some())
}

pub fn compute_flags(episode: &mut SearchEpisode) {
    if episode.queries.iter().any(|q| q.result_count == Some(0)) {
        episode.flags.push("zero_hits".to_string());
    }
    if queries_overlap(&episode.queries) {
        episode.flags.push("repeat_query".to_string());
    }
    if episode
        .queries
        .iter()
        .any(|q| q.top_hit_score.is_some_and(|s| s < LOW_SCORE_THRESHOLD))
    {
        episode.flags.push("low_score".to_string());
    }
    if episode.downstream_tool.is_none() {
        episode.flags.push("no_follow_up".to_string());
    }
    if episode.queries.iter().any(|q| q.relaxed == Some(true)) {
        episode.flags.push("relaxed".to_string());
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchSummary {
    pub episode_count: usize,
    pub zero_hit_count: usize,
    pub zero_hit_rate: f64,
    pub median_top_score: Option<f32>,
    pub repeat_query_count: usize,
    pub repeat_query_rate: f64,
}

pub fn compute_summary(episodes: &[SearchEpisode]) -> SearchSummary {
    let total = episodes.len().max(1) as f64;
    let zero_hit_count = episodes
        .iter()
        .filter(|e| e.flags.contains(&"zero_hits".to_string()))
        .count();
    let repeat_query_count = episodes
        .iter()
        .filter(|e| e.flags.contains(&"repeat_query".to_string()))
        .count();

    let mut scores: Vec<f32> = episodes
        .iter()
        .flat_map(|e| e.queries.iter())
        .filter_map(|q| q.top_hit_score)
        .collect();
    scores.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let median_top_score = if scores.is_empty() {
        None
    } else {
        Some(scores[scores.len() / 2])
    };

    SearchSummary {
        episode_count: episodes.len(),
        zero_hit_count,
        zero_hit_rate: zero_hit_count as f64 / total,
        median_top_score,
        repeat_query_count,
        repeat_query_rate: repeat_query_count as f64 / total,
    }
}
