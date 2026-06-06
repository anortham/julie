use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use anyhow::Result;
use julie::daemon::database::{DaemonDatabase, WorkspaceRow};
use julie::paths::DaemonPaths;
use serde::{Deserialize, Serialize};

use crate::cli::{Ablation, SearchMatrixCommand};
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
    /// Ablation label for this execution. Empty string means no ablation (baseline).
    /// Serde default keeps existing reports parseable when this field is absent.
    #[serde(default)]
    pub ablation_label: String,
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
        SearchMatrixCommand::Baseline {
            profile,
            out,
            ablation,
        } => {
            let daemon_paths = DaemonPaths::new();
            let cases_path =
                workspace_root().join("fixtures/search-quality/search-matrix-cases.toml");
            let corpus_path =
                workspace_root().join("fixtures/search-quality/search-matrix-corpus.toml");
            let ablation_label = ablation.label();
            let out_path = out.clone().unwrap_or_else(|| {
                let filename = if ablation_label.is_empty() {
                    format!("{profile}-baseline.json")
                } else {
                    format!("{profile}-baseline-{ablation_label}.json")
                };
                workspace_root()
                    .join("artifacts")
                    .join("search-matrix")
                    .join(filename)
            });
            let report = run_search_matrix_baseline_with_home(
                &daemon_paths.julie_home(),
                &cases_path,
                &corpus_path,
                profile,
                &out_path,
                ablation,
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
    ablation: &Ablation,
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
        ablation,
    ))?;
    write_baseline_report(&report, out_path)?;
    Ok(report)
}

/// Pool-backed search matrix (Phase 3d.2b-ii: WorkspacePool deleted).
/// Returns error — restore in Phase 3d.3 when dashboard gets standalone registry.
async fn run_baseline_async(
    _daemon_paths: DaemonPaths,
    _cases: &SearchMatrixCaseSet,
    _corpus: &SearchMatrixCorpus,
    _profile: &str,
    _workspaces: &[WorkspaceRow],
    _ablation: &Ablation,
) -> Result<SearchMatrixBaselineReport> {
    anyhow::bail!(
        "search matrix requires WorkspacePool which was removed in Phase 3d.2b; \
         restore in Phase 3d.3"
    )
}

// Baseline-runner helpers below are dormant: `run_baseline_async` is stubbed
// (WorkspacePool removed in Phase 3d.2b) and is rewired against the registry in
// Phase 3d.3, which revives these. Kept, not deleted — most are
// registry-independent corpus/case logic that 3d.3 reuses verbatim.
#[allow(dead_code)]
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

#[allow(dead_code)]
fn resolve_repo_root(corpus: &SearchMatrixCorpus, repo_name: &str) -> Option<PathBuf> {
    corpus
        .roots
        .iter()
        .map(|root| expand_search_root(root))
        .map(|root| root.join(repo_name))
        .find(|candidate| candidate.is_dir())
}

#[allow(dead_code)]
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

#[allow(dead_code)]
fn find_workspace_row<'a>(
    workspaces: &'a [WorkspaceRow],
    repo_root: &Path,
) -> Option<&'a WorkspaceRow> {
    let target_root = normalize_repo_root(repo_root);
    workspaces
        .iter()
        .find(|workspace| normalize_repo_root(Path::new(&workspace.path)) == target_root)
}

#[allow(dead_code)]
fn normalize_repo_root(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[allow(dead_code)]
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
        let Some(case) = cases
            .cases
            .iter()
            .find(|case| case.case_id == execution.case_id)
        else {
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

#[allow(dead_code)]
fn push_flag(flags: &mut Vec<String>, flag: &str) {
    if !flags.iter().any(|existing| existing == flag) {
        flags.push(flag.to_string());
    }
}

#[allow(dead_code)]
fn enum_label<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|json| json.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "unknown".to_string())
}
