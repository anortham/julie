use xtask::changed::{ChangedSelectionMode, render_changed_selection, select_changed_buckets};
use xtask::cli::{TestCommand, parse_test_command};
use xtask::manifest::TestManifest;

#[test]
fn changed_tests_cli_parses_changed_subcommand() {
    assert!(matches!(
        parse_test_command(["xtask", "test", "changed"]),
        Ok(TestCommand::Changed {
            timeout_multiplier: 1,
            coverage: false,
        })
    ));
}

#[test]
fn changed_tests_select_localized_tool_buckets() {
    let manifest = sample_manifest();

    let selection = select_changed_buckets(
        &manifest,
        &[
            "src/tools/workspace/mod.rs".to_string(),
            "src/tests/tools/workspace/mod_tests.rs".to_string(),
            "xtask/src/cli.rs".to_string(),
        ],
    );

    assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
    assert_eq!(
        selection.bucket_names,
        vec!["xtask-runner", "tools-workspace", "workspace-init"]
    );
    assert!(selection.fallback_paths.is_empty());
}

#[test]
fn changed_tests_workspace_test_changes_do_not_pull_workspace_init() {
    let manifest = sample_manifest();

    let selection = select_changed_buckets(
        &manifest,
        &["src/tests/tools/workspace/mod_tests.rs".to_string()],
    );

    assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
    assert_eq!(selection.bucket_names, vec!["tools-workspace"]);
}

#[test]
fn changed_tests_falls_back_to_dev_for_shared_infrastructure() {
    let manifest = sample_manifest();

    let selection = select_changed_buckets(&manifest, &["src/handler.rs".to_string()]);

    assert_eq!(selection.mode, ChangedSelectionMode::FallbackToDev);
    assert_eq!(selection.bucket_names, manifest.tiers["dev"]);
    assert_eq!(selection.fallback_paths, vec!["src/handler.rs"]);
}

#[test]
fn changed_tests_ignores_docs_only_changes() {
    let manifest = sample_manifest();

    let selection = select_changed_buckets(
        &manifest,
        &[
            "docs/PRE-RELEASE-FINDINGS.md".to_string(),
            ".memories/checkpoints/example.md".to_string(),
        ],
    );

    assert_eq!(selection.mode, ChangedSelectionMode::NoChanges);
    assert!(selection.bucket_names.is_empty());
    assert_eq!(
        selection.ignored_paths,
        vec![
            ".memories/checkpoints/example.md",
            "docs/PRE-RELEASE-FINDINGS.md",
        ]
    );
}

#[test]
fn changed_tests_selects_search_and_dogfood_for_search_core_changes() {
    let manifest = sample_manifest();

    let selection = select_changed_buckets(&manifest, &["src/search/scoring.rs".to_string()]);

    assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
    assert_eq!(
        selection.bucket_names,
        vec![
            "tools-search-tantivy",
            "tools-search-line-file",
            "tools-search-ranking-format",
            "tools-search-context",
            "tools-search-text",
            "tools-search-hybrid",
            "tools-search-query",
            "search-quality"
        ]
    );
}

#[test]
fn changed_tests_dogfood_repo_index_file_routes_to_new_bucket() {
    let manifest = sample_manifest();

    let selection = select_changed_buckets(
        &manifest,
        &["src/tests/tools/get_symbols_target_filtering_dogfood.rs".to_string()],
    );

    assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
    assert_eq!(selection.bucket_names, vec!["tools-dogfood-repo-index"]);
}

#[test]
fn changed_tests_extractor_crate_paths_select_extractors_bucket() {
    let manifest = sample_manifest();

    let selection = select_changed_buckets(
        &manifest,
        &[
            "crates/julie-extractors/src/rust/mod.rs".to_string(),
            "crates/julie-extractors/src/tests/rust/mod.rs".to_string(),
        ],
    );

    assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
    assert_eq!(selection.bucket_names, vec!["extractors"]);
    assert!(selection.fallback_paths.is_empty());
}

#[test]
fn changed_tests_extraction_fixtures_select_parser_upgrade_bucket() {
    let manifest = sample_manifest();

    let selection = select_changed_buckets(
        &manifest,
        &["fixtures/extraction/rust/basic/expected.json".to_string()],
    );

    assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
    assert_eq!(selection.bucket_names, vec!["parser-upgrade"]);
    assert!(selection.fallback_paths.is_empty());
}

#[test]
fn changed_tests_tree_sitter_dependency_paths_select_parser_upgrade_bucket() {
    let manifest = sample_manifest();

    let selection = select_changed_buckets(
        &manifest,
        &[
            "Cargo.toml".to_string(),
            "Cargo.lock".to_string(),
            "crates/julie-extractors/Cargo.toml".to_string(),
        ],
    );

    assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
    assert_eq!(selection.bucket_names, vec!["parser-upgrade"]);
    assert!(selection.fallback_paths.is_empty());
}

#[test]
fn changed_tests_reports_fallback_prefix_rationale() {
    let manifest = sample_manifest();

    let selection = select_changed_buckets(&manifest, &["src/analysis/scoring.rs".to_string()]);
    let output = render_changed_selection(&selection);

    assert_eq!(selection.mode, ChangedSelectionMode::FallbackToDev);
    assert!(output.contains(
        "CHANGED: rationale: src/analysis/scoring.rs -> dev (fallback prefix: src/analysis/)"
    ));
}

#[test]
fn changed_tests_reports_path_to_bucket_rationale() {
    let manifest = sample_manifest();

    let selection = select_changed_buckets(&manifest, &["src/tools/search/mod.rs".to_string()]);
    let output = render_changed_selection(&selection);

    assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
    assert_eq!(
        selection.bucket_names,
        vec![
            "tools-search-tantivy",
            "tools-search-line-file",
            "tools-search-ranking-format",
            "tools-search-context",
            "tools-search-text",
            "tools-search-hybrid",
            "tools-search-query",
        ]
    );
    assert!(output.contains("CHANGED: rationale: src/tools/search/mod.rs -> tools-search-tantivy, tools-search-line-file, tools-search-ranking-format, tools-search-context, tools-search-text, tools-search-hybrid, tools-search-query"));
}

#[test]
fn changed_tests_search_paths_select_split_search_buckets() {
    let manifest = sample_manifest();

    let selection = select_changed_buckets(
        &manifest,
        &[
            "src/tests/tools/search/tantivy_tokenizer_tests.rs".to_string(),
            "src/tests/tools/search/line_mode.rs".to_string(),
            "src/tests/tools/search/content_scoring_tests.rs".to_string(),
            "src/tests/tools/search_context_lines.rs".to_string(),
            "src/tests/tools/text_search_tantivy.rs".to_string(),
            "src/tests/tools/hybrid_search_tests.rs".to_string(),
            "src/tools/search/query_preprocessor.rs".to_string(),
        ],
    );

    assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
    assert_eq!(
        selection.bucket_names,
        vec![
            "tools-search-tantivy",
            "tools-search-line-file",
            "tools-search-ranking-format",
            "tools-search-context",
            "tools-search-text",
            "tools-search-hybrid",
            "tools-search-query",
        ]
    );
}

#[test]
fn changed_tests_xtask_paths_select_xtask_runner_bucket() {
    let manifest = sample_manifest();

    let selection = select_changed_buckets(&manifest, &["xtask/src/runner.rs".to_string()]);

    assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
    assert_eq!(selection.bucket_names, vec!["xtask-runner"]);
}

#[test]
fn changed_tests_harness_docs_select_xtask_runner_bucket() {
    let manifest = sample_manifest();

    let selection = select_changed_buckets(
        &manifest,
        &[
            "AGENTS.md".to_string(),
            "CLAUDE.md".to_string(),
            ".cargo/config.toml".to_string(),
            "docs/TESTING_GUIDE.md".to_string(),
            "docs/plans/verification-ledger-template.md".to_string(),
        ],
    );

    assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
    assert_eq!(selection.bucket_names, vec!["xtask-runner"]);
    assert!(selection.ignored_paths.is_empty());
}

#[test]
fn changed_tests_misc_tool_paths_select_split_tool_buckets() {
    let manifest = sample_manifest();

    let selection = select_changed_buckets(
        &manifest,
        &[
            "src/tools/symbols/mod.rs".to_string(),
            "src/tools/editing/rewrite.rs".to_string(),
            "src/tools/deep_dive/mod.rs".to_string(),
            "src/tools/refactoring/rename.rs".to_string(),
            "src/tools/metrics/centrality.rs".to_string(),
            "src/tests/tools/filtering_tests.rs".to_string(),
        ],
    );

    assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
    assert_eq!(
        selection.bucket_names,
        vec![
            "tools-get-symbols",
            "tools-editing",
            "tools-navigation",
            "tools-refactoring",
            "tools-metrics",
            "tools-format-filter",
        ]
    );
}

#[test]
fn changed_tests_routes_projection_paths_to_projection_bucket() {
    let manifest = sample_manifest();

    let selection = select_changed_buckets(&manifest, &["src/search/projection.rs".to_string()]);

    assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
    assert_eq!(selection.bucket_names, vec!["projection"]);
}

#[test]
fn changed_tests_routes_projection_pipeline_paths_to_projection_bucket() {
    let manifest = sample_manifest();

    let selection = select_changed_buckets(
        &manifest,
        &["src/tools/workspace/indexing/pipeline.rs".to_string()],
    );

    assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
    assert_eq!(selection.bucket_names, vec!["projection"]);
}

#[test]
fn changed_tests_routes_lifecycle_paths_to_lifecycle_bucket() {
    let manifest = sample_manifest();

    let selection = select_changed_buckets(&manifest, &["src/daemon/lifecycle.rs".to_string()]);

    assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
    assert_eq!(selection.bucket_names, vec!["lifecycle"]);
}

#[test]
fn changed_tests_routes_daemon_mod_to_lifecycle_and_daemon_buckets() {
    let manifest = sample_manifest();

    let selection = select_changed_buckets(&manifest, &["src/daemon/mod.rs".to_string()]);

    assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
    assert_eq!(selection.bucket_names, vec!["lifecycle", "daemon"]);
}

#[test]
fn changed_tests_routes_transport_paths_to_transport_bucket() {
    let manifest = sample_manifest();

    let selection = select_changed_buckets(&manifest, &["src/adapter/mod.rs".to_string()]);

    assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
    assert_eq!(selection.bucket_names, vec!["transport"]);
}

#[test]
fn changed_tests_routes_http_transport_paths_to_transport_bucket() {
    let manifest = sample_manifest();

    let selection =
        select_changed_buckets(&manifest, &["src/daemon/http_transport.rs".to_string()]);

    assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
    assert_eq!(selection.bucket_names, vec!["transport"]);
}

#[test]
fn changed_tests_routes_workspace_runtime_paths_to_workspace_runtime_bucket() {
    let manifest = sample_manifest();

    let selection =
        select_changed_buckets(&manifest, &["src/daemon/workspace_pool.rs".to_string()]);

    assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
    assert_eq!(selection.bucket_names, vec!["workspace-runtime"]);
}

#[test]
fn changed_tests_routes_workspace_registry_commands_to_workspace_runtime_bucket() {
    let manifest = sample_manifest();

    let selection = select_changed_buckets(
        &manifest,
        &["src/tools/workspace/commands/registry/open.rs".to_string()],
    );

    assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
    assert_eq!(selection.bucket_names, vec!["workspace-runtime"]);
}

#[test]
fn changed_tests_ignored_docs_only_output_remains_concise() {
    let manifest = sample_manifest();

    let selection = select_changed_buckets(
        &manifest,
        &[
            "docs/PRE-RELEASE-FINDINGS.md".to_string(),
            ".memories/checkpoints/example.md".to_string(),
        ],
    );
    let output = render_changed_selection(&selection);

    assert_eq!(selection.mode, ChangedSelectionMode::NoChanges);
    assert_eq!(
        output,
        "CHANGED: no code/test buckets matched local changes\n\
CHANGED: ignored non-executable paths: .memories/checkpoints/example.md, docs/PRE-RELEASE-FINDINGS.md\n"
    );
}

fn sample_manifest() -> TestManifest {
    TestManifest::from_str(
        r#"
[tiers]
dev = [
  "cli",
  "xtask-runner",
  "tools-workspace",
  "workspace-init",
  "tools-search-tantivy",
  "tools-search-line-file",
  "tools-search-ranking-format",
  "tools-search-context",
  "tools-search-text",
  "tools-search-hybrid",
  "tools-search-query",
  "tools-get-symbols",
  "tools-editing",
  "tools-navigation",
  "tools-refactoring",
  "tools-metrics",
  "tools-format-filter",
  "search-quality",
]
dogfood = ["tools-dogfood-repo-index", "search-quality"]

[buckets.cli]
expected_seconds = 5
timeout_seconds = 30
commands = ["cargo test --lib tests::cli_tests"]

[buckets.xtask-runner]
expected_seconds = 5
timeout_seconds = 30
commands = ["cargo nextest run -p xtask"]

[buckets.tools-workspace]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::tools::workspace"]

[buckets.workspace-init]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::core::workspace_init"]

[buckets.tools-search-tantivy]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::tools::search::tantivy_"]

[buckets.tools-search-line-file]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::tools::search::line_"]

[buckets.tools-search-ranking-format]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::tools::search::content_scoring_tests"]

[buckets.tools-search-context]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::tools::search_context_lines"]

[buckets.tools-search-text]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::tools::text_search_tantivy"]

[buckets.tools-search-hybrid]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::tools::hybrid_search_tests"]

[buckets.tools-search-query]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tools::search::query_preprocessor::tests"]

[buckets.tools-get-symbols]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::tools::get_symbols::"]

[buckets.tools-editing]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::tools::editing::"]

[buckets.tools-navigation]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::tools::deep_dive_tests"]

[buckets.tools-refactoring]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::tools::refactoring::"]

[buckets.tools-metrics]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::tools::metrics::"]

[buckets.tools-format-filter]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::tools::filtering_tests"]

[buckets.search-quality]
expected_seconds = 60
timeout_seconds = 120
commands = ["cargo test --lib search_quality"]

[buckets.tools-dogfood-repo-index]
expected_seconds = 200
timeout_seconds = 450
commands = ["cargo nextest run --lib tests::tools::get_symbols_target_filtering_dogfood -- --skip search_quality"]

[buckets.extractors]
expected_seconds = 30
timeout_seconds = 90
commands = [
  "cargo nextest run -p julie-extractors golden",
  "cargo nextest run -p julie-extractors capability_matrix",
]

[buckets.parser-upgrade]
expected_seconds = 60
timeout_seconds = 180
commands = [
  "cargo nextest run -p julie-extractors -E 'test(golden) | test(capability_matrix) | test(parser_upgrade)'",
]

[buckets.projection]
expected_seconds = 40
timeout_seconds = 90
commands = ["cargo nextest run --lib tests::integration::projection_repair -- --skip search_quality"]

[buckets.transport]
expected_seconds = 40
timeout_seconds = 90
commands = ["cargo nextest run --lib tests::daemon::transport -- --skip search_quality"]

[buckets.lifecycle]
expected_seconds = 40
timeout_seconds = 90
commands = ["cargo nextest run --lib tests::daemon::lifecycle -- --skip search_quality"]

[buckets.daemon]
expected_seconds = 40
timeout_seconds = 90
commands = ["cargo nextest run --lib tests::daemon -- --skip search_quality"]

[buckets.workspace-runtime]
expected_seconds = 40
timeout_seconds = 90
commands = ["cargo nextest run --lib tests::daemon::workspace_pool -- --skip search_quality"]
    "#,
    )
    .unwrap()
}
