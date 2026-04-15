use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use xtask::manifest::TestManifest;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExpectedBucket {
    expected_seconds: u64,
    timeout_seconds: u64,
    commands: &'static [&'static str],
}

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
    }
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
                "core-database".to_string(),
                "core-embeddings".to_string(),
                "tools-get-context".to_string(),
                "tools-search".to_string(),
                "tools-workspace".to_string(),
                "tools-misc".to_string(),
                "core-fast".to_string(),
                "daemon".to_string(),
            ],
        ),
        ("dogfood".to_string(), vec!["search-quality".to_string()]),
        (
            "full".to_string(),
            vec![
                "cli".to_string(),
                "core-database".to_string(),
                "core-embeddings".to_string(),
                "tools-get-context".to_string(),
                "tools-search".to_string(),
                "tools-workspace".to_string(),
                "tools-misc".to_string(),
                "core-fast".to_string(),
                "daemon".to_string(),
                "workspace-init".to_string(),
                "integration".to_string(),
                "search-quality".to_string(),
            ],
        ),
        (
            "smoke".to_string(),
            vec![
                "cli".to_string(),
                "core-database".to_string(),
                "core-embeddings".to_string(),
                "tools-get-context".to_string(),
            ],
        ),
        (
            "system".to_string(),
            vec!["workspace-init".to_string(), "integration".to_string()],
        ),
    ])
}

fn expected_blocked_tiers() -> BTreeMap<String, String> {
    BTreeMap::new()
}

fn expected_buckets() -> BTreeMap<&'static str, ExpectedBucket> {
    BTreeMap::from([
        (
            "cli",
            ExpectedBucket {
                expected_seconds: 30,
                timeout_seconds: 90,
                commands: &["cargo test --lib tests::cli_tests", "cargo test -p xtask"],
            },
        ),
        (
            "core-database",
            ExpectedBucket {
                expected_seconds: 5,
                timeout_seconds: 30,
                commands: &["cargo test --lib tests::core::database -- --skip search_quality"],
            },
        ),
        (
            "core-embeddings",
            ExpectedBucket {
                expected_seconds: 15,
                timeout_seconds: 60,
                commands: &[
                    "cargo test --lib tests::core::embedding_provider -- --skip search_quality",
                    "cargo test --lib tests::core::embedding_metadata -- --skip search_quality",
                    "cargo test --lib tests::core::embedding_deps -- --skip search_quality",
                    "cargo test --lib tests::core::embedding_sidecar_protocol -- --skip search_quality",
                    "cargo test --lib tests::core::embedding_sidecar_provider -- --skip search_quality",
                    "cargo test --lib tests::core::sidecar_supervisor_tests -- --skip search_quality",
                    "cargo test --lib tests::core::sidecar_embedding_tests -- --skip search_quality",
                ],
            },
        ),
        (
            "core-fast",
            ExpectedBucket {
                expected_seconds: 10,
                timeout_seconds: 45,
                commands: &[
                    "cargo test --lib tests::main_error_handling -- --skip search_quality",
                    "cargo test --lib tests::regression_prevention_tests -- --skip search_quality",
                    "cargo test --lib utils::paths::tests -- --skip search_quality",
                    "cargo test --lib utils::string_similarity::tests -- --skip search_quality",
                    "cargo test --lib watcher::filtering::tests -- --skip search_quality",
                    "cargo test --lib watcher::tests -- --skip search_quality",
                    "cargo test --lib tests::core::database_lightweight_query -- --skip search_quality",
                    "cargo test --lib tests::core::handler -- --skip search_quality",
                    "cargo test --lib tests::core::language -- --skip search_quality",
                    "cargo test --lib tests::core::memory_vectors -- --skip search_quality",
                    "cargo test --lib tests::core::paths -- --skip search_quality",
                    "cargo test --lib tests::core::tracing -- --skip search_quality",
                    "cargo test --lib tests::core::vector_storage -- --skip search_quality",
                ],
            },
        ),
        (
            "daemon",
            ExpectedBucket {
                expected_seconds: 12,
                timeout_seconds: 60,
                commands: &["cargo test --lib tests::daemon -- --skip search_quality"],
            },
        ),
        (
            "integration",
            ExpectedBucket {
                expected_seconds: 30,
                timeout_seconds: 120,
                commands: &["cargo test --lib tests::integration -- --skip search_quality"],
            },
        ),
        (
            "search-quality",
            ExpectedBucket {
                expected_seconds: 390,
                timeout_seconds: 480,
                commands: &["cargo test --lib search_quality"],
            },
        ),
        (
            "tools-get-context",
            ExpectedBucket {
                expected_seconds: 10,
                timeout_seconds: 45,
                commands: &[
                    "cargo test --lib tests::tools::get_context_allocation_tests -- --skip search_quality",
                    "cargo test --lib tests::tools::get_context_formatting_tests -- --skip search_quality",
                    "cargo test --lib tests::tools::get_context_graph_expansion_tests -- --skip search_quality",
                    "cargo test --lib tests::tools::get_context_pipeline_relevance_tests -- --skip search_quality",
                    "cargo test --lib tests::tools::get_context_pipeline_tests -- --skip search_quality",
                    "cargo test --lib tests::tools::get_context_quality_tests -- --skip search_quality",
                    "cargo test --lib tests::tools::get_context_relevance_tests -- --skip search_quality",
                    "cargo test --lib tests::tools::get_context_scoring_tests -- --skip search_quality",
                    "cargo test --lib tests::tools::get_context_tests -- --skip search_quality",
                    "cargo test --lib tests::tools::get_context_token_budget_tests -- --skip search_quality",
                ],
            },
        ),
        (
            "tools-misc",
            ExpectedBucket {
                expected_seconds: 200,
                timeout_seconds: 450,
                commands: &[
                    "cargo test --lib tests::tools::get_symbols -- --skip search_quality",
                    "cargo test --lib tests::tools::get_symbols_reference_workspace -- --skip search_quality",
                    "cargo test --lib tests::tools::get_symbols_relative_paths -- --skip search_quality",
                    "cargo test --lib tests::tools::get_symbols_smart_read -- --skip search_quality",
                    "cargo test --lib tests::tools::get_symbols_target_filtering -- --skip search_quality",
                    "cargo test --lib tests::tools::get_symbols_token -- --skip search_quality",
                    "cargo test --lib tests::tools::smart_read -- --skip search_quality",
                    "cargo test --lib tests::tools::editing -- --skip search_quality",
                    "cargo test --lib tests::tools::deep_dive_tests -- --skip search_quality",
                    "cargo test --lib tests::tools::refactoring -- --skip search_quality",
                    "cargo test --lib tests::tools::filtering_tests -- --skip search_quality",
                    "cargo test --lib tests::tools::formatting_tests -- --skip search_quality",
                    "cargo test --lib tests::tools::reference_workspace_fast_refs_tests -- --skip search_quality",
                    "cargo test --lib tools::search::query_preprocessor::tests -- --skip search_quality",
                    "cargo test --lib tests::tools::metrics -- --skip search_quality",
                ],
            },
        ),
        (
            "tools-search",
            ExpectedBucket {
                expected_seconds: 25,
                timeout_seconds: 90,
                commands: &[
                    "cargo test --lib tests::tools::search -- --skip search_quality",
                    "cargo test --lib tests::tools::search_context_lines -- --skip search_quality",
                    "cargo test --lib tests::tools::text_search_tantivy -- --skip search_quality",
                    "cargo test --lib tests::tools::hybrid_search_tests -- --skip search_quality",
                ],
            },
        ),
        (
            "tools-workspace",
            ExpectedBucket {
                expected_seconds: 40,
                timeout_seconds: 120,
                commands: &["cargo test --lib tests::tools::workspace -- --skip search_quality"],
            },
        ),
        (
            "workspace-init",
            ExpectedBucket {
                expected_seconds: 360,
                timeout_seconds: 480,
                commands: &[
                    "cargo test --lib tests::core::workspace_init -- --skip search_quality",
                ],
            },
        ),
    ])
}
