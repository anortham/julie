use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use julie::daemon::database::DaemonDatabase;
use julie::daemon::workspace_pool::WorkspacePool;
use julie::handler::JulieServerHandler;
use serde_json::json;
use tempfile::TempDir;
use tokio::runtime::Builder;
use xtask::cli::{CliCommand, SearchMatrixCommand, parse_cli_command};
use xtask::search_matrix::{
    SearchMatrixCaseSet, SearchMatrixCorpus, run_search_matrix_baseline_with_home,
};
use xtask::search_matrix_mine::mine_search_matrix_seed_report;
use xtask::workspace_root;

#[test]
fn search_matrix_contract_tests_parse_mine_command() {
    let parsed = parse_cli_command([
        "xtask",
        "search-matrix",
        "mine",
        "--days",
        "7",
        "--out",
        "artifacts/search-matrix/seeds.json",
    ])
    .expect("mine command should parse");

    assert_eq!(
        parsed,
        CliCommand::SearchMatrix(SearchMatrixCommand::Mine {
            days: 7,
            out: PathBuf::from("artifacts/search-matrix/seeds.json"),
        })
    );
}

#[test]
fn search_matrix_contract_tests_parse_baseline_command() {
    let parsed = parse_cli_command([
        "xtask",
        "search-matrix",
        "baseline",
        "--profile",
        "smoke",
        "--out",
        "artifacts/search-matrix/baseline.json",
    ])
    .expect("baseline command should parse");

    assert_eq!(
        parsed,
        CliCommand::SearchMatrix(SearchMatrixCommand::Baseline {
            profile: "smoke".to_string(),
            out: Some(PathBuf::from("artifacts/search-matrix/baseline.json")),
        })
    );
}

#[test]
fn search_matrix_contract_tests_reject_unknown_search_matrix_subcommand() {
    let error = parse_cli_command(["xtask", "search-matrix", "weird"]).unwrap_err();

    assert!(
        error
            .to_string()
            .contains("unsupported `cargo xtask search-matrix` subcommand `weird`"),
        "got: {error}"
    );
}

#[test]
fn search_matrix_contract_tests_reject_profile_flag_for_mine() {
    let error = parse_cli_command([
        "xtask",
        "search-matrix",
        "mine",
        "--days",
        "7",
        "--out",
        "artifacts/search-matrix/seeds.json",
        "--profile",
        "smoke",
    ])
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("`--profile` is not valid for `cargo xtask search-matrix mine`"),
        "got: {error}"
    );
}

#[test]
fn search_matrix_contract_tests_reject_days_flag_for_baseline() {
    let error = parse_cli_command([
        "xtask",
        "search-matrix",
        "baseline",
        "--profile",
        "smoke",
        "--days",
        "7",
    ])
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("`--days` is not valid for `cargo xtask search-matrix baseline`"),
        "got: {error}"
    );
}

#[test]
fn search_matrix_contract_tests_case_fixture_loads_query_families_and_profiles() {
    let fixture_path = workspace_root().join("fixtures/search-quality/search-matrix-cases.toml");

    let cases =
        SearchMatrixCaseSet::load(&fixture_path).expect("case fixture should deserialize cleanly");

    assert!(cases.cases.len() >= 6, "expected a non-trivial starter matrix");
    assert!(
        cases
            .cases
            .iter()
            .any(|case| case.family == "scoped_content" && case.profile_tags.contains(&"smoke".to_string()))
    );
    assert!(
        cases
            .cases
            .iter()
            .any(|case| case.expected_mode == "expect_hint_kind")
    );
}

#[test]
fn search_matrix_contract_tests_corpus_fixture_loads_profiles_and_roots() {
    let fixture_path = workspace_root().join("fixtures/search-quality/search-matrix-corpus.toml");

    let corpus =
        SearchMatrixCorpus::load(&fixture_path).expect("corpus fixture should deserialize cleanly");

    assert_eq!(
        corpus
            .roots
            .first()
            .expect("starter corpus should declare a default root"),
        "~/source"
    );
    assert!(corpus.profiles.contains_key("smoke"));
    assert!(corpus.profiles.contains_key("breadth"));
    assert!(
        corpus
            .repos
            .iter()
            .any(|repo| repo.name == "julie" && repo.profile_tags.contains(&"smoke".to_string()))
    );
}

#[test]
fn search_matrix_contract_tests_mine_ignores_non_search_rows_and_preserves_trace_fields() {
    let temp_dir = TempDir::new().expect("temp dir");
    let db_path = temp_dir.path().join("daemon.db");
    let out_path = temp_dir.path().join("seed-report.json");
    let db = DaemonDatabase::open(&db_path).expect("daemon db");

    let scoped_zero_hit = json!({
        "query": "line_matches",
        "normalized_query": "line matches",
        "search_target": "content",
        "language": null,
        "file_pattern": "src/ui/**",
        "exclude_tests": false,
        "trace": {
            "result_count": 0,
            "relaxed": false,
            "zero_hit_reason": "file_pattern_filtered",
            "file_pattern_diagnostic": "no_in_scope_candidates",
            "hint_kind": "out_of_scope_content_hint",
            "top_hits": []
        }
    });
    db.insert_tool_call(
        "julie_ws",
        "sess-1",
        "fast_search",
        12.0,
        Some(0),
        None,
        Some(300),
        true,
        Some(&scoped_zero_hit.to_string()),
    )
    .expect("insert scoped zero-hit");

    let exact_hit = json!({
        "query": "WorkspacePool",
        "normalized_query": "workspacepool",
        "search_target": "definitions",
        "language": "rust",
        "file_pattern": null,
        "exclude_tests": false,
        "trace": {
            "result_count": 3,
            "relaxed": false,
            "zero_hit_reason": null,
            "file_pattern_diagnostic": null,
            "hint_kind": null,
            "top_hits": [{"name": "WorkspacePool", "file": "src/daemon/workspace_pool.rs"}]
        }
    });
    db.insert_tool_call(
        "julie_ws",
        "sess-2",
        "fast_search",
        8.0,
        Some(3),
        None,
        Some(180),
        true,
        Some(&exact_hit.to_string()),
    )
    .expect("insert exact hit");

    db.insert_tool_call(
        "julie_ws",
        "sess-3",
        "deep_dive",
        7.0,
        Some(1),
        None,
        Some(220),
        true,
        Some(r#"{"symbol":"WorkspacePool"}"#),
    )
    .expect("insert non-search row");

    let report = mine_search_matrix_seed_report(&db_path, 7, &out_path).expect("mine report");

    assert_eq!(report.candidates.len(), 2, "only fast_search rows should be mined");
    assert!(
        report.clusters.iter().any(|cluster| {
            cluster.family == "scoped_content"
                && cluster.zero_hit_reason.as_deref() == Some("file_pattern_filtered")
                && cluster.file_pattern_diagnostic.as_deref() == Some("no_in_scope_candidates")
                && cluster.hint_kind.as_deref() == Some("out_of_scope_content_hint")
        }),
        "expected scoped zero-hit cluster in {:?}",
        report.clusters
    );
    assert!(out_path.is_file(), "json artifact should be written");
    assert!(
        out_path.with_extension("md").is_file(),
        "markdown artifact should be written next to the json report"
    );
}

#[test]
fn search_matrix_contract_tests_mine_preserves_alternation_and_exclusion_file_pattern_families() {
    let temp_dir = TempDir::new().expect("temp dir");
    let db_path = temp_dir.path().join("daemon.db");
    let out_path = temp_dir.path().join("seed-report.json");
    let db = DaemonDatabase::open(&db_path).expect("daemon db");

    let alternation = json!({
        "query": "blast_radius",
        "search_target": "content",
        "file_pattern": "docs/**|src/**",
        "exclude_tests": false,
        "trace": {
            "result_count": 0,
            "relaxed": false,
            "zero_hit_reason": "file_pattern_filtered",
            "file_pattern_diagnostic": "candidate_starvation",
            "hint_kind": null
        }
    });
    db.insert_tool_call(
        "julie_ws",
        "sess-alt",
        "fast_search",
        5.0,
        Some(0),
        None,
        Some(120),
        true,
        Some(&alternation.to_string()),
    )
    .expect("insert alternation row");

    let exclusion = json!({
        "query": "needle_token",
        "search_target": "content",
        "file_pattern": "src/**,!src/tests/**",
        "exclude_tests": false,
        "trace": {
            "result_count": 0,
            "relaxed": false,
            "zero_hit_reason": "file_pattern_filtered",
            "file_pattern_diagnostic": "no_in_scope_candidates",
            "hint_kind": null
        }
    });
    db.insert_tool_call(
        "julie_ws",
        "sess-excl",
        "fast_search",
        5.0,
        Some(0),
        None,
        Some(140),
        true,
        Some(&exclusion.to_string()),
    )
    .expect("insert exclusion row");

    let report = mine_search_matrix_seed_report(&db_path, 7, &out_path).expect("mine report");
    let families = report
        .candidates
        .iter()
        .map(|candidate| candidate.family.as_str())
        .collect::<Vec<_>>();

    assert!(
        families.contains(&"alternation_file_pattern"),
        "expected alternation family in {:?}",
        families
    );
    assert!(
        families.contains(&"exclusion_file_pattern"),
        "expected exclusion family in {:?}",
        families
    );
}

#[test]
fn search_matrix_contract_tests_baseline_runs_ready_workspace_and_skips_pending_one() {
    let temp_dir = TempDir::new().expect("temp dir");
    let julie_home = temp_dir.path().join("julie-home");
    let source_root = temp_dir.path().join("source");
    fs::create_dir_all(&julie_home).expect("julie home");
    fs::create_dir_all(&source_root).expect("source root");

    let ready_repo = source_root.join("ready-repo");
    fs::create_dir_all(ready_repo.join("src")).expect("ready repo src");
    fs::write(
        ready_repo.join("src/lib.rs"),
        "pub fn needle_function() -> i32 {\n    7\n}\n",
    )
    .expect("write ready repo");

    let pending_repo = source_root.join("pending-repo");
    fs::create_dir_all(pending_repo.join("src")).expect("pending repo src");
    fs::write(
        pending_repo.join("src/lib.rs"),
        "pub fn pending_function() -> i32 {\n    9\n}\n",
    )
    .expect("write pending repo");

    let daemon_db = Arc::new(
        DaemonDatabase::open(&julie_home.join("daemon.db")).expect("open temp daemon db"),
    );
    daemon_db
        .upsert_workspace(
            "pending_repo_ws",
            &pending_repo.to_string_lossy(),
            "pending",
        )
        .expect("register pending repo");

    index_ready_repo(&julie_home, Arc::clone(&daemon_db), ready_repo.clone());

    let cases_path = temp_dir.path().join("cases.toml");
    fs::write(
        &cases_path,
        r#"
[[cases]]
case_id = "needle-definition"
family = "exact_identifier"
query = "needle_function"
search_target = "definitions"
profile_tags = ["smoke"]
repo_selector = ["ready-repo", "pending-repo"]
expected_mode = "expect_hits"
"#,
    )
    .expect("write cases fixture");

    let corpus_path = temp_dir.path().join("corpus.toml");
    fs::write(
        &corpus_path,
        format!(
            r#"
roots = ["{}"]

[profiles.smoke]
repos = ["ready-repo", "pending-repo"]

[[repos]]
name = "ready-repo"
language = "rust"
profile_tags = ["smoke"]

[[repos]]
name = "pending-repo"
language = "rust"
profile_tags = ["smoke"]
"#,
            source_root.display()
        ),
    )
    .expect("write corpus fixture");

    let out_path = temp_dir.path().join("baseline-report.json");
    let report = run_search_matrix_baseline_with_home(
        &julie_home,
        &cases_path,
        &corpus_path,
        "smoke",
        &out_path,
    )
    .expect("baseline report");

    assert_eq!(report.executions.len(), 1, "only ready repo should execute");
    assert_eq!(report.executions[0].repo_name, "ready-repo");
    assert!(
        report.executions[0].hit_count >= 1,
        "expected indexed repo to return at least one hit"
    );
    assert!(
        report
            .skipped_repos
            .iter()
            .any(|repo| repo.repo_name == "pending-repo" && repo.reason.contains("pending"))
    );
    assert!(out_path.is_file(), "baseline json artifact should be written");
    assert!(
        out_path.with_extension("md").is_file(),
        "baseline markdown artifact should be written"
    );
}

#[test]
fn search_matrix_contract_tests_baseline_uses_configured_roots_for_workspace_matching() {
    let temp_dir = TempDir::new().expect("temp dir");
    let julie_home = temp_dir.path().join("julie-home");
    let preferred_root = temp_dir.path().join("preferred-source");
    let other_root = temp_dir.path().join("other-source");
    fs::create_dir_all(&julie_home).expect("julie home");
    fs::create_dir_all(&preferred_root).expect("preferred source root");
    fs::create_dir_all(&other_root).expect("other source root");

    let preferred_repo = preferred_root.join("shared-repo");
    fs::create_dir_all(preferred_repo.join("src")).expect("preferred repo src");
    fs::write(
        preferred_repo.join("src/lib.rs"),
        "pub fn preferred_function() -> i32 {\n    11\n}\n",
    )
    .expect("write preferred repo");

    let indexed_repo = other_root.join("shared-repo");
    fs::create_dir_all(indexed_repo.join("src")).expect("indexed repo src");
    fs::write(
        indexed_repo.join("src/lib.rs"),
        "pub fn preferred_function() -> i32 {\n    13\n}\n",
    )
    .expect("write indexed repo");

    let daemon_db = Arc::new(
        DaemonDatabase::open(&julie_home.join("daemon.db")).expect("open temp daemon db"),
    );
    index_ready_repo(&julie_home, Arc::clone(&daemon_db), indexed_repo);

    let cases_path = temp_dir.path().join("cases.toml");
    fs::write(
        &cases_path,
        r#"
[[cases]]
case_id = "preferred-definition"
family = "exact_identifier"
query = "preferred_function"
search_target = "definitions"
profile_tags = ["smoke"]
repo_selector = ["shared-repo"]
expected_mode = "expect_hits"
"#,
    )
    .expect("write cases fixture");

    let corpus_path = temp_dir.path().join("corpus.toml");
    fs::write(
        &corpus_path,
        format!(
            r#"
roots = ["{}"]

[profiles.smoke]
repos = ["shared-repo"]

[[repos]]
name = "shared-repo"
language = "rust"
profile_tags = ["smoke"]
"#,
            preferred_root.display()
        ),
    )
    .expect("write corpus fixture");

    let out_path = temp_dir.path().join("baseline-report.json");
    let report = run_search_matrix_baseline_with_home(
        &julie_home,
        &cases_path,
        &corpus_path,
        "smoke",
        &out_path,
    )
    .expect("baseline report");

    assert!(
        report.executions.is_empty(),
        "runner should not grab a same-name workspace outside configured roots"
    );
    assert!(
        report.skipped_repos.iter().any(|repo| {
            repo.repo_name == "shared-repo"
                && repo.reason.contains("resolved repo root")
        }),
        "expected root-scoped workspace miss, got {:?}",
        report.skipped_repos
    );
}

#[test]
fn search_matrix_contract_tests_baseline_reports_total_results_beyond_limit() {
    let temp_dir = TempDir::new().expect("temp dir");
    let julie_home = temp_dir.path().join("julie-home");
    let source_root = temp_dir.path().join("source");
    fs::create_dir_all(&julie_home).expect("julie home");
    fs::create_dir_all(&source_root).expect("source root");

    let dense_repo = source_root.join("dense-repo");
    fs::create_dir_all(dense_repo.join("src")).expect("dense repo src");
    let repeated_lines = (0..12)
        .map(|idx| format!("const NEEDLE_TOKEN_{idx}: &str = \"needletoken\";\n"))
        .collect::<String>();
    fs::write(dense_repo.join("src/lib.rs"), repeated_lines).expect("write dense repo");

    let daemon_db = Arc::new(
        DaemonDatabase::open(&julie_home.join("daemon.db")).expect("open temp daemon db"),
    );
    index_ready_repo(&julie_home, Arc::clone(&daemon_db), dense_repo.clone());

    let cases_path = temp_dir.path().join("cases.toml");
    fs::write(
        &cases_path,
        r#"
[[cases]]
case_id = "needle-content"
family = "multi_token"
query = "needletoken"
search_target = "content"
profile_tags = ["smoke"]
repo_selector = ["dense-repo"]
expected_mode = "observational"
"#,
    )
    .expect("write cases fixture");

    let corpus_path = temp_dir.path().join("corpus.toml");
    fs::write(
        &corpus_path,
        format!(
            r#"
roots = ["{}"]

[profiles.smoke]
repos = ["dense-repo"]

[[repos]]
name = "dense-repo"
language = "rust"
profile_tags = ["smoke"]
"#,
            source_root.display()
        ),
    )
    .expect("write corpus fixture");

    let out_path = temp_dir.path().join("baseline-report.json");
    let report = run_search_matrix_baseline_with_home(
        &julie_home,
        &cases_path,
        &corpus_path,
        "smoke",
        &out_path,
    )
    .expect("baseline report");

    assert_eq!(report.executions.len(), 1, "expected one execution");
    assert_eq!(report.executions[0].hit_count, 10);
    assert!(
        report.executions[0].hit_count_is_lower_bound,
        "content execution should mark a saturated page-size count as a lower bound"
    );
    assert_eq!(
        report.executions[0].top_hits.len(),
        3,
        "top hits remain a small sample even when total hits exceed the page limit"
    );
}

#[test]
fn search_matrix_contract_tests_baseline_flags_expect_hits_zero_hit() {
    let temp_dir = TempDir::new().expect("temp dir");
    let julie_home = temp_dir.path().join("julie-home");
    let source_root = temp_dir.path().join("source");
    fs::create_dir_all(&julie_home).expect("julie home");
    fs::create_dir_all(&source_root).expect("source root");

    let ready_repo = source_root.join("ready-repo");
    fs::create_dir_all(ready_repo.join("src")).expect("ready repo src");
    fs::write(
        ready_repo.join("src/lib.rs"),
        "pub fn present_function() -> i32 {\n    7\n}\n",
    )
    .expect("write ready repo");

    let daemon_db = Arc::new(
        DaemonDatabase::open(&julie_home.join("daemon.db")).expect("open temp daemon db"),
    );
    index_ready_repo(&julie_home, Arc::clone(&daemon_db), ready_repo.clone());

    let cases_path = temp_dir.path().join("cases.toml");
    fs::write(
        &cases_path,
        r#"
[[cases]]
case_id = "missing-definition"
family = "exact_identifier"
query = "missing_function"
search_target = "definitions"
profile_tags = ["smoke"]
repo_selector = ["ready-repo"]
expected_mode = "expect_hits"
"#,
    )
    .expect("write cases fixture");

    let corpus_path = temp_dir.path().join("corpus.toml");
    fs::write(
        &corpus_path,
        format!(
            r#"
roots = ["{}"]

[profiles.smoke]
repos = ["ready-repo"]

[[repos]]
name = "ready-repo"
language = "rust"
profile_tags = ["smoke"]
"#,
            source_root.display()
        ),
    )
    .expect("write corpus fixture");

    let out_path = temp_dir.path().join("baseline-report.json");
    let report = run_search_matrix_baseline_with_home(
        &julie_home,
        &cases_path,
        &corpus_path,
        "smoke",
        &out_path,
    )
    .expect("baseline report");

    assert!(
        report.summary_flags.iter().any(|flag| flag == "expected_hits_missing"),
        "expected expect_hits zero-hit to surface in summary flags: {:?}",
        report.summary_flags
    );
}

fn index_ready_repo(julie_home: &std::path::Path, daemon_db: Arc<DaemonDatabase>, repo_root: PathBuf) {
    let indexes_dir = julie_home.join("indexes");
    fs::create_dir_all(&indexes_dir).expect("indexes dir");

    let runtime = Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    runtime.block_on(async move {
        let pool = Arc::new(WorkspacePool::new(indexes_dir, Some(Arc::clone(&daemon_db)), None, None));
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
