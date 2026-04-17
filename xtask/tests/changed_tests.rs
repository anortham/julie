use xtask::changed::{ChangedSelectionMode, select_changed_buckets};
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
        vec!["cli", "tools-workspace", "workspace-init"]
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
        vec!["tools-search", "search-quality"]
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

fn sample_manifest() -> TestManifest {
    TestManifest::from_str(
        r#"
[tiers]
dev = ["cli", "tools-workspace", "workspace-init", "tools-search", "search-quality"]
dogfood = ["tools-dogfood-repo-index", "search-quality"]

[buckets.cli]
expected_seconds = 5
timeout_seconds = 30
commands = ["cargo test --lib tests::cli_tests"]

[buckets.tools-workspace]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::tools::workspace"]

[buckets.workspace-init]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::core::workspace_init"]

[buckets.tools-search]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::tools::search"]

[buckets.search-quality]
expected_seconds = 60
timeout_seconds = 120
commands = ["cargo test --lib search_quality"]

[buckets.tools-dogfood-repo-index]
expected_seconds = 200
timeout_seconds = 450
commands = ["cargo nextest run --lib tests::tools::get_symbols_target_filtering_dogfood -- --skip search_quality"]
"#,
    )
    .unwrap()
}
