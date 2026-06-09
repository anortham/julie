use std::fs;
use std::path::PathBuf;

use julie::registry::database::DaemonDatabase;
use serde_json::json;
use tempfile::TempDir;
use xtask::cli::{Ablation, CliCommand, SearchMatrixCommand, parse_cli_command};
use xtask::search_matrix::{
    SearchMatrixCaseSet, SearchMatrixCorpus, run_search_matrix_baseline_with_home,
};
use xtask::search_matrix_mine::mine_search_matrix_seed_report;
use xtask::workspace_root;

#[path = "support/toml_fixture.rs"]
mod toml_fixture;

use toml_fixture::toml_roots;

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
            ablation: xtask::cli::Ablation::None,
        })
    );
}

#[test]
fn search_matrix_contract_tests_baseline_no_longer_errors_on_removed_workspace_pool() {
    let temp = TempDir::new().expect("tempdir");
    let julie_home = temp.path().join("julie-home");
    let roots_dir = temp.path().join("roots");
    let cases_path = temp.path().join("cases.toml");
    let corpus_path = temp.path().join("corpus.toml");
    let out_path = temp.path().join("baseline.json");

    fs::create_dir_all(&julie_home).expect("julie home");
    fs::create_dir_all(&roots_dir).expect("roots");
    fs::write(
        &cases_path,
        r#"
[[cases]]
case_id = "missing_repo_case"
family = "exact_identifier"
query = "missing_symbol"
search_target = "definitions"
language = "rust"
profile_tags = ["smoke"]
expected_mode = "expect_hits"
"#,
    )
    .expect("write cases");
    fs::write(
        &corpus_path,
        format!(
            r#"
roots = ["{}"]

[profiles.smoke]
repos = ["missing-repo"]

[[repos]]
name = "missing-repo"
language = "rust"
profile_tags = ["smoke"]
"#,
            roots_dir.display()
        ),
    )
    .expect("write corpus");

    let report = run_search_matrix_baseline_with_home(
        &julie_home,
        &cases_path,
        &corpus_path,
        "smoke",
        &out_path,
        &Ablation::None,
    )
    .expect("baseline should skip missing repos instead of requiring WorkspacePool");

    assert!(report.executions.is_empty());
    assert_eq!(report.skipped_repos.len(), 1);
    assert_eq!(report.skipped_repos[0].repo_name, "missing-repo");
    assert!(
        report.skipped_repos[0]
            .reason
            .contains("repo root not found")
    );
    assert!(out_path.exists(), "baseline report should be written");
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
fn search_matrix_contract_tests_toml_roots_escape_windows_paths() {
    let windows_root = r"C:\Users\runner\source".to_string();
    let toml_text = toml_roots(&[windows_root.clone()]);
    let parsed: toml::Value = toml::from_str(&toml_text).expect("roots TOML should parse");
    let roots = parsed
        .get("roots")
        .and_then(toml::Value::as_array)
        .expect("roots should be an array");

    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0].as_str(), Some(windows_root.as_str()));
}

#[test]
fn search_matrix_contract_tests_case_fixture_loads_query_families_and_profiles() {
    let fixture_path = workspace_root().join("fixtures/search-quality/search-matrix-cases.toml");

    let cases =
        SearchMatrixCaseSet::load(&fixture_path).expect("case fixture should deserialize cleanly");

    assert!(
        cases.cases.len() >= 6,
        "expected a non-trivial starter matrix"
    );
    assert!(
        cases
            .cases
            .iter()
            .any(|case| case.family == "scoped_content"
                && case.profile_tags.contains(&"smoke".to_string()))
    );
    assert!(
        cases
            .cases
            .iter()
            .any(|case| case.expected_mode == "expect_hint_kind")
    );
    assert!(
        cases.cases.iter().any(|case| {
            case.search_target == "files" && case.profile_tags.contains(&"smoke".to_string())
        }),
        "starter matrix should include a smoke file-mode case"
    );
    assert!(
        cases.cases.iter().any(|case| {
            case.search_target == "files" && case.profile_tags.contains(&"breadth".to_string())
        }),
        "starter matrix should include a breadth file-mode case"
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
fn search_matrix_contract_tests_starter_fixtures_cover_large_verification_repos_in_breadth() {
    let cases_path = workspace_root().join("fixtures/search-quality/search-matrix-cases.toml");
    let corpus_path = workspace_root().join("fixtures/search-quality/search-matrix-corpus.toml");

    let cases =
        SearchMatrixCaseSet::load(&cases_path).expect("case fixture should deserialize cleanly");
    let corpus =
        SearchMatrixCorpus::load(&corpus_path).expect("corpus fixture should deserialize cleanly");

    let breadth_repos = corpus
        .profiles
        .get("breadth")
        .expect("breadth profile should exist");
    assert!(
        breadth_repos.repos.iter().any(|repo| repo == "riverpod"),
        "breadth profile should include riverpod"
    );
    assert!(
        breadth_repos
            .repos
            .iter()
            .any(|repo| repo == "nlohmann-json"),
        "breadth profile should include nlohmann-json"
    );
    assert!(
        !breadth_repos.repos.iter().any(|repo| repo == "rtk"),
        "breadth profile should not keep rtk once riverpod is added"
    );
    assert!(
        !breadth_repos.repos.iter().any(|repo| repo == "toon-python"),
        "breadth profile should not keep toon-python once nlohmann-json is added"
    );

    assert!(
        corpus
            .repos
            .iter()
            .any(|repo| { repo.name == "riverpod" && repo.language == "dart" }),
        "corpus should define riverpod as a dart repo"
    );
    assert!(
        corpus
            .repos
            .iter()
            .any(|repo| { repo.name == "nlohmann-json" && repo.language == "cpp" }),
        "corpus should define nlohmann-json as a cpp repo"
    );

    assert!(
        cases.cases.iter().any(|case| {
            case.repo_selector
                .as_ref()
                .is_some_and(|repos| repos.iter().any(|repo| repo == "riverpod"))
        }),
        "starter matrix should contain a riverpod-targeted case"
    );
    assert!(
        cases.cases.iter().any(|case| {
            case.repo_selector
                .as_ref()
                .is_some_and(|repos| repos.iter().any(|repo| repo == "nlohmann-json"))
        }),
        "starter matrix should contain a nlohmann-json-targeted case"
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
            "top_hits": [{"name": "WorkspacePool", "file": "src/registry/workspace_pool.rs"}]
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

    assert_eq!(
        report.candidates.len(),
        2,
        "only fast_search rows should be mined"
    );
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
