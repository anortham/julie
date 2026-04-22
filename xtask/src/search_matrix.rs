use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Result, anyhow};
use julie::daemon::database::{DaemonDatabase, WorkspaceRow};
use julie::daemon::workspace_pool::WorkspacePool;
use julie::handler::JulieServerHandler;
use julie::paths::DaemonPaths;
use julie::tools::search::FastSearchTool;
use serde::{Deserialize, Serialize};

use crate::cli::SearchMatrixCommand;
use crate::search_matrix_mine::mine_search_matrix_seed_report;
use crate::search_matrix_report::write_baseline_report;
use crate::workspace_root;

#[derive(Debug, Clone, Deserialize)]
pub struct SearchMatrixCaseSet {
    pub cases: Vec<SearchMatrixCase>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchMatrixCase {
    pub case_id: String,
    pub family: String,
    pub query: String,
    pub search_target: String,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub file_pattern: Option<String>,
    #[serde(default)]
    pub exclude_tests: Option<bool>,
    #[serde(default)]
    pub profile_tags: Vec<String>,
    #[serde(default)]
    pub repo_selector: Option<Vec<String>>,
    pub expected_mode: String,
    #[serde(default)]
    pub expected_value: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchMatrixCorpus {
    pub roots: Vec<String>,
    pub profiles: BTreeMap<String, SearchMatrixProfile>,
    pub repos: Vec<SearchMatrixRepo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchMatrixProfile {
    pub repos: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchMatrixRepo {
    pub name: String,
    pub language: String,
    #[serde(default)]
    pub profile_tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchMatrixBaselineReport {
    pub profile: String,
    pub executions: Vec<SearchMatrixBaselineExecution>,
    pub skipped_repos: Vec<SearchMatrixSkippedRepo>,
    pub summary_flags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchMatrixBaselineExecution {
    pub repo_name: String,
    pub workspace_id: String,
    pub case_id: String,
    pub family: String,
    pub search_target: String,
    pub hit_count: usize,
    pub hit_count_is_lower_bound: bool,
    pub relaxed: bool,
    pub zero_hit_reason: Option<String>,
    pub file_pattern_diagnostic: Option<String>,
    pub hint_kind: Option<String>,
    pub latency_ms: u128,
    pub top_hits: Vec<SearchMatrixTopHit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchMatrixTopHit {
    pub name: String,
    pub file: String,
    pub line: Option<u32>,
    pub kind: String,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchMatrixSkippedRepo {
    pub repo_name: String,
    pub reason: String,
}

impl SearchMatrixCaseSet {
    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)?;
        Ok(toml::from_str(&text)?)
    }
}

impl SearchMatrixCorpus {
    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)?;
        Ok(toml::from_str(&text)?)
    }
}

pub fn run_search_matrix_command(
    command: &SearchMatrixCommand,
    stdout: &mut dyn Write,
) -> Result<()> {
    match command {
        SearchMatrixCommand::Mine { days, out } => {
            let daemon_paths = DaemonPaths::new();
            let report = mine_search_matrix_seed_report(&daemon_paths.daemon_db(), *days, out)?;
            writeln!(
                stdout,
                "search-matrix mine wrote {} candidates across {} clusters to {}",
                report.candidates.len(),
                report.clusters.len(),
                out.display()
            )?;
        }
        SearchMatrixCommand::Baseline { profile, out } => {
            let daemon_paths = DaemonPaths::new();
            let cases_path = workspace_root().join("fixtures/search-quality/search-matrix-cases.toml");
            let corpus_path =
                workspace_root().join("fixtures/search-quality/search-matrix-corpus.toml");
            let out_path = out.clone().unwrap_or_else(|| {
                workspace_root()
                    .join("artifacts")
                    .join("search-matrix")
                    .join(format!("{profile}-baseline.json"))
            });
            let report = run_search_matrix_baseline_with_home(
                &daemon_paths.julie_home(),
                &cases_path,
                &corpus_path,
                profile,
                &out_path,
            )?;
            writeln!(
                stdout,
                "search-matrix baseline wrote {} executions and {} skipped repos to {}",
                report.executions.len(),
                report.skipped_repos.len(),
                out_path.display()
            )?;
        }
    }

    Ok(())
}

pub fn run_search_matrix_baseline_with_home(
    julie_home: &Path,
    cases_path: &Path,
    corpus_path: &Path,
    profile: &str,
    out_path: &Path,
) -> Result<SearchMatrixBaselineReport> {
    let cases = SearchMatrixCaseSet::load(cases_path)?;
    let corpus = SearchMatrixCorpus::load(corpus_path)?;
    let daemon_paths = DaemonPaths::with_home(julie_home.to_path_buf());
    let daemon_db = DaemonDatabase::open(&daemon_paths.daemon_db())?;
    let workspaces = daemon_db.list_workspaces()?;

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let report = runtime.block_on(run_baseline_async(
        daemon_paths,
        &cases,
        &corpus,
        profile,
        &workspaces,
    ))?;
    write_baseline_report(&report, out_path)?;
    Ok(report)
}

async fn run_baseline_async(
    daemon_paths: DaemonPaths,
    cases: &SearchMatrixCaseSet,
    corpus: &SearchMatrixCorpus,
    profile: &str,
    workspaces: &[WorkspaceRow],
) -> Result<SearchMatrixBaselineReport> {
    let profile_entry = corpus
        .profiles
        .get(profile)
        .ok_or_else(|| anyhow!("unknown search-matrix profile `{profile}`"))?;
    let pool = Arc::new(WorkspacePool::new(
        daemon_paths.indexes_dir(),
        None,
        None,
        None,
    ));

    let mut executions = Vec::new();
    let mut skipped_repos = Vec::new();

    for repo_name in &profile_entry.repos {
        let Some(repo_meta) = corpus.repos.iter().find(|repo| &repo.name == repo_name) else {
            skipped_repos.push(SearchMatrixSkippedRepo {
                repo_name: repo_name.clone(),
                reason: "missing repo metadata".to_string(),
            });
            continue;
        };

        if !repo_meta.profile_tags.iter().any(|tag| tag == profile) {
            skipped_repos.push(SearchMatrixSkippedRepo {
                repo_name: repo_name.clone(),
                reason: format!("repo metadata does not include profile `{profile}`"),
            });
            continue;
        }

        let Some(repo_root) = resolve_repo_root(corpus, repo_name) else {
            skipped_repos.push(SearchMatrixSkippedRepo {
                repo_name: repo_name.clone(),
                reason: "repo not found under configured search roots".to_string(),
            });
            continue;
        };

        let Some(workspace_row) = find_workspace_row(workspaces, &repo_root) else {
            skipped_repos.push(SearchMatrixSkippedRepo {
                repo_name: repo_name.clone(),
                reason: "no matching daemon workspace for resolved repo root".to_string(),
            });
            continue;
        };

        if !workspace_row.status.eq_ignore_ascii_case("ready") {
            skipped_repos.push(SearchMatrixSkippedRepo {
                repo_name: repo_name.clone(),
                reason: format!("workspace status is {}", workspace_row.status),
            });
            continue;
        }

        let workspace = pool
            .get_or_init(&workspace_row.workspace_id, repo_root.clone())
            .await?;
        let handler = JulieServerHandler::new_with_shared_workspace(
            workspace,
            repo_root,
            None,
            Some(workspace_row.workspace_id.clone()),
            None,
            None,
            None,
            None,
            Some(Arc::clone(&pool)),
        )
        .await?;

        for case in eligible_cases_for_repo(cases, profile, repo_meta) {
            let started_at = Instant::now();
            let search_limit = 10usize;
            let execution = FastSearchTool {
                query: case.query.clone(),
                search_target: case.search_target.clone(),
                language: case.language.clone(),
                file_pattern: case.file_pattern.clone(),
                limit: search_limit as u32,
                context_lines: None,
                exclude_tests: case.exclude_tests,
                workspace: Some(workspace_row.workspace_id.clone()),
                return_format: "locations".to_string(),
            }
            .execute_with_trace(&handler)
            .await?;
            let execution = execution
                .execution
                .ok_or_else(|| anyhow!("search-matrix baseline received no execution trace"))?;
            let hit_count_is_lower_bound =
                case.search_target == "content" && execution.hits.len() >= search_limit;
            let hit_count = if case.search_target == "content" {
                execution.hits.len()
            } else {
                execution.total_results
            };

            executions.push(SearchMatrixBaselineExecution {
                repo_name: repo_name.clone(),
                workspace_id: workspace_row.workspace_id.clone(),
                case_id: case.case_id.clone(),
                family: case.family.clone(),
                search_target: case.search_target.clone(),
                hit_count,
                hit_count_is_lower_bound,
                relaxed: execution.relaxed,
                zero_hit_reason: execution.trace.zero_hit_reason.as_ref().map(enum_label),
                file_pattern_diagnostic: execution
                    .trace
                    .file_pattern_diagnostic
                    .as_ref()
                    .map(enum_label),
                hint_kind: execution.trace.hint_kind.as_ref().map(enum_label),
                latency_ms: started_at.elapsed().as_millis(),
                top_hits: execution
                    .hits
                    .iter()
                    .take(3)
                    .map(|hit| SearchMatrixTopHit {
                        name: hit.name.clone(),
                        file: hit.file.clone(),
                        line: hit.line,
                        kind: hit.kind.clone(),
                        score: hit.score,
                    })
                    .collect(),
            });
        }
    }

    let summary_flags = compute_summary_flags(&executions, cases);
    Ok(SearchMatrixBaselineReport {
        profile: profile.to_string(),
        executions,
        skipped_repos,
        summary_flags,
    })
}

fn eligible_cases_for_repo<'a>(
    cases: &'a SearchMatrixCaseSet,
    profile: &str,
    repo: &SearchMatrixRepo,
) -> Vec<&'a SearchMatrixCase> {
    cases
        .cases
        .iter()
        .filter(|case| case.profile_tags.iter().any(|tag| tag == profile))
        .filter(|case| {
            case.language
                .as_ref()
                .is_none_or(|language| language == &repo.language)
        })
        .filter(|case| {
            case.repo_selector
                .as_ref()
                .is_none_or(|selectors| selectors.iter().any(|selector| selector == &repo.name))
        })
        .collect()
}

fn resolve_repo_root(corpus: &SearchMatrixCorpus, repo_name: &str) -> Option<PathBuf> {
    corpus
        .roots
        .iter()
        .map(|root| expand_search_root(root))
        .map(|root| root.join(repo_name))
        .find(|candidate| candidate.is_dir())
}

fn expand_search_root(root: &str) -> PathBuf {
    if root == "~" {
        return std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(root));
    }

    if let Some(suffix) = root.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(suffix);
        }
    }

    PathBuf::from(root)
}

fn find_workspace_row<'a>(workspaces: &'a [WorkspaceRow], repo_root: &Path) -> Option<&'a WorkspaceRow> {
    let target_root = normalize_repo_root(repo_root);
    workspaces.iter().find(|workspace| {
        normalize_repo_root(Path::new(&workspace.path)) == target_root
    })
}

fn normalize_repo_root(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn compute_summary_flags(
    executions: &[SearchMatrixBaselineExecution],
    cases: &SearchMatrixCaseSet,
) -> Vec<String> {
    let mut flags = Vec::new();
    let mut zero_hit_cases: BTreeMap<&str, usize> = BTreeMap::new();

    for execution in executions {
        if execution.hit_count == 0 {
            *zero_hit_cases.entry(&execution.case_id).or_default() += 1;
        }
        if execution.hit_count == 0
            && execution.zero_hit_reason.is_none()
            && execution.file_pattern_diagnostic.is_none()
            && execution.hint_kind.is_none()
        {
            push_flag(&mut flags, "unattributed_zero_hit");
        }
        if execution.zero_hit_reason.as_deref() == Some("line_match_miss") {
            push_flag(&mut flags, "line_match_miss_cluster");
        }
        if execution.file_pattern_diagnostic.as_deref() == Some("no_in_scope_candidates") {
            push_flag(&mut flags, "scoped_no_in_scope_cluster");
        }
    }

    if zero_hit_cases.values().any(|count| *count > 1) {
        push_flag(&mut flags, "cross_repo_zero_hit");
    }

    for execution in executions {
        let Some(case) = cases.cases.iter().find(|case| case.case_id == execution.case_id) else {
            continue;
        };
        if case.expected_mode == "expect_hint_kind"
            && case.expected_value.as_deref() != execution.hint_kind.as_deref()
        {
            push_flag(&mut flags, "unexpected_hint");
        }
        if case.expected_mode == "expect_zero_hit_reason"
            && case.expected_value.as_deref() != execution.zero_hit_reason.as_deref()
        {
            push_flag(&mut flags, "unexpected_hint");
        }
        if case.expected_mode == "expect_hits" && execution.hit_count == 0 {
            push_flag(&mut flags, "expected_hits_missing");
        }
    }

    flags
}

fn push_flag(flags: &mut Vec<String>, flag: &str) {
    if !flags.iter().any(|existing| existing == flag) {
        flags.push(flag.to_string());
    }
}

fn enum_label<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|json| json.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "unknown".to_string())
}
