use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use julie::daemon::database::DaemonDatabase;
use julie::daemon::workspace_pool::WorkspacePool;
use julie::handler::JulieServerHandler;
use julie::paths::DaemonPaths;
use julie::tools::workspace::ManageWorkspaceTool;
use julie::workspace::registry::generate_workspace_id;
use julie::workspace::startup_hint::WorkspaceStartupHint;
use rusqlite::{Connection, params};
use serde::Deserialize;

use crate::tree_sitter_certification::current_git_head_sha;
pub use crate::tree_sitter_real_world_report::{
    TreeSitterRealWorldEvidenceReport, TreeSitterRealWorldRepoEvidence,
    TreeSitterRealWorldSkippedRepo, load_tree_sitter_real_world_evidence, write_real_world_report,
};
use crate::workspace_root;

mod representative_specs;
pub use representative_specs::RepresentativeSpec;
use representative_specs::representative_spec_failures;

pub const DEFAULT_TREE_SITTER_REAL_WORLD_CORPUS: &str =
    "fixtures/extraction/tree-sitter-real-world-corpus.toml";
pub const DEFAULT_TREE_SITTER_REAL_WORLD_EVIDENCE: &str = "docs/LANGUAGE_REAL_WORLD_EVIDENCE.json";
pub const DEFAULT_TREE_SITTER_REAL_WORLD_HOME: &str =
    "artifacts/tree-sitter-certification/julie-home";

#[derive(Debug, Clone, Deserialize)]
pub struct TreeSitterRealWorldCorpus {
    pub roots: Vec<String>,
    pub profiles: BTreeMap<String, TreeSitterRealWorldProfile>,
    pub repos: Vec<TreeSitterRealWorldRepo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TreeSitterRealWorldProfile {
    pub repos: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TreeSitterRealWorldRepo {
    pub name: String,
    pub language: String,
    #[serde(default)]
    pub profile_tags: Vec<String>,
    #[serde(default = "default_min_one")]
    pub min_files: i64,
    #[serde(default = "default_min_one")]
    pub min_language_files: i64,
    #[serde(default = "default_min_one")]
    pub min_symbols: i64,
    #[serde(default)]
    pub min_relationships: i64,
    #[serde(default)]
    pub max_parse_diagnostic_files: Option<i64>,
    #[serde(default)]
    pub representative_specs: Vec<RepresentativeSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RepoCounts {
    file_count: i64,
    language_file_count: i64,
    symbol_count: i64,
    relationship_count: i64,
    identifier_count: i64,
    type_count: i64,
    parse_diagnostic_file_count: i64,
}

impl TreeSitterRealWorldCorpus {
    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("failed to read real-world corpus at {}", path.display()))?;
        toml::from_str(&text)
            .with_context(|| format!("failed to parse real-world corpus at {}", path.display()))
    }
}

pub fn run_tree_sitter_real_world_certification(
    profile: &str,
    corpus: &Path,
    out: &Path,
    julie_home: &Path,
    stdout: &mut dyn Write,
) -> Result<()> {
    let root = workspace_root();
    let report = run_tree_sitter_real_world_with_head(
        current_git_head_sha(&root)?,
        julie_home,
        &resolve_path(&root, corpus),
        profile,
        &resolve_path(&root, out),
    )?;
    writeln!(
        stdout,
        "tree-sitter real-world evidence wrote {} verified and {} skipped repos to {}",
        report.verified_repos.len(),
        report.skipped_repos.len(),
        resolve_path(&root, out).display()
    )?;
    Ok(())
}

pub fn run_tree_sitter_real_world_with_head(
    julie_head: String,
    julie_home: &Path,
    corpus_path: &Path,
    profile: &str,
    out_path: &Path,
) -> Result<TreeSitterRealWorldEvidenceReport> {
    let corpus = TreeSitterRealWorldCorpus::load(corpus_path)?;
    fs::create_dir_all(julie_home)
        .with_context(|| format!("failed to create julie home {}", julie_home.display()))?;
    let daemon_paths = DaemonPaths::with_home(julie_home.to_path_buf());
    let daemon_db = Arc::new(DaemonDatabase::open(&daemon_paths.daemon_db())?);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let mut report = runtime.block_on(run_real_world_async(
        daemon_paths,
        Arc::clone(&daemon_db),
        &corpus,
        profile,
        julie_head,
        corpus_path,
    ))?;
    sort_report(&mut report);
    write_real_world_report(&report, out_path)?;
    Ok(report)
}

async fn run_real_world_async(
    daemon_paths: DaemonPaths,
    daemon_db: Arc<DaemonDatabase>,
    corpus: &TreeSitterRealWorldCorpus,
    profile: &str,
    julie_head: String,
    corpus_path: &Path,
) -> Result<TreeSitterRealWorldEvidenceReport> {
    let profile_entry = corpus
        .profiles
        .get(profile)
        .ok_or_else(|| anyhow!("unknown tree-sitter real-world profile `{profile}`"))?;
    let indexes_dir = daemon_paths.indexes_dir();
    let pool = Arc::new(WorkspacePool::new_isolated(
        indexes_dir.clone(),
        Some(Arc::clone(&daemon_db)),
    ));

    let mut verified_repos = Vec::new();
    let mut skipped_repos = Vec::new();

    for repo_name in &profile_entry.repos {
        let Some(repo) = corpus.repos.iter().find(|repo| &repo.name == repo_name) else {
            skipped_repos.push(TreeSitterRealWorldSkippedRepo {
                repo_name: repo_name.clone(),
                language: "unknown".to_string(),
                reason: "missing repo metadata".to_string(),
            });
            continue;
        };

        if !repo.profile_tags.iter().any(|tag| tag == profile) {
            skipped_repos.push(TreeSitterRealWorldSkippedRepo {
                repo_name: repo.name.clone(),
                language: repo.language.clone(),
                reason: format!("repo metadata does not include profile `{profile}`"),
            });
            continue;
        }

        let Some(repo_root) = resolve_repo_root(corpus, &repo.name) else {
            skipped_repos.push(TreeSitterRealWorldSkippedRepo {
                repo_name: repo.name.clone(),
                language: repo.language.clone(),
                reason: "repo not found under configured roots".to_string(),
            });
            continue;
        };

        verified_repos.push(
            match index_and_collect_repo(&pool, &daemon_db, &indexes_dir, corpus, repo, &repo_root)
                .await
            {
                Ok(evidence) => evidence,
                Err(error) => failed_repo_evidence(corpus, repo, &repo_root, error),
            },
        );
    }

    let mut summary_flags = Vec::new();
    if verified_repos.iter().any(|repo| repo.status == "fail") {
        summary_flags.push("real_world_repo_failed".to_string());
    }
    if !skipped_repos.is_empty() {
        summary_flags.push("real_world_repo_skipped".to_string());
    }

    Ok(TreeSitterRealWorldEvidenceReport {
        profile: profile.to_string(),
        julie_head,
        corpus_path: relativize_to_workspace(corpus_path),
        verified_repos,
        skipped_repos,
        summary_flags,
    })
}

async fn index_and_collect_repo(
    pool: &Arc<WorkspacePool>,
    daemon_db: &Arc<DaemonDatabase>,
    indexes_dir: &Path,
    corpus: &TreeSitterRealWorldCorpus,
    repo: &TreeSitterRealWorldRepo,
    repo_root: &Path,
) -> Result<TreeSitterRealWorldRepoEvidence> {
    let canonical_root = repo_root
        .canonicalize()
        .with_context(|| format!("failed to canonicalize {}", repo_root.display()))?;
    let workspace_id = generate_workspace_id(&canonical_root.to_string_lossy())?;
    let handler = JulieServerHandler::new_deferred_daemon_startup_hint_without_project_log(
        WorkspaceStartupHint {
            path: canonical_root.clone(),
            source: None,
        },
        Some(Arc::clone(daemon_db)),
        None,
        None,
        None,
        None,
        Some(Arc::clone(pool)),
    )
    .await?;

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(canonical_root.to_string_lossy().to_string()),
        force: Some(true),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    let result = index_tool.call_tool_with_options(&handler, true).await?;
    if result.is_error.unwrap_or(false) {
        return Err(anyhow!("index tool returned an error for {}", repo.name));
    }

    let db_path = indexes_dir
        .join(&workspace_id)
        .join("db")
        .join("symbols.db");
    let counts = collect_counts(&db_path, &repo.language)
        .with_context(|| format!("failed to collect database counts for {}", repo.name))?;
    let hard_failures = hard_failures(repo, &counts, &db_path);

    Ok(TreeSitterRealWorldRepoEvidence {
        repo_name: repo.name.clone(),
        language: repo.language.clone(),
        display_path: display_path(corpus, &repo.name),
        repo_head: git_head(&canonical_root),
        workspace_id,
        file_count: counts.file_count,
        language_file_count: counts.language_file_count,
        symbol_count: counts.symbol_count,
        relationship_count: counts.relationship_count,
        identifier_count: counts.identifier_count,
        type_count: counts.type_count,
        parse_diagnostic_file_count: counts.parse_diagnostic_file_count,
        status: if hard_failures.is_empty() {
            "pass".to_string()
        } else {
            "fail".to_string()
        },
        hard_failures,
    })
}

fn collect_counts(db_path: &Path, language: &str) -> Result<RepoCounts> {
    let conn = Connection::open(db_path)
        .with_context(|| format!("failed to open workspace db {}", db_path.display()))?;

    Ok(RepoCounts {
        file_count: count_all(&conn, "files")?,
        language_file_count: conn.query_row(
            "SELECT COUNT(*) FROM files WHERE language = ?1",
            params![language],
            |row| row.get(0),
        )?,
        symbol_count: count_all(&conn, "symbols")?,
        relationship_count: count_all(&conn, "relationships")?,
        identifier_count: count_all(&conn, "identifiers")?,
        type_count: count_all(&conn, "types")?,
        parse_diagnostic_file_count: conn.query_row(
            "SELECT COUNT(*) FROM files WHERE parse_cache IS NOT NULL",
            [],
            |row| row.get(0),
        )?,
    })
}

fn count_all(conn: &Connection, table: &str) -> Result<i64> {
    let sql = format!("SELECT COUNT(*) FROM {table}");
    Ok(conn.query_row(&sql, [], |row| row.get(0))?)
}

fn hard_failures(
    repo: &TreeSitterRealWorldRepo,
    counts: &RepoCounts,
    db_path: &Path,
) -> Vec<String> {
    let mut failures = Vec::new();
    if counts.file_count < repo.min_files {
        failures.push(format!(
            "expected at least {} indexed files, got {}",
            repo.min_files, counts.file_count
        ));
    }
    if counts.language_file_count < repo.min_language_files {
        failures.push(format!(
            "expected at least {} `{}` files, got {}",
            repo.min_language_files, repo.language, counts.language_file_count
        ));
    }
    if counts.symbol_count < repo.min_symbols {
        failures.push(format!(
            "expected at least {} symbols, got {}",
            repo.min_symbols, counts.symbol_count
        ));
    }
    if counts.relationship_count < repo.min_relationships {
        failures.push(format!(
            "expected at least {} relationships, got {}",
            repo.min_relationships, counts.relationship_count
        ));
    }
    if let Some(max) = repo.max_parse_diagnostic_files {
        if counts.parse_diagnostic_file_count > max {
            failures.push(format!(
                "expected at most {} files with parse diagnostics, got {}",
                max, counts.parse_diagnostic_file_count
            ));
        }
    }
    failures.extend(representative_spec_failures(repo, db_path));
    failures
}

fn failed_repo_evidence(
    corpus: &TreeSitterRealWorldCorpus,
    repo: &TreeSitterRealWorldRepo,
    repo_root: &Path,
    error: anyhow::Error,
) -> TreeSitterRealWorldRepoEvidence {
    TreeSitterRealWorldRepoEvidence {
        repo_name: repo.name.clone(),
        language: repo.language.clone(),
        display_path: display_path(corpus, &repo.name),
        repo_head: git_head(repo_root),
        workspace_id: String::new(),
        file_count: 0,
        language_file_count: 0,
        symbol_count: 0,
        relationship_count: 0,
        identifier_count: 0,
        type_count: 0,
        parse_diagnostic_file_count: 0,
        status: "fail".to_string(),
        hard_failures: vec![error.to_string()],
    }
}

fn sort_report(report: &mut TreeSitterRealWorldEvidenceReport) {
    report
        .verified_repos
        .sort_by(|left, right| left.repo_name.cmp(&right.repo_name));
    report
        .skipped_repos
        .sort_by(|left, right| left.repo_name.cmp(&right.repo_name));
    report.summary_flags.sort();
}

fn resolve_repo_root(corpus: &TreeSitterRealWorldCorpus, repo_name: &str) -> Option<PathBuf> {
    corpus
        .roots
        .iter()
        .map(|root| expand_search_root(root).join(repo_name))
        .find(|candidate| candidate.is_dir())
}

fn display_path(corpus: &TreeSitterRealWorldCorpus, repo_name: &str) -> String {
    corpus
        .roots
        .iter()
        .find(|root| expand_search_root(root).join(repo_name).is_dir())
        .map(|root| format!("{}/{}", root.trim_end_matches('/'), repo_name))
        .unwrap_or_else(|| repo_name.to_string())
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

fn git_head(repo_root: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn resolve_path(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn relativize_to_workspace(path: &Path) -> String {
    let root = workspace_root();
    path.strip_prefix(&root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}

fn default_min_one() -> i64 {
    1
}
