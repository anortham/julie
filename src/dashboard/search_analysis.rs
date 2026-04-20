use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

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
}

#[derive(Debug, Clone, Serialize)]
pub struct EpisodeStats {
    pub total_episodes: usize,
    pub convergence_rate: f64,
    pub stall_rate: f64,
    pub first_try_rate: f64,
    pub one_shot_count: usize,
    pub reformulation_count: usize,
    pub stall_count: usize,
    pub exploratory_count: usize,
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
    let mut one_shot = 0usize;
    let mut reformulated = 0usize;
    let mut stalled = 0usize;
    let mut exploratory = 0usize;

    for episode in episodes {
        match episode.outcome.as_str() {
            "one_shot_success" => one_shot += 1,
            "reformulation_converged" => reformulated += 1,
            "stalled" => stalled += 1,
            _ => exploratory += 1,
        }
    }

    EpisodeStats {
        total_episodes: episodes.len(),
        convergence_rate: reformulated as f64 / total,
        stall_rate: stalled as f64 / total,
        first_try_rate: one_shot as f64 / total,
        one_shot_count: one_shot,
        reformulation_count: reformulated,
        stall_count: stalled,
        exploratory_count: exploratory,
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

fn pair_queries_overlap(left: &str, right: &str) -> bool {
    if left == right {
        return true;
    }
    let left_tokens = token_set(left);
    let right_tokens = token_set(right);
    let overlap = left_tokens.intersection(&right_tokens).count();
    overlap > 0 && overlap * 2 >= left_tokens.len().max(right_tokens.len())
}

fn token_set(text: &str) -> std::collections::BTreeSet<&str> {
    text.split_whitespace().collect()
}

// ---------------------------------------------------------------------------
// Canonical key for query grouping
// ---------------------------------------------------------------------------

const FILLER_TOKENS: &[&str] = &[
    "find", "get", "the", "a", "an", "for", "in", "of", "to", "with",
];

pub fn canonical_key(query: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    for part in query.split(|c: char| c.is_whitespace() || c == ':' || c == '_' || c == '.') {
        if part.is_empty() {
            continue;
        }
        split_camel_case(part, &mut tokens);
    }
    tokens.retain(|t| !FILLER_TOKENS.contains(&t.as_str()));
    tokens.sort();
    tokens
}

fn split_camel_case(text: &str, out: &mut Vec<String>) {
    let mut current = String::new();
    let mut prev_lower = false;

    for ch in text.chars() {
        if ch.is_uppercase() && prev_lower {
            if !current.is_empty() {
                out.push(current.to_lowercase());
                current.clear();
            }
        }
        current.push(ch);
        prev_lower = ch.is_lowercase();
    }
    if !current.is_empty() {
        out.push(current.to_lowercase());
    }
}

// ---------------------------------------------------------------------------
// Problem query aggregation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct QueryProblem {
    pub representative_query: String,
    pub variants: Vec<String>,
    pub failure_count: usize,
    pub stall_count: usize,
    pub reformulation_count: usize,
    pub last_seen: i64,
    pub last_seen_display: String,
    pub avg_top_score: Option<f32>,
    pub avg_result_count: Option<f32>,
    pub triage_signal: String,
}

pub fn aggregate_problems(episodes: &[SearchEpisode]) -> Vec<QueryProblem> {
    let mut groups: HashMap<Vec<String>, ProblemGroup> = HashMap::new();

    for episode in episodes {
        let is_stalled = episode.outcome == "stalled";
        let is_reformulated = episode.outcome == "reformulation_converged";
        if !is_stalled && !is_reformulated {
            continue;
        }

        let candidate_queries: &[SearchEpisodeQuery] = if is_reformulated && episode.queries.len() > 1 {
            &episode.queries[..episode.queries.len() - 1]
        } else {
            &episode.queries
        };

        let mut counted_groups: std::collections::HashSet<Vec<String>> =
            std::collections::HashSet::new();

        for query in candidate_queries {
            let key = canonical_key(&query.query);
            if key.is_empty() {
                continue;
            }
            let group = groups.entry(key.clone()).or_insert_with(|| ProblemGroup {
                queries: HashMap::new(),
                stall_count: 0,
                reformulation_count: 0,
                failure_count: 0,
                last_seen: 0,
                scores: Vec::new(),
                result_counts: Vec::new(),
                ranking_hits: 0,
                recall_misses: 0,
            });

            *group.queries.entry(query.query.clone()).or_insert(0) += 1;
            if let Some(score) = query.top_hit_score {
                group.scores.push(score);
            }
            if let Some(count) = query.result_count {
                group.result_counts.push(count);
            }
            group.last_seen = group.last_seen.max(query.timestamp);

            if is_reformulated {
                if let Some(target_name) = &episode.target_symbol_name {
                    let name_matches = query.top_hit_name.as_ref() == Some(target_name);
                    let file_matches = match (&query.top_hit_file, &episode.target_file_path) {
                        (Some(hit_file), Some(target_file)) => hit_file == target_file,
                        _ => true,
                    };
                    if name_matches && file_matches {
                        group.ranking_hits += 1;
                    } else {
                        group.recall_misses += 1;
                    }
                }
            }

            if counted_groups.insert(key) {
                group.failure_count += 1;
                if is_stalled {
                    group.stall_count += 1;
                } else {
                    group.reformulation_count += 1;
                }
            }
        }
    }

    let mut problems: Vec<QueryProblem> = groups
        .into_values()
        .map(|group| {
            let (representative, variants) = group.representative_and_variants();
            let avg_top_score = if group.scores.is_empty() {
                None
            } else {
                Some(group.scores.iter().sum::<f32>() / group.scores.len() as f32)
            };
            let avg_result_count = if group.result_counts.is_empty() {
                None
            } else {
                Some(
                    group.result_counts.iter().sum::<usize>() as f32
                        / group.result_counts.len() as f32,
                )
            };
            let triage_signal = if group.ranking_hits == 0 && group.recall_misses == 0 {
                "unknown"
            } else if group.ranking_hits > 0 && group.recall_misses == 0 {
                "ranking_problem"
            } else if group.recall_misses > 0 && group.ranking_hits == 0 {
                "recall_gap"
            } else {
                "mixed"
            };

            QueryProblem {
                representative_query: representative,
                variants,
                failure_count: group.failure_count,
                stall_count: group.stall_count,
                reformulation_count: group.reformulation_count,
                last_seen: group.last_seen,
                last_seen_display: format_relative_time(group.last_seen),
                avg_top_score,
                avg_result_count,
                triage_signal: triage_signal.to_string(),
            }
        })
        .collect();

    problems.sort_by(|a, b| b.failure_count.cmp(&a.failure_count));
    problems.truncate(20);
    problems
}

struct ProblemGroup {
    queries: HashMap<String, usize>,
    stall_count: usize,
    reformulation_count: usize,
    failure_count: usize,
    last_seen: i64,
    scores: Vec<f32>,
    result_counts: Vec<usize>,
    ranking_hits: usize,
    recall_misses: usize,
}

impl ProblemGroup {
    fn representative_and_variants(&self) -> (String, Vec<String>) {
        let mut sorted: Vec<_> = self.queries.iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(a.1));
        let representative = sorted
            .first()
            .map(|(q, _)| (*q).clone())
            .unwrap_or_default();
        let variants: Vec<String> = sorted
            .iter()
            .skip(1)
            .map(|(q, _)| (*q).clone())
            .collect();
        (representative, variants)
    }
}

// ---------------------------------------------------------------------------
// Reformulation pair extraction
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct ReformulationPair {
    pub initial_query: String,
    pub successful_query: String,
    pub target_name: Option<String>,
    pub target_file: Option<String>,
    pub occurrences: usize,
}

pub fn extract_reformulation_pairs(episodes: &[SearchEpisode]) -> Vec<ReformulationPair> {
    let mut pair_map: HashMap<(Vec<String>, Vec<String>), ReformulationPairBuilder> =
        HashMap::new();

    for episode in episodes {
        if episode.outcome != "reformulation_converged" || episode.queries.len() < 2 {
            continue;
        }

        for window in episode.queries.windows(2) {
            let initial_key = canonical_key(&window[0].query);
            let successful_key = canonical_key(&window[1].query);
            if initial_key.is_empty() || successful_key.is_empty() {
                continue;
            }
            if !pair_queries_overlap(&window[0].normalized_query, &window[1].normalized_query) {
                continue;
            }

            let map_key = (initial_key, successful_key);
            let entry = pair_map.entry(map_key).or_insert_with(|| {
                ReformulationPairBuilder {
                    initial_query: window[0].query.clone(),
                    successful_query: window[1].query.clone(),
                    target_name: episode.target_symbol_name.clone(),
                    target_file: episode.target_file_path.clone(),
                    occurrences: 0,
                }
            });
            entry.occurrences += 1;
        }
    }

    let mut pairs: Vec<ReformulationPair> = pair_map
        .into_values()
        .map(|b| ReformulationPair {
            initial_query: b.initial_query,
            successful_query: b.successful_query,
            target_name: b.target_name,
            target_file: b.target_file,
            occurrences: b.occurrences,
        })
        .collect();

    pairs.sort_by(|a, b| b.occurrences.cmp(&a.occurrences));
    pairs.truncate(15);
    pairs
}

struct ReformulationPairBuilder {
    initial_query: String,
    successful_query: String,
    target_name: Option<String>,
    target_file: Option<String>,
    occurrences: usize,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub fn has_trace_data(episode: &SearchEpisode) -> bool {
    episode.queries.iter().any(|q| q.strategy.is_some())
}

fn format_relative_time(unix_ts: i64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let delta = now - unix_ts;
    if delta < 0 {
        return "just now".to_string();
    }
    let minutes = delta / 60;
    let hours = delta / 3600;
    let days = delta / 86400;
    if minutes < 1 {
        "just now".to_string()
    } else if minutes < 60 {
        format!("{}m ago", minutes)
    } else if hours < 24 {
        format!("{}h ago", hours)
    } else {
        format!("{}d ago", days)
    }
}
