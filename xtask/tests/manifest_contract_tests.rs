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
            .any(|command| command.contains("tests::tools::get_symbols_target_filtering::")),
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
                "daemon".to_string(),
                "dashboard".to_string(),
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
                "daemon".to_string(),
                "dashboard".to_string(),
                "projection".to_string(),
                "transport".to_string(),
                "lifecycle".to_string(),
                "workspace-runtime".to_string(),
                "workspace-init".to_string(),
                "integration".to_string(),
                "tools-dogfood-repo-index".to_string(),
                "search-quality".to_string(),
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
                "integration".to_string(),
            ],
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
                expected_seconds: 45,
                timeout_seconds: 120,
                commands: &[
                    "cargo nextest run --lib tests::cli_tests",
                    "cargo nextest run --lib tests::cli_execution_tests",
                    "cargo nextest run --lib tests::cli_tools_tests",
                    "cargo build",
                    "cargo nextest run --lib --run-ignored only tests::cli::",
                ],
            },
        ),
        (
            "xtask-runner",
            ExpectedBucket {
                expected_seconds: 15,
                timeout_seconds: 60,
                commands: &["cargo nextest run -p xtask"],
            },
        ),
        (
            "core-database",
            ExpectedBucket {
                expected_seconds: 5,
                timeout_seconds: 30,
                commands: &[
                    "cargo nextest run --lib tests::core::database -- --skip search_quality",
                ],
            },
        ),
        (
            "core-embeddings",
            ExpectedBucket {
                expected_seconds: 15,
                timeout_seconds: 60,
                commands: &[
                    "cargo nextest run --lib tests::core::embedding_provider -- --skip search_quality",
                    "cargo nextest run --lib tests::core::embedding_metadata -- --skip search_quality",
                    "cargo nextest run --lib tests::core::embedding_deps -- --skip search_quality",
                    "cargo nextest run --lib tests::core::embedding_sidecar_protocol -- --skip search_quality",
                    "cargo nextest run --lib tests::core::embedding_sidecar_provider -- --skip search_quality",
                    "cargo nextest run --lib tests::core::sidecar_supervisor_tests -- --skip search_quality",
                    "cargo nextest run --lib tests::core::sidecar_embedding_tests -- --skip search_quality",
                ],
            },
        ),
        (
            "analysis",
            ExpectedBucket {
                expected_seconds: 30,
                timeout_seconds: 90,
                commands: &["cargo nextest run --lib tests::analysis -- --skip search_quality"],
            },
        ),
        (
            "core-fast",
            ExpectedBucket {
                expected_seconds: 20,
                timeout_seconds: 60,
                commands: &[
                    "cargo nextest run --lib tests::main_error_handling -- --skip search_quality",
                    "cargo nextest run --lib tests::regression_prevention_tests -- --skip search_quality",
                    "cargo nextest run --lib utils::paths::tests -- --skip search_quality",
                    "cargo nextest run --lib utils::string_similarity::tests -- --skip search_quality",
                    "cargo nextest run --lib watcher::filtering::tests -- --skip search_quality",
                    "cargo nextest run --lib tests::core::database_lightweight_query -- --skip search_quality",
                    "cargo nextest run --lib tests::core::handler -- --skip search_quality",
                    "cargo nextest run --lib tests::core::language -- --skip search_quality",
                    "cargo nextest run --lib tests::core::memory_vectors -- --skip search_quality",
                    "cargo nextest run --lib tests::core::paths -- --skip search_quality",
                    "cargo nextest run --lib tests::core::vector_storage -- --skip search_quality",
                ],
            },
        ),
        (
            "extractors",
            ExpectedBucket {
                expected_seconds: 60,
                timeout_seconds: 180,
                commands: &[
                    "cargo nextest run -p julie-extractors golden",
                    "cargo nextest run -p julie-extractors capability_matrix",
                    "cargo xtask certify tree-sitter --check",
                    "cargo nextest run -p julie-extractors --test downstream_smoke julie_extractors_works_as_path_dependency_in_downstream_crate",
                ],
            },
        ),
        (
            "daemon",
            ExpectedBucket {
                expected_seconds: 12,
                timeout_seconds: 60,
                commands: &["cargo nextest run --lib tests::daemon -- --skip search_quality"],
            },
        ),
        (
            "dashboard",
            ExpectedBucket {
                expected_seconds: 12,
                timeout_seconds: 60,
                commands: &["cargo nextest run --lib tests::dashboard -- --skip search_quality"],
            },
        ),
        (
            "parser-upgrade",
            ExpectedBucket {
                expected_seconds: 60,
                timeout_seconds: 180,
                commands: &[
                    "cargo nextest run -p julie-extractors -E 'test(golden) | test(capability_matrix) | test(parser_upgrade)'",
                    "cargo nextest run --lib real_world_parser_upgrade_contracts_assert_expected_outputs",
                ],
            },
        ),
        (
            "integration",
            ExpectedBucket {
                expected_seconds: 130,
                timeout_seconds: 240,
                commands: &["cargo nextest run --lib tests::integration -- --skip search_quality"],
            },
        ),
        (
            "lifecycle",
            ExpectedBucket {
                expected_seconds: 30,
                timeout_seconds: 90,
                commands: &[
                    "cargo nextest run --lib tests::daemon::lifecycle -- --skip search_quality",
                    "cargo nextest run --lib tests::integration::daemon_lifecycle -- --skip search_quality",
                ],
            },
        ),
        (
            "projection",
            ExpectedBucket {
                expected_seconds: 45,
                timeout_seconds: 120,
                commands: &[
                    "cargo nextest run --lib tests::integration::projection_repair -- --skip search_quality",
                ],
            },
        ),
        (
            "search-quality",
            ExpectedBucket {
                expected_seconds: 180,
                timeout_seconds: 300,
                commands: &["cargo nextest run --lib search_quality"],
            },
        ),
        (
            "transport",
            ExpectedBucket {
                expected_seconds: 35,
                timeout_seconds: 90,
                commands: &[
                    "cargo nextest run --lib tests::adapter -- --skip search_quality",
                    "cargo nextest run --lib tests::daemon::transport -- --skip search_quality",
                    "cargo nextest run --lib tests::daemon::http_transport -- --skip search_quality",
                    "cargo nextest run --lib tests::daemon::mcp_session -- --skip search_quality",
                ],
            },
        ),
        (
            "tools-dogfood-repo-index",
            ExpectedBucket {
                expected_seconds: 200,
                timeout_seconds: 450,
                commands: &[
                    "cargo nextest run --lib tests::tools::get_symbols_target_filtering_dogfood -- --skip search_quality",
                ],
            },
        ),
        (
            "tools-get-context-pipeline",
            ExpectedBucket {
                expected_seconds: 15,
                timeout_seconds: 60,
                commands: &[
                    "cargo nextest run --lib tests::tools::get_context_pipeline_tests -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::get_context_pipeline_relevance_tests -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::get_context_relevance_tests -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::get_context_scoring_tests -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::get_context_quality_tests -- --skip search_quality",
                ],
            },
        ),
        (
            "tools-get-context-format",
            ExpectedBucket {
                expected_seconds: 12,
                timeout_seconds: 45,
                commands: &[
                    "cargo nextest run --lib tests::tools::get_context_allocation_tests -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::get_context_formatting_tests -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::get_context_token_budget_tests -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::get_context_tests -- --skip search_quality",
                ],
            },
        ),
        (
            "tools-get-context-graph",
            ExpectedBucket {
                expected_seconds: 12,
                timeout_seconds: 45,
                commands: &[
                    "cargo nextest run --lib tests::tools::get_context_graph_expansion_tests -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::get_context_task_inputs_tests -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::get_context_primary_rebind_tests -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::get_context_target_workspace_metrics_tests -- --skip search_quality",
                ],
            },
        ),
        (
            "tools-editing",
            ExpectedBucket {
                expected_seconds: 30,
                timeout_seconds: 90,
                commands: &[
                    "cargo nextest run --lib tests::tools::editing:: -- --skip search_quality",
                ],
            },
        ),
        (
            "tools-format-filter",
            ExpectedBucket {
                expected_seconds: 15,
                timeout_seconds: 60,
                commands: &[
                    "cargo nextest run --lib tests::tools::filtering_tests -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::formatting_tests -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::query_classification_tests -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::phase4_token_savings -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::smart_read -- --skip search_quality",
                ],
            },
        ),
        (
            "tools-get-symbols",
            ExpectedBucket {
                expected_seconds: 40,
                timeout_seconds: 120,
                commands: &[
                    "cargo nextest run --lib tests::tools::get_symbols:: -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::get_symbols_relative_paths -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::get_symbols_smart_read -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::get_symbols_target_workspace -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::get_symbols_token -- --skip search_quality",
                ],
            },
        ),
        (
            "tools-metrics",
            ExpectedBucket {
                expected_seconds: 10,
                timeout_seconds: 45,
                commands: &[
                    "cargo nextest run --lib tests::tools::metrics:: -- --skip search_quality",
                ],
            },
        ),
        (
            "tools-deep-dive",
            ExpectedBucket {
                expected_seconds: 12,
                timeout_seconds: 60,
                commands: &[
                    "cargo nextest run --lib tests::tools::deep_dive_tests -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::deep_dive_primary_rebind_tests -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::deep_dive_regression_tests -- --skip search_quality",
                ],
            },
        ),
        (
            "tools-call-path",
            ExpectedBucket {
                expected_seconds: 8,
                timeout_seconds: 45,
                commands: &[
                    "cargo nextest run --lib tests::tools::call_path_tests -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::call_path_disambiguation_tests -- --skip search_quality",
                ],
            },
        ),
        (
            "tools-fast-refs",
            ExpectedBucket {
                expected_seconds: 8,
                timeout_seconds: 45,
                commands: &[
                    "cargo nextest run --lib tests::tools::fast_refs_primary_rebind_tests -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::target_workspace_fast_refs_tests -- --skip search_quality",
                ],
            },
        ),
        (
            "tools-blast-spillover",
            ExpectedBucket {
                expected_seconds: 10,
                timeout_seconds: 45,
                commands: &[
                    "cargo nextest run --lib tests::tools::blast_radius -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::spillover_tests -- --skip search_quality",
                ],
            },
        ),
        (
            "tools-refactoring",
            ExpectedBucket {
                expected_seconds: 15,
                timeout_seconds: 60,
                commands: &[
                    "cargo nextest run --lib tests::tools::refactoring:: -- --skip search_quality",
                ],
            },
        ),
        (
            "tools-search-context",
            ExpectedBucket {
                expected_seconds: 15,
                timeout_seconds: 60,
                commands: &[
                    "cargo nextest run --lib tests::tools::search_context_lines -- --skip search_quality",
                ],
            },
        ),
        (
            "tools-search-hybrid",
            ExpectedBucket {
                expected_seconds: 25,
                timeout_seconds: 90,
                commands: &[
                    "cargo nextest run --lib tests::tools::hybrid_search_tests -- --skip search_quality",
                ],
            },
        ),
        (
            "tools-search-line",
            ExpectedBucket {
                expected_seconds: 50,
                timeout_seconds: 150,
                commands: &[
                    "cargo nextest run --lib tests::tools::search::line_ -- --skip search_quality",
                ],
            },
        ),
        (
            "tools-search-file-mode",
            ExpectedBucket {
                expected_seconds: 20,
                timeout_seconds: 90,
                commands: &[
                    "cargo nextest run --lib tests::tools::search::file_ -- --skip search_quality",
                ],
            },
        ),
        (
            "tools-search-zero-hit",
            ExpectedBucket {
                expected_seconds: 15,
                timeout_seconds: 60,
                commands: &[
                    "cargo nextest run --lib tests::tools::search::primary_workspace_bug -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::search::zero_hit_reason_tests -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::search::zero_hit_reason_propagation_tests -- --skip search_quality",
                ],
            },
        ),
        (
            "tools-search-query",
            ExpectedBucket {
                expected_seconds: 5,
                timeout_seconds: 30,
                commands: &[
                    "cargo nextest run --lib tools::search::query_preprocessor::tests -- --skip search_quality",
                ],
            },
        ),
        (
            "tools-search-promotion",
            ExpectedBucket {
                expected_seconds: 15,
                timeout_seconds: 60,
                commands: &[
                    "cargo nextest run --lib tests::tools::search::definition_ -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::search::promotion_tests -- --skip search_quality",
                ],
            },
        ),
        (
            "tools-search-format-quality",
            ExpectedBucket {
                expected_seconds: 25,
                timeout_seconds: 90,
                commands: &[
                    "cargo nextest run --lib tests::tools::search::annotation_search_tests -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::search::content_scoring_tests -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::search::fast_search_regression_tests -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::search::lean_format_tests -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::search::quality -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::search::race_condition -- --skip search_quality",
                ],
            },
        ),
        (
            "tools-search-tantivy",
            ExpectedBucket {
                expected_seconds: 35,
                timeout_seconds: 120,
                commands: &[
                    "cargo nextest run --lib tests::tools::search::tantivy_ -- --skip search_quality",
                ],
            },
        ),
        (
            "tools-search-text",
            ExpectedBucket {
                expected_seconds: 25,
                timeout_seconds: 90,
                commands: &[
                    "cargo nextest run --lib tests::tools::text_search_tantivy -- --skip search_quality",
                ],
            },
        ),
        (
            "tools-workspace",
            ExpectedBucket {
                expected_seconds: 20,
                timeout_seconds: 90,
                commands: &[
                    "cargo nextest run --lib tests::tools::workspace::discovery -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::workspace::file_policy -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::workspace::index_embedding_tests -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::workspace::isolation -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::workspace::management_token -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::workspace::mod_tests -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::workspace::processor -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::workspace::resolver -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::workspace::utils -- --skip search_quality",
                ],
            },
        ),
        (
            "tools-workspace-targeting",
            ExpectedBucket {
                expected_seconds: 25,
                timeout_seconds: 90,
                commands: &[
                    "cargo nextest run --lib tests::tools::workspace::global_targeting -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::workspace::refresh_routing -- --skip search_quality",
                ],
            },
        ),
        (
            "workspace-init",
            ExpectedBucket {
                expected_seconds: 5,
                timeout_seconds: 30,
                commands: &[
                    "cargo nextest run --lib tests::core::workspace_init -- --skip search_quality",
                ],
            },
        ),
        (
            "workspace-runtime",
            ExpectedBucket {
                expected_seconds: 45,
                timeout_seconds: 120,
                commands: &[
                    "cargo nextest run --lib tests::daemon::workspace_pool -- --skip search_quality",
                    "cargo nextest run --lib tests::daemon::watcher_pool -- --skip search_quality",
                    "cargo nextest run --lib tests::daemon::workspace_cleanup -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::workspace::registry -- --skip search_quality",
                ],
            },
        ),
    ])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ExpectedBucketMetadata {
    scope_label: &'static str,
    owner: &'static str,
    expensive: bool,
    notes: Option<&'static str>,
}

fn expected_bucket_metadata() -> BTreeMap<&'static str, ExpectedBucketMetadata> {
    BTreeMap::from([
        (
            "cli",
            ExpectedBucketMetadata {
                scope_label: "cli",
                owner: "lead",
                expensive: false,
                notes: Some("CLI contract and execution path"),
            },
        ),
        (
            "xtask-runner",
            ExpectedBucketMetadata {
                scope_label: "tooling",
                owner: "lead",
                expensive: false,
                notes: Some("xtask runner and manifest contract"),
            },
        ),
        (
            "core-database",
            ExpectedBucketMetadata {
                scope_label: "core",
                owner: "lead",
                expensive: false,
                notes: Some("core database layer"),
            },
        ),
        (
            "core-embeddings",
            ExpectedBucketMetadata {
                scope_label: "core",
                owner: "lead",
                expensive: false,
                notes: Some("embedding stack"),
            },
        ),
        (
            "analysis",
            ExpectedBucketMetadata {
                scope_label: "tooling",
                owner: "lead",
                expensive: false,
                notes: Some("post-indexing analysis (test quality, risk, linkage)"),
            },
        ),
        (
            "core-fast",
            ExpectedBucketMetadata {
                scope_label: "core",
                owner: "lead",
                expensive: false,
                notes: Some("misc fast core coverage"),
            },
        ),
        (
            "extractors",
            ExpectedBucketMetadata {
                scope_label: "extractors",
                owner: "lead",
                expensive: false,
                notes: Some("extractor golden, capability matrix, and Pillar-3 downstream-consumer gate"),
            },
        ),
        (
            "daemon",
            ExpectedBucketMetadata {
                scope_label: "daemon",
                owner: "lead",
                expensive: false,
                notes: Some("daemon-mode protocol coverage"),
            },
        ),
        (
            "dashboard",
            ExpectedBucketMetadata {
                scope_label: "dashboard",
                owner: "lead",
                expensive: false,
                notes: Some("dashboard route coverage"),
            },
        ),
        (
            "parser-upgrade",
            ExpectedBucketMetadata {
                scope_label: "extractors",
                owner: "lead",
                expensive: false,
                notes: Some("parser dependency upgrade gate"),
            },
        ),
        (
            "integration",
            ExpectedBucketMetadata {
                scope_label: "system",
                owner: "lead",
                expensive: false,
                notes: Some("cross-cutting integration"),
            },
        ),
        (
            "lifecycle",
            ExpectedBucketMetadata {
                scope_label: "system",
                owner: "lead",
                expensive: false,
                notes: Some(
                    "daemon lifecycle and restart handoff (backed by daemon::lifecycle + integration::daemon_lifecycle)",
                ),
            },
        ),
        (
            "projection",
            ExpectedBucketMetadata {
                scope_label: "system",
                owner: "lead",
                expensive: false,
                notes: Some("projection repair gate (backed by integration::projection_repair)"),
            },
        ),
        (
            "search-quality",
            ExpectedBucketMetadata {
                scope_label: "dogfood",
                owner: "lead",
                expensive: true,
                notes: Some("heavy fixture-backed relevance suite"),
            },
        ),
        (
            "transport",
            ExpectedBucketMetadata {
                scope_label: "system",
                owner: "lead",
                expensive: false,
                notes: Some(
                    "adapter + HTTP transport coverage (backed by adapter/daemon transport tests)",
                ),
            },
        ),
        (
            "tools-dogfood-repo-index",
            ExpectedBucketMetadata {
                scope_label: "dogfood",
                owner: "lead",
                expensive: true,
                notes: Some("indexes the julie repository"),
            },
        ),
        (
            "tools-get-context-pipeline",
            ExpectedBucketMetadata {
                scope_label: "tooling",
                owner: "lead",
                expensive: false,
                notes: Some("get_context pipeline + scoring/relevance/quality"),
            },
        ),
        (
            "tools-get-context-format",
            ExpectedBucketMetadata {
                scope_label: "tooling",
                owner: "lead",
                expensive: false,
                notes: Some("get_context formatting + token budget"),
            },
        ),
        (
            "tools-get-context-graph",
            ExpectedBucketMetadata {
                scope_label: "tooling",
                owner: "lead",
                expensive: false,
                notes: Some("get_context graph expansion + cross-workspace coverage"),
            },
        ),
        (
            "tools-editing",
            ExpectedBucketMetadata {
                scope_label: "tooling",
                owner: "lead",
                expensive: false,
                notes: Some("editing tools"),
            },
        ),
        (
            "tools-format-filter",
            ExpectedBucketMetadata {
                scope_label: "tooling",
                owner: "lead",
                expensive: false,
                notes: Some("formatting, filtering, and token budget helpers"),
            },
        ),
        (
            "tools-get-symbols",
            ExpectedBucketMetadata {
                scope_label: "tooling",
                owner: "lead",
                expensive: false,
                notes: Some("get_symbols surface"),
            },
        ),
        (
            "tools-metrics",
            ExpectedBucketMetadata {
                scope_label: "tooling",
                owner: "lead",
                expensive: false,
                notes: Some("metrics tools"),
            },
        ),
        (
            "tools-deep-dive",
            ExpectedBucketMetadata {
                scope_label: "tooling",
                owner: "lead",
                expensive: false,
                notes: Some("deep_dive tool coverage"),
            },
        ),
        (
            "tools-call-path",
            ExpectedBucketMetadata {
                scope_label: "tooling",
                owner: "lead",
                expensive: false,
                notes: Some("call_path tool coverage"),
            },
        ),
        (
            "tools-fast-refs",
            ExpectedBucketMetadata {
                scope_label: "tooling",
                owner: "lead",
                expensive: false,
                notes: Some("fast_refs and target-workspace ref coverage"),
            },
        ),
        (
            "tools-blast-spillover",
            ExpectedBucketMetadata {
                scope_label: "tooling",
                owner: "lead",
                expensive: false,
                notes: Some("blast_radius and spillover coverage"),
            },
        ),
        (
            "tools-refactoring",
            ExpectedBucketMetadata {
                scope_label: "tooling",
                owner: "lead",
                expensive: false,
                notes: Some("refactoring tools"),
            },
        ),
        (
            "tools-search-context",
            ExpectedBucketMetadata {
                scope_label: "tooling",
                owner: "lead",
                expensive: false,
                notes: Some("search context line coverage"),
            },
        ),
        (
            "tools-search-hybrid",
            ExpectedBucketMetadata {
                scope_label: "tooling",
                owner: "lead",
                expensive: false,
                notes: Some("hybrid search coverage"),
            },
        ),
        (
            "tools-search-line",
            ExpectedBucketMetadata {
                scope_label: "tooling",
                owner: "lead",
                expensive: false,
                notes: Some("line-mode search coverage"),
            },
        ),
        (
            "tools-search-file-mode",
            ExpectedBucketMetadata {
                scope_label: "tooling",
                owner: "lead",
                expensive: false,
                notes: Some("file-mode search coverage"),
            },
        ),
        (
            "tools-search-zero-hit",
            ExpectedBucketMetadata {
                scope_label: "tooling",
                owner: "lead",
                expensive: false,
                notes: Some("zero-hit reason and primary-workspace regression coverage"),
            },
        ),
        (
            "tools-search-query",
            ExpectedBucketMetadata {
                scope_label: "tooling",
                owner: "lead",
                expensive: false,
                notes: Some("search query preprocessing"),
            },
        ),
        (
            "tools-search-promotion",
            ExpectedBucketMetadata {
                scope_label: "tooling",
                owner: "lead",
                expensive: false,
                notes: Some("definition and promotion ranking coverage"),
            },
        ),
        (
            "tools-search-format-quality",
            ExpectedBucketMetadata {
                scope_label: "tooling",
                owner: "lead",
                expensive: false,
                notes: Some("search formatting, scoring, quality, and race coverage"),
            },
        ),
        (
            "tools-search-tantivy",
            ExpectedBucketMetadata {
                scope_label: "tooling",
                owner: "lead",
                expensive: false,
                notes: Some("tantivy-backed search coverage"),
            },
        ),
        (
            "tools-search-text",
            ExpectedBucketMetadata {
                scope_label: "tooling",
                owner: "lead",
                expensive: false,
                notes: Some("text search coverage"),
            },
        ),
        (
            "tools-workspace",
            ExpectedBucketMetadata {
                scope_label: "tooling",
                owner: "lead",
                expensive: false,
                notes: Some("workspace management (excluding heavy targeting fixtures)"),
            },
        ),
        (
            "tools-workspace-targeting",
            ExpectedBucketMetadata {
                scope_label: "tooling",
                owner: "lead",
                expensive: false,
                notes: Some("workspace global-targeting and refresh-routing coverage"),
            },
        ),
        (
            "workspace-init",
            ExpectedBucketMetadata {
                scope_label: "system",
                owner: "lead",
                expensive: false,
                notes: Some("workspace initialization path"),
            },
        ),
        (
            "workspace-runtime",
            ExpectedBucketMetadata {
                scope_label: "system",
                owner: "lead",
                expensive: false,
                notes: Some(
                    "workspace pool and watcher runtime ownership (backed by daemon workspace/watcher tests)",
                ),
            },
        ),
    ])
}
