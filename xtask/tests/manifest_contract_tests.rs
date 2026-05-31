use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use xtask::manifest::TestManifest;

#[path = "support/manifest_contract_expected.rs"]
mod manifest_contract_expected;

use manifest_contract_expected::{expected_bucket_metadata, expected_buckets};

#[test]
fn manifest_contract_tests_checked_in_manifest_uses_approved_first_pass_tiers() {
    let manifest = load_checked_in_manifest();

    assert_eq!(manifest.tiers, expected_tiers());
    assert_eq!(manifest.blocked_tiers, expected_blocked_tiers());
}

#[test]
fn manifest_contract_tests_checked_in_manifest_uses_exact_bucket_specs() {
    let manifest = load_checked_in_manifest();
    let expected = expected_buckets();
    let expected_metadata = expected_bucket_metadata();

    assert_eq!(
        manifest.buckets.keys().cloned().collect::<Vec<_>>(),
        expected
            .keys()
            .cloned()
            .map(str::to_string)
            .collect::<Vec<_>>()
    );

    for (bucket_name, expected_bucket) in expected {
        let actual = &manifest.buckets[bucket_name];
        let expected_metadata = &expected_metadata[bucket_name];

        assert_eq!(
            actual.expected_seconds, expected_bucket.expected_seconds,
            "bucket `{bucket_name}` expected_seconds changed"
        );
        assert_eq!(
            actual.timeout_seconds, expected_bucket.timeout_seconds,
            "bucket `{bucket_name}` timeout_seconds changed"
        );
        assert_eq!(
            actual.commands, expected_bucket.commands,
            "bucket `{bucket_name}` commands changed"
        );
        assert_eq!(
            actual.scope_label, expected_metadata.scope_label,
            "bucket `{bucket_name}` scope_label changed"
        );
        assert_eq!(
            actual.owner, expected_metadata.owner,
            "bucket `{bucket_name}` owner changed"
        );
        assert_eq!(
            actual.expensive, expected_metadata.expensive,
            "bucket `{bucket_name}` expensive marker changed"
        );
        assert_eq!(
            actual.notes.as_deref(),
            expected_metadata.notes,
            "bucket `{bucket_name}` notes changed"
        );
    }
}

#[test]
fn manifest_contract_tests_unix_only_lifecycle_filter_allows_zero_tests() {
    let manifest = load_checked_in_manifest();
    let lifecycle = &manifest.buckets["lifecycle"];
    let command = lifecycle
        .commands
        .iter()
        .find(|command| command.contains("tests::integration::daemon_lifecycle"))
        .expect("lifecycle bucket should include daemon_lifecycle integration coverage");

    assert!(
        command.contains("--no-tests pass"),
        "daemon_lifecycle integration tests are cfg(unix); this command must not fail the lifecycle bucket on Windows when it legitimately matches zero tests: {command}"
    );
}

#[test]
fn manifest_contract_tests_checked_in_manifest_includes_rebalancing_hints() {
    let contents = fs::read_to_string(manifest_path()).unwrap();

    assert!(contents.contains("# Fast local confidence buckets."));
    assert!(
        contents.contains("# Expected seconds are first-pass estimates for later rebalancing.")
    );
    assert!(contents.contains("# Smoke: keep this tiny for frequent local runs."));
    assert!(contents.contains("# System coverage kept separate from fast local buckets."));
    assert!(contents.contains("# Dogfood stays isolated because it is the long pole."));
}

#[test]
fn manifest_contract_tests_get_symbols_bucket_omits_ignored_target_filtering_module() {
    let manifest = load_checked_in_manifest();
    let bucket = &manifest.buckets["tools-get-symbols"];

    assert!(
        !bucket
            .commands
            .iter()
            .any(|command| command_selects_forbidden_target_filtering_module(command)),
        "get_symbols_target_filtering.rs only contains ignored slow tests; dev must not select it"
    );
    assert!(
        manifest.buckets["tools-dogfood-repo-index"]
            .commands
            .iter()
            .any(|command| command.contains("get_symbols_target_filtering_dogfood")),
        "non-ignored target-filtering coverage should stay isolated in dogfood"
    );
}

#[test]
fn manifest_contract_tests_forbidden_target_filtering_predicate_rejects_exact_and_nested_filters() {
    let cases = [
        (
            "cargo nextest run --lib tests::tools::get_symbols_target_filtering -- --skip search_quality",
            true,
        ),
        (
            "cargo nextest run --lib tests::tools::get_symbols_target_filtering::slow_target_filter -- --skip search_quality",
            true,
        ),
        (
            "cargo nextest run --lib --run-ignored only tests::tools::get_symbols_target_filtering",
            true,
        ),
        (
            "cargo nextest run --lib tests::tools::get_symbols_target_filtering_dogfood -- --skip search_quality",
            false,
        ),
        (
            "cargo nextest run --lib tests::tools::get_symbols:: -- --skip search_quality",
            false,
        ),
    ];

    for (command, expected) in cases {
        assert_eq!(
            command_selects_forbidden_target_filtering_module(command),
            expected,
            "unexpected target-filtering predicate result for `{command}`"
        );
    }
}

fn command_selects_forbidden_target_filtering_module(command: &str) -> bool {
    const FORBIDDEN_MODULE: &str = "tests::tools::get_symbols_target_filtering";

    command.split_whitespace().any(|token| {
        token == FORBIDDEN_MODULE
            || token
                .strip_prefix(FORBIDDEN_MODULE)
                .is_some_and(|suffix| suffix.starts_with("::"))
    })
}

fn manifest_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test_tiers.toml")
}

fn load_checked_in_manifest() -> TestManifest {
    TestManifest::load(manifest_path()).unwrap()
}

fn expected_tiers() -> BTreeMap<String, Vec<String>> {
    BTreeMap::from([
        (
            "dev".to_string(),
            vec![
                "cli".to_string(),
                "xtask-runner".to_string(),
                "core-database".to_string(),
                "core-embeddings".to_string(),
                "tools-get-context-pipeline".to_string(),
                "tools-get-context-format".to_string(),
                "tools-get-context-graph".to_string(),
                "tools-search-tantivy".to_string(),
                "tools-search-line".to_string(),
                "tools-search-file-mode".to_string(),
                "tools-search-zero-hit".to_string(),
                "tools-search-promotion".to_string(),
                "tools-search-format-quality".to_string(),
                "tools-search-context".to_string(),
                "tools-search-text".to_string(),
                "tools-search-hybrid".to_string(),
                "tools-search-query".to_string(),
                "tools-search-unified".to_string(),
                "tools-workspace".to_string(),
                "tools-workspace-targeting".to_string(),
                "tools-get-symbols".to_string(),
                "tools-editing".to_string(),
                "tools-deep-dive".to_string(),
                "tools-call-path".to_string(),
                "tools-fast-refs".to_string(),
                "tools-blast-spillover".to_string(),
                "tools-refactoring".to_string(),
                "tools-metrics".to_string(),
                "tools-format-filter".to_string(),
                "analysis".to_string(),
                "core-fast".to_string(),
                "core-handler-telemetry".to_string(),
                "daemon".to_string(),
                "dashboard".to_string(),
                "extractor-units".to_string(),
            ],
        ),
        (
            "dogfood".to_string(),
            vec![
                "tools-dogfood-repo-index".to_string(),
                "search-quality".to_string(),
            ],
        ),
        (
            "full".to_string(),
            vec![
                "cli".to_string(),
                "xtask-runner".to_string(),
                "core-database".to_string(),
                "core-embeddings".to_string(),
                "tools-get-context-pipeline".to_string(),
                "tools-get-context-format".to_string(),
                "tools-get-context-graph".to_string(),
                "tools-search-tantivy".to_string(),
                "tools-search-line".to_string(),
                "tools-search-file-mode".to_string(),
                "tools-search-zero-hit".to_string(),
                "tools-search-promotion".to_string(),
                "tools-search-format-quality".to_string(),
                "tools-search-context".to_string(),
                "tools-search-text".to_string(),
                "tools-search-hybrid".to_string(),
                "tools-search-query".to_string(),
                "tools-search-unified".to_string(),
                "tools-workspace".to_string(),
                "tools-workspace-targeting".to_string(),
                "tools-get-symbols".to_string(),
                "tools-editing".to_string(),
                "tools-deep-dive".to_string(),
                "tools-call-path".to_string(),
                "tools-fast-refs".to_string(),
                "tools-blast-spillover".to_string(),
                "tools-refactoring".to_string(),
                "tools-metrics".to_string(),
                "tools-format-filter".to_string(),
                "analysis".to_string(),
                "core-fast".to_string(),
                "core-handler-telemetry".to_string(),
                "daemon".to_string(),
                "dashboard".to_string(),
                "projection".to_string(),
                "transport".to_string(),
                "lifecycle".to_string(),
                "workspace-runtime".to_string(),
                "workspace-init".to_string(),
                "documentation-indexing".to_string(),
                "integration".to_string(),
                "tools-dogfood-repo-index".to_string(),
                "search-quality".to_string(),
                "extractor-units".to_string(),
            ],
        ),
        (
            "nano".to_string(),
            vec!["core-database".to_string(), "core-fast".to_string()],
        ),
        (
            "smoke".to_string(),
            vec![
                "cli".to_string(),
                "core-database".to_string(),
                "core-embeddings".to_string(),
                "tools-get-context-pipeline".to_string(),
            ],
        ),
        (
            "system".to_string(),
            vec![
                "projection".to_string(),
                "transport".to_string(),
                "lifecycle".to_string(),
                "workspace-runtime".to_string(),
                "workspace-init".to_string(),
                "documentation-indexing".to_string(),
                "integration".to_string(),
            ],
        ),
    ])
}

fn expected_blocked_tiers() -> BTreeMap<String, String> {
    BTreeMap::new()
}
