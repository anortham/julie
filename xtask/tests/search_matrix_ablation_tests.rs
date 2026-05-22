//! Integration tests for the ablation A/B bakeoff extension added in T4.
//!
//! # What these tests verify
//!
//! 1. The CLI accepts `--ablation <variant>` for all four variants and rejects invalid
//!    variants.
//! 2. `ablation_label` is present in each `SearchMatrixBaselineExecution` in the
//!    serialized JSON report, matching the variant that was passed.
//! 3. Different ablation labels applied to the same corpus produce execution records
//!    with different `ablation_label` values — i.e. the flag flows end-to-end from CLI
//!    through workspace setup to each per-execution report record.
//! 4. The env-guard hygiene: after `run_search_matrix_baseline_with_home` returns, the
//!    ablation env vars are not set (no leakage into subsequent runs).
//!
//! # Invariant proved
//!
//! "Ablation flag flows from CLI through workspace setup to per-execution report records,
//! with the correct env var set during reindex."
//!
//! Verified by: (b) the JSON contains `ablation_label` matching the input variant, and
//! (c) the two runs stamped with different labels are distinct in the output.

use std::fs;
use std::sync::Arc;

use julie::daemon::database::DaemonDatabase;
use julie::daemon::workspace_pool::WorkspacePool;
use julie::handler::JulieServerHandler;
use tempfile::TempDir;
use tokio::runtime::Builder;
use xtask::cli::{Ablation, CliCommand, SearchMatrixCommand, parse_cli_command};
use xtask::search_matrix::run_search_matrix_baseline_with_home;

#[path = "support/toml_fixture.rs"]
mod toml_fixture;

use toml_fixture::toml_roots_from_paths;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Minimal fixture: a single Rust source file containing a function whose name
/// is highly specific so it appears in search results unambiguously.
const FIXTURE_SRC: &str = r#"
/// A uniquely named function used as a search-matrix fixture.
pub fn ablation_needle_alpha() -> u32 {
    42
}

/// A second uniquely named function for camel-case ablation testing.
pub fn AblationNeedleBeta() -> u32 {
    99
}
"#;

/// Build a minimal indexed workspace in `julie_home`, write `FIXTURE_SRC` under
/// `repo_root/src/lib.rs`, and return after the index is complete.
fn setup_indexed_workspace(
    julie_home: &std::path::Path,
    repo_root: std::path::PathBuf,
    daemon_db: Arc<DaemonDatabase>,
) {
    fs::create_dir_all(repo_root.join("src")).expect("src dir");
    fs::write(repo_root.join("src/lib.rs"), FIXTURE_SRC).expect("write fixture src");

    let indexes_dir = julie_home.join("indexes");
    fs::create_dir_all(&indexes_dir).expect("indexes dir");

    let runtime = Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    runtime.block_on(async move {
        let pool = Arc::new(WorkspacePool::new(
            indexes_dir,
            Some(Arc::clone(&daemon_db)),
        ));
        let handler = JulieServerHandler::new_deferred_daemon_startup_hint(
            julie::workspace::startup_hint::WorkspaceStartupHint {
                path: repo_root.clone(),
                source: None,
            },
            Some(Arc::clone(&daemon_db)),
            None,
            None,
            None,
            None,
            Some(Arc::clone(&pool)),
        )
        .await
        .expect("daemon-aware handler");

        let index_tool = julie::tools::workspace::ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(repo_root.to_string_lossy().to_string()),
            force: Some(true),
            name: None,
            workspace_id: None,
            detailed: None,
        };
        index_tool
            .call_tool_with_options(&handler, true)
            .await
            .expect("index workspace");
    });
}

/// Write a minimal TOML cases file with one definition-search case.
fn write_cases_toml(path: &std::path::Path) {
    fs::write(
        path,
        r#"
[[cases]]
case_id = "ablation-needle-alpha"
family = "exact_identifier"
query = "ablation_needle_alpha"
search_target = "definitions"
profile_tags = ["ablation-smoke"]
expected_mode = "expect_hits"
"#,
    )
    .expect("write cases toml");
}

/// Write a minimal TOML corpus file referencing `source_root`.
fn write_corpus_toml(path: &std::path::Path, source_root: &std::path::Path) {
    let roots = toml_roots_from_paths(&[source_root]);
    fs::write(
        path,
        format!(
            r#"
{roots}

[profiles.ablation-smoke]
repos = ["ablation-fixture-repo"]

[[repos]]
name = "ablation-fixture-repo"
language = "rust"
profile_tags = ["ablation-smoke"]
"#
        ),
    )
    .expect("write corpus toml");
}

// ---------------------------------------------------------------------------
// (a) CLI accepts --ablation for all four variants
// ---------------------------------------------------------------------------

#[test]
fn search_matrix_ablation_tests_cli_accepts_ablation_none() {
    let parsed = parse_cli_command([
        "xtask",
        "search-matrix",
        "baseline",
        "--profile",
        "smoke",
        "--ablation",
        "none",
    ])
    .expect("ablation=none should parse");

    assert_eq!(
        parsed,
        CliCommand::SearchMatrix(SearchMatrixCommand::Baseline {
            profile: "smoke".to_string(),
            out: None,
            ablation: Ablation::None,
        })
    );
}

#[test]
fn search_matrix_ablation_tests_cli_accepts_ablation_no_stemming() {
    let parsed = parse_cli_command([
        "xtask",
        "search-matrix",
        "baseline",
        "--profile",
        "smoke",
        "--ablation",
        "no-stemming",
    ])
    .expect("ablation=no-stemming should parse");

    assert_eq!(
        parsed,
        CliCommand::SearchMatrix(SearchMatrixCommand::Baseline {
            profile: "smoke".to_string(),
            out: None,
            ablation: Ablation::NoStemming,
        })
    );
}

#[test]
fn search_matrix_ablation_tests_cli_accepts_ablation_no_camel() {
    let parsed = parse_cli_command([
        "xtask",
        "search-matrix",
        "baseline",
        "--profile",
        "smoke",
        "--ablation",
        "no-camel",
    ])
    .expect("ablation=no-camel should parse");

    assert_eq!(
        parsed,
        CliCommand::SearchMatrix(SearchMatrixCommand::Baseline {
            profile: "smoke".to_string(),
            out: None,
            ablation: Ablation::NoCamel,
        })
    );
}

#[test]
fn search_matrix_ablation_tests_cli_accepts_ablation_both() {
    let parsed = parse_cli_command([
        "xtask",
        "search-matrix",
        "baseline",
        "--profile",
        "smoke",
        "--ablation",
        "both",
    ])
    .expect("ablation=both should parse");

    assert_eq!(
        parsed,
        CliCommand::SearchMatrix(SearchMatrixCommand::Baseline {
            profile: "smoke".to_string(),
            out: None,
            ablation: Ablation::Both,
        })
    );
}

#[test]
fn search_matrix_ablation_tests_cli_rejects_invalid_ablation_variant() {
    let error = parse_cli_command([
        "xtask",
        "search-matrix",
        "baseline",
        "--profile",
        "smoke",
        "--ablation",
        "turbo-boost",
    ])
    .unwrap_err();

    assert!(
        error.to_string().contains("invalid ablation variant"),
        "expected 'invalid ablation variant' in error, got: {error}"
    );
}

#[test]
fn search_matrix_ablation_tests_cli_rejects_ablation_for_mine() {
    let error = parse_cli_command([
        "xtask",
        "search-matrix",
        "mine",
        "--days",
        "7",
        "--out",
        "artifacts/seeds.json",
        "--ablation",
        "none",
    ])
    .unwrap_err();

    assert!(
        error.to_string().contains("`--ablation` is not valid"),
        "expected '--ablation is not valid' in error, got: {error}"
    );
}

// ---------------------------------------------------------------------------
// (b) ablation_label present in JSON output
// ---------------------------------------------------------------------------

#[test]
fn search_matrix_ablation_tests_report_contains_ablation_label_baseline() {
    let temp_dir = TempDir::new().expect("temp dir");
    let julie_home = temp_dir.path().join("julie-home");
    let source_root = temp_dir.path().join("source");
    fs::create_dir_all(&julie_home).expect("julie home");
    fs::create_dir_all(&source_root).expect("source root");

    let repo_root = source_root.join("ablation-fixture-repo");
    let daemon_db = Arc::new(
        DaemonDatabase::open(&julie_home.join("daemon.db")).expect("open daemon db"),
    );
    setup_indexed_workspace(&julie_home, repo_root, Arc::clone(&daemon_db));

    let cases_path = temp_dir.path().join("cases.toml");
    let corpus_path = temp_dir.path().join("corpus.toml");
    write_cases_toml(&cases_path);
    write_corpus_toml(&corpus_path, &source_root);

    let out_path = temp_dir.path().join("baseline-report.json");
    let report = run_search_matrix_baseline_with_home(
        &julie_home,
        &cases_path,
        &corpus_path,
        "ablation-smoke",
        &out_path,
        &Ablation::None,
    )
    .expect("baseline run with ablation=none");

    // Every execution must have an ablation_label; for Ablation::None it is ""
    assert!(
        !report.executions.is_empty(),
        "expected at least one execution"
    );
    for exec in &report.executions {
        assert_eq!(
            exec.ablation_label, "",
            "ablation=none should produce empty label, got: {:?}",
            exec.ablation_label
        );
    }

    // Verify the label round-trips through JSON
    let json_text = fs::read_to_string(&out_path).expect("read report json");
    let parsed: serde_json::Value =
        serde_json::from_str(&json_text).expect("parse report json");
    let first_exec = &parsed["executions"][0];
    assert_eq!(
        first_exec["ablation_label"].as_str(),
        Some(""),
        "JSON ablation_label should be empty string for baseline"
    );
}

#[test]
fn search_matrix_ablation_tests_report_contains_ablation_label_no_stemming() {
    let temp_dir = TempDir::new().expect("temp dir");
    let julie_home = temp_dir.path().join("julie-home");
    let source_root = temp_dir.path().join("source");
    fs::create_dir_all(&julie_home).expect("julie home");
    fs::create_dir_all(&source_root).expect("source root");

    let repo_root = source_root.join("ablation-fixture-repo");
    let daemon_db = Arc::new(
        DaemonDatabase::open(&julie_home.join("daemon.db")).expect("open daemon db"),
    );
    setup_indexed_workspace(&julie_home, repo_root, Arc::clone(&daemon_db));

    let cases_path = temp_dir.path().join("cases.toml");
    let corpus_path = temp_dir.path().join("corpus.toml");
    write_cases_toml(&cases_path);
    write_corpus_toml(&corpus_path, &source_root);

    let out_path = temp_dir.path().join("ablation-no-stemming-report.json");
    let report = run_search_matrix_baseline_with_home(
        &julie_home,
        &cases_path,
        &corpus_path,
        "ablation-smoke",
        &out_path,
        &Ablation::NoStemming,
    )
    .expect("baseline run with ablation=no-stemming");

    assert!(
        !report.executions.is_empty(),
        "expected at least one execution"
    );
    for exec in &report.executions {
        assert_eq!(
            exec.ablation_label, "no-stemming",
            "ablation=no-stemming should produce label 'no-stemming', got: {:?}",
            exec.ablation_label
        );
    }

    // Round-trip through JSON
    let json_text = fs::read_to_string(&out_path).expect("read report json");
    let parsed: serde_json::Value =
        serde_json::from_str(&json_text).expect("parse report json");
    let first_exec = &parsed["executions"][0];
    assert_eq!(
        first_exec["ablation_label"].as_str(),
        Some("no-stemming"),
        "JSON ablation_label should be 'no-stemming'"
    );
}

// ---------------------------------------------------------------------------
// (c) Env-var hygiene: ablation env vars are not leaked after a run
// ---------------------------------------------------------------------------

#[test]
fn search_matrix_ablation_tests_env_vars_restored_after_run() {
    // Ensure the vars are not set before we begin.
    // SAFETY: single-threaded test; no parallel tokenizer construction in this binary.
    unsafe {
        std::env::remove_var("JULIE_ABLATE_STEMMING");
        std::env::remove_var("JULIE_ABLATE_CAMEL_EMIT");
    }

    let temp_dir = TempDir::new().expect("temp dir");
    let julie_home = temp_dir.path().join("julie-home");
    let source_root = temp_dir.path().join("source");
    fs::create_dir_all(&julie_home).expect("julie home");
    fs::create_dir_all(&source_root).expect("source root");

    let repo_root = source_root.join("ablation-fixture-repo");
    let daemon_db = Arc::new(
        DaemonDatabase::open(&julie_home.join("daemon.db")).expect("open daemon db"),
    );
    setup_indexed_workspace(&julie_home, repo_root, Arc::clone(&daemon_db));

    let cases_path = temp_dir.path().join("cases.toml");
    let corpus_path = temp_dir.path().join("corpus.toml");
    write_cases_toml(&cases_path);
    write_corpus_toml(&corpus_path, &source_root);

    let out_path = temp_dir.path().join("ablation-both-report.json");
    run_search_matrix_baseline_with_home(
        &julie_home,
        &cases_path,
        &corpus_path,
        "ablation-smoke",
        &out_path,
        &Ablation::Both,
    )
    .expect("ablation=both run completes");

    // After the run, both env vars must be restored to unset.
    assert!(
        std::env::var("JULIE_ABLATE_STEMMING").is_err(),
        "JULIE_ABLATE_STEMMING must be unset after ablation=both run"
    );
    assert!(
        std::env::var("JULIE_ABLATE_CAMEL_EMIT").is_err(),
        "JULIE_ABLATE_CAMEL_EMIT must be unset after ablation=both run"
    );
}

// ---------------------------------------------------------------------------
// (d) Two runs with different ablation labels produce distinct ablation_label
//     fields — proves the label flows end-to-end (the invariant).
// ---------------------------------------------------------------------------

#[test]
fn search_matrix_ablation_tests_different_labels_produce_distinct_execution_stamps() {
    let temp_dir = TempDir::new().expect("temp dir");
    let julie_home = temp_dir.path().join("julie-home");
    let source_root = temp_dir.path().join("source");
    fs::create_dir_all(&julie_home).expect("julie home");
    fs::create_dir_all(&source_root).expect("source root");

    let repo_root = source_root.join("ablation-fixture-repo");
    let daemon_db = Arc::new(
        DaemonDatabase::open(&julie_home.join("daemon.db")).expect("open daemon db"),
    );
    setup_indexed_workspace(&julie_home, repo_root, Arc::clone(&daemon_db));

    let cases_path = temp_dir.path().join("cases.toml");
    let corpus_path = temp_dir.path().join("corpus.toml");
    write_cases_toml(&cases_path);
    write_corpus_toml(&corpus_path, &source_root);

    // Run baseline (Ablation::None)
    let out_baseline = temp_dir.path().join("baseline.json");
    let baseline_report = run_search_matrix_baseline_with_home(
        &julie_home,
        &cases_path,
        &corpus_path,
        "ablation-smoke",
        &out_baseline,
        &Ablation::None,
    )
    .expect("baseline run");

    // Run no-stemming ablation
    let out_no_stem = temp_dir.path().join("no-stemming.json");
    let no_stem_report = run_search_matrix_baseline_with_home(
        &julie_home,
        &cases_path,
        &corpus_path,
        "ablation-smoke",
        &out_no_stem,
        &Ablation::NoStemming,
    )
    .expect("no-stemming run");

    // Both reports must have executions
    assert!(
        !baseline_report.executions.is_empty(),
        "baseline must have executions"
    );
    assert!(
        !no_stem_report.executions.is_empty(),
        "no-stemming must have executions"
    );

    // The ablation_label fields must differ — this is the invariant:
    // "the flag flows from CLI through workspace setup to per-execution records".
    let baseline_label = &baseline_report.executions[0].ablation_label;
    let no_stem_label = &no_stem_report.executions[0].ablation_label;

    assert_ne!(
        baseline_label, no_stem_label,
        "different ablation variants must produce different ablation_label values; \
         baseline={baseline_label:?}, no-stemming={no_stem_label:?}"
    );
    assert_eq!(baseline_label, "", "baseline label must be empty");
    assert_eq!(no_stem_label, "no-stemming", "no-stemming label must be 'no-stemming'");
}

// ---------------------------------------------------------------------------
// (e) Eager restore: a non-baseline ablation run must leave the on-disk
//     Tantivy compat marker reflecting BASELINE flags, not the ablated ones.
//
// Regression for codex pre-merge finding "Ablation runs rewrite the real
// daemon indexes": the harness forced an ablated reindex but never restored
// the baseline projection.  Recovery used to rely on the next
// `open_or_create_with_tokenizer` call detecting the compat-marker mismatch
// and recreating-then-reindexing the empty index — an expensive surprise
// for the maintainer who didn't know they had to pay it.  After this fix,
// the harness eagerly re-indexes each touched workspace with baseline env
// vars before returning, so the on-disk marker is back to baseline and the
// next daemon open is a fast `Compatible` path.
// ---------------------------------------------------------------------------

#[test]
fn search_matrix_ablation_tests_eager_restore_returns_index_to_baseline() {
    use std::path::PathBuf;

    // Ensure the env vars are not set before we begin.
    // SAFETY: single-threaded test; no parallel tokenizer construction here.
    unsafe {
        std::env::remove_var("JULIE_ABLATE_STEMMING");
        std::env::remove_var("JULIE_ABLATE_CAMEL_EMIT");
    }

    let temp_dir = TempDir::new().expect("temp dir");
    let julie_home = temp_dir.path().join("julie-home");
    let source_root = temp_dir.path().join("source");
    fs::create_dir_all(&julie_home).expect("julie home");
    fs::create_dir_all(&source_root).expect("source root");

    let repo_root = source_root.join("ablation-fixture-repo");
    let daemon_db = Arc::new(
        DaemonDatabase::open(&julie_home.join("daemon.db")).expect("open daemon db"),
    );
    setup_indexed_workspace(&julie_home, repo_root, Arc::clone(&daemon_db));

    // Locate the (single) workspace's tantivy directory by glob.  The
    // workspace_id is generated at index time; we don't need it here, the
    // indexes/ dir has exactly one entry in the fixture.
    let indexes_dir = julie_home.join("indexes");
    let workspace_dirs: Vec<PathBuf> = fs::read_dir(&indexes_dir)
        .expect("read indexes dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    assert_eq!(
        workspace_dirs.len(),
        1,
        "fixture must have exactly one workspace directory, got: {workspace_dirs:?}"
    );
    let tantivy_dir = workspace_dirs[0].join("tantivy");
    let marker_path = tantivy_dir.join("julie-search-compat.json");

    let cases_path = temp_dir.path().join("cases.toml");
    let corpus_path = temp_dir.path().join("corpus.toml");
    write_cases_toml(&cases_path);
    write_corpus_toml(&corpus_path, &source_root);

    let out_path = temp_dir.path().join("ablation-both-report.json");
    run_search_matrix_baseline_with_home(
        &julie_home,
        &cases_path,
        &corpus_path,
        "ablation-smoke",
        &out_path,
        &Ablation::Both,
    )
    .expect("ablation=both run completes");

    // The compat marker must exist and reflect BASELINE tokenizer flags,
    // proving the eager restore ran successfully at end of ablation.
    let marker_text =
        fs::read_to_string(&marker_path).expect("compat marker must exist on disk");
    let marker: serde_json::Value =
        serde_json::from_str(&marker_text).expect("compat marker is JSON");
    let tok = &marker["tokenizer_signature"];

    assert_eq!(
        tok["ablate_stemming"].as_bool(),
        Some(false),
        "After ablation=both with eager restore, on-disk compat marker \
         must report ablate_stemming=false (baseline). Marker: {marker}"
    );
    assert_eq!(
        tok["ablate_camel_emit"].as_bool(),
        Some(false),
        "After ablation=both with eager restore, on-disk compat marker \
         must report ablate_camel_emit=false (baseline). Marker: {marker}"
    );
}
