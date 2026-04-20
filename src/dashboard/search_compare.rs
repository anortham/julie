use anyhow::Result;
use serde::Serialize;

use crate::daemon::database::{
    SearchCompareCaseInput, SearchCompareCaseRow, SearchCompareRunInput, SearchCompareRunRow,
};
use crate::dashboard::AppState;
use crate::dashboard::routes::projects_actions::{
    cleanup_dashboard_anchor, dashboard_handler, disconnect_dashboard_attached_workspaces,
};
use crate::dashboard::search_analysis::{SearchEpisode, analyze_tool_calls, episode_stats};
use crate::search::index::SearchFilter;
use crate::tools::search::execution::{self, SearchExecutionWorkspace};
use crate::tools::search::trace::{SearchExecutionResult, SearchHit};

const BASELINE_STRATEGY: &str = "shared_current";
const CANDIDATE_STRATEGY: &str = "legacy_direct";

#[derive(Debug, Clone, Serialize)]
pub struct SearchCompareCaseView {
    pub session_id: String,
    pub workspace_id: String,
    pub query: String,
    pub search_target: String,
    pub expected_symbol_name: Option<String>,
    pub expected_file_path: Option<String>,
    pub baseline_rank: Option<i64>,
    pub candidate_rank: Option<i64>,
    pub baseline_top_hit: Option<String>,
    pub candidate_top_hit: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchCompareView {
    pub runs: Vec<SearchCompareRunRow>,
    pub selected_run: Option<SearchCompareRunRow>,
    pub cases: Vec<SearchCompareCaseRow>,
}

pub async fn run_compare(state: &AppState, days: u32) -> Result<SearchCompareView> {
    let daemon_db = state
        .dashboard
        .daemon_db()
        .ok_or_else(|| anyhow::anyhow!("daemon database unavailable"))?;
    let rows = daemon_db.list_tool_calls_for_search_analysis(days)?;
    let episodes = analyze_tool_calls(&rows);
    let corpus = episodes
        .iter()
        .filter_map(compare_case_from_episode)
        .collect::<Vec<_>>();
    let stats = episode_stats(&episodes);

    let (handler, _anchor_dir, anchor_id) = dashboard_handler(state).await?;
    let mut persisted_cases = Vec::new();
    let mut baseline_top1_hits = 0i64;
    let mut candidate_top1_hits = 0i64;
    let mut baseline_top3_hits = 0i64;
    let mut candidate_top3_hits = 0i64;
    let mut baseline_source_wins = 0i64;
    let mut candidate_source_wins = 0i64;

    for case in corpus {
        let shared = execute_shared_current(state, &handler, &case).await?;
        let legacy = execute_legacy_direct(state, &case).await?;

        let baseline_rank = expected_rank(&shared.hits, &case);
        let candidate_rank = expected_rank(&legacy, &case);
        if baseline_rank == Some(1) {
            baseline_top1_hits += 1;
        }
        if candidate_rank == Some(1) {
            candidate_top1_hits += 1;
        }
        if baseline_rank.is_some_and(|rank| rank <= 3) {
            baseline_top3_hits += 1;
        }
        if candidate_rank.is_some_and(|rank| rank <= 3) {
            candidate_top3_hits += 1;
        }
        if shared.hits.first().is_some_and(|hit| is_source_hit(hit)) {
            baseline_source_wins += 1;
        }
        if legacy.first().is_some_and(|hit| is_source_hit(hit)) {
            candidate_source_wins += 1;
        }

        persisted_cases.push(SearchCompareCaseInput {
            session_id: case.session_id,
            workspace_id: case.workspace_id,
            query: case.query,
            search_target: case.search_target,
            expected_symbol_name: case.expected_symbol_name,
            expected_file_path: case.expected_file_path,
            baseline_rank: baseline_rank.map(|rank| rank as i64),
            candidate_rank: candidate_rank.map(|rank| rank as i64),
            baseline_top_hit: shared.hits.first().map(hit_label),
            candidate_top_hit: legacy.first().map(hit_label),
        });
    }

    disconnect_dashboard_attached_workspaces(state, &handler).await;
    cleanup_dashboard_anchor(state, &anchor_id).await;

    let run_id = daemon_db.insert_search_compare_run(&SearchCompareRunInput {
        baseline_strategy: BASELINE_STRATEGY.to_string(),
        candidate_strategy: CANDIDATE_STRATEGY.to_string(),
        case_count: persisted_cases.len() as i64,
        baseline_top1_hits,
        candidate_top1_hits,
        baseline_top3_hits,
        candidate_top3_hits,
        baseline_source_wins,
        candidate_source_wins,
        convergence_rate: Some(stats.convergence_rate),
        stall_rate: Some(stats.stall_rate),
    })?;
    daemon_db.replace_search_compare_cases(run_id, &persisted_cases)?;

    latest_compare_view(state, 10, Some(run_id))
}

pub fn latest_compare_view(
    state: &AppState,
    limit: u32,
    selected_run_id: Option<i64>,
) -> Result<SearchCompareView> {
    let Some(daemon_db) = state.dashboard.daemon_db() else {
        return Ok(SearchCompareView {
            runs: Vec::new(),
            selected_run: None,
            cases: Vec::new(),
        });
    };
    let runs = daemon_db.list_search_compare_runs(limit)?;
    let chosen_run_id = selected_run_id.or_else(|| runs.first().map(|run| run.id));
    let selected_run = runs
        .iter()
        .find(|run| Some(run.id) == chosen_run_id)
        .cloned();
    let cases = chosen_run_id
        .map(|run_id| daemon_db.list_search_compare_cases(run_id))
        .transpose()?
        .unwrap_or_default();

    Ok(SearchCompareView {
        runs,
        selected_run,
        cases,
    })
}

#[derive(Clone)]
struct CompareCase {
    session_id: String,
    workspace_id: String,
    query: String,
    search_target: String,
    expected_symbol_name: Option<String>,
    expected_file_path: Option<String>,
}

fn compare_case_from_episode(episode: &SearchEpisode) -> Option<CompareCase> {
    let first_query = episode.queries.first()?;
    if episode.target_symbol_name.is_none() && episode.target_file_path.is_none() {
        return None;
    }

    Some(CompareCase {
        session_id: episode.session_id.clone(),
        workspace_id: episode.workspace_id.clone(),
        query: first_query.query.clone(),
        search_target: first_query.search_target.clone(),
        expected_symbol_name: episode.target_symbol_name.clone(),
        expected_file_path: episode.target_file_path.clone(),
    })
}

async fn execute_shared_current(
    _state: &AppState,
    handler: &crate::handler::JulieServerHandler,
    case: &CompareCase,
) -> Result<SearchExecutionResult> {
    execution::execute_search(
        execution::SearchExecutionParams {
            query: &case.query,
            language: &None,
            file_pattern: &None,
            limit: 10,
            search_target: &case.search_target,
            context_lines: None,
            exclude_tests: None,
        },
        &[SearchExecutionWorkspace::target(case.workspace_id.clone())],
        handler,
    )
    .await
}

async fn execute_legacy_direct(state: &AppState, case: &CompareCase) -> Result<Vec<SearchHit>> {
    let pool = state
        .dashboard
        .workspace_pool()
        .ok_or_else(|| anyhow::anyhow!("workspace pool unavailable"))?;
    let workspace = pool
        .get(&case.workspace_id)
        .await
        .ok_or_else(|| anyhow::anyhow!("workspace not attached"))?;
    let search_index = workspace
        .search_index
        .as_ref()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("search index unavailable"))?;
    let target = case.search_target.clone();
    let query = case.query.clone();
    let workspace_id = case.workspace_id.clone();
    tokio::task::spawn_blocking(move || -> Result<Vec<SearchHit>> {
        let index = search_index
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let filter = SearchFilter::default();
        if target == "content" {
            Ok(index
                .search_content(&query, &filter, 10)?
                .results
                .into_iter()
                .map(|result| SearchHit {
                    name: result
                        .file_path
                        .rsplit('/')
                        .next()
                        .unwrap_or(&result.file_path)
                        .to_string(),
                    file: result.file_path,
                    line: None,
                    kind: "file".to_string(),
                    language: result.language,
                    score: result.score,
                    snippet: None,
                    workspace: workspace_id.clone(),
                    symbol_id: None,
                    backing: crate::tools::search::trace::SearchHitBacking::LineMatch(
                        crate::tools::search::LineMatch {
                            file_path: String::new(),
                            line_number: 0,
                            line_content: String::new(),
                        },
                    ),
                })
                .collect())
        } else {
            Ok(index
                .search_symbols(&query, &filter, 10)?
                .results
                .into_iter()
                .map(|result| SearchHit {
                    name: result.name,
                    file: result.file_path,
                    line: Some(result.start_line),
                    kind: result.kind,
                    language: result.language,
                    score: result.score,
                    snippet: if result.signature.is_empty() {
                        if result.doc_comment.is_empty() {
                            None
                        } else {
                            Some(result.doc_comment)
                        }
                    } else {
                        Some(result.signature)
                    },
                    workspace: workspace_id.clone(),
                    symbol_id: Some(result.id),
                    backing: crate::tools::search::trace::SearchHitBacking::LineMatch(
                        crate::tools::search::LineMatch {
                            file_path: String::new(),
                            line_number: 0,
                            line_content: String::new(),
                        },
                    ),
                })
                .collect())
        }
    })
    .await?
}

fn expected_rank(hits: &[SearchHit], case: &CompareCase) -> Option<usize> {
    hits.iter()
        .position(|hit| {
            case.expected_symbol_name.as_ref().is_some_and(|expected| {
                hit.name == *expected || hit.symbol_id.as_deref() == Some(expected.as_str())
            }) || case
                .expected_file_path
                .as_ref()
                .is_some_and(|expected| hit.file == *expected)
        })
        .map(|idx| idx + 1)
}

fn is_source_hit(hit: &SearchHit) -> bool {
    let file = hit.file.to_lowercase();
    !(file.contains("/docs/")
        || file.contains("/test")
        || file.contains("/spec")
        || file.ends_with(".md"))
}

fn hit_label(hit: &SearchHit) -> String {
    format!("{} @ {}", hit.name, hit.file)
}
