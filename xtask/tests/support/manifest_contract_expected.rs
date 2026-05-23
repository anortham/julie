use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExpectedBucket {
    pub(crate) expected_seconds: u64,
    pub(crate) timeout_seconds: u64,
    pub(crate) commands: &'static [&'static str],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExpectedBucketMetadata {
    pub(crate) scope_label: &'static str,
    pub(crate) owner: &'static str,
    pub(crate) expensive: bool,
    pub(crate) notes: Option<&'static str>,
}

pub(crate) fn expected_buckets() -> BTreeMap<&'static str, ExpectedBucket> {
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
                    "cargo nextest run --lib tests::cli::cli_search_no_target_test",
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
            "core-handler-telemetry",
            ExpectedBucket {
                expected_seconds: 20,
                timeout_seconds: 90,
                commands: &[
                    "cargo nextest run --lib tests::core::handler_telemetry -- --skip search_quality",
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
                // Bumped to 60s/180s after the 2026-05 daemon-split bucket additions
                // (lock_test, discovery_test, token_file_test, app_test, shutdown_drain_test).
                expected_seconds: 60,
                timeout_seconds: 180,
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
                commands: &[
                    "cargo nextest run --lib tests::integration -- --skip search_quality --skip documentation_indexing",
                ],
            },
        ),
        (
            "documentation-indexing",
            ExpectedBucket {
                expected_seconds: 25,
                timeout_seconds: 90,
                commands: &[
                    "cargo nextest run --lib tests::integration::documentation_indexing -- --skip search_quality",
                ],
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
                expected_seconds: 40,
                timeout_seconds: 120,
                commands: &[
                    "cargo nextest run --lib tests::tools::hybrid_search_tests -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::search::backend_param_tests -- --skip search_quality",
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
                expected_seconds: 8,
                timeout_seconds: 45,
                commands: &[
                    "cargo nextest run --lib search::query_parse::tests -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::search::reranker_tests -- --skip search_quality",
                    "cargo nextest run --lib tools::search::query_preprocessor::tests -- --skip search_quality",
                ],
            },
        ),
        (
            "tools-search-unified",
            ExpectedBucket {
                expected_seconds: 25,
                timeout_seconds: 90,
                commands: &[
                    "cargo nextest run --lib tests::tools::search::c3_enriched_schema_tests -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::search::compat_marker_v4_test -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::search::fast_search_unified_cutover_test -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::search::nl_path_prior_pipeline_tests -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::search::nl_symbol_query_latency_tests -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::search::pretokenized_emit_test -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::search::projection_search_doc_test -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::search::relationship_text_test -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::search::reranker_ordering_tests -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::search::schema_phase2_fields_test -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::search::title_exact_boost_tests -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::search::tokenizer_simple_test -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::search::unified_ -- --skip search_quality",
                ],
            },
        ),
        (
            "tools-search-promotion",
            ExpectedBucket {
                expected_seconds: 15,
                timeout_seconds: 60,
                commands: &[
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

pub(crate) fn expected_bucket_metadata() -> BTreeMap<&'static str, ExpectedBucketMetadata> {
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
            "core-handler-telemetry",
            ExpectedBucketMetadata {
                scope_label: "core",
                owner: "lead",
                expensive: false,
                notes: Some("handler telemetry metadata contracts"),
            },
        ),
        (
            "extractors",
            ExpectedBucketMetadata {
                scope_label: "extractors",
                owner: "lead",
                expensive: false,
                notes: Some(
                    "extractor golden, capability matrix, and Pillar-3 downstream-consumer gate",
                ),
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
            "documentation-indexing",
            ExpectedBucketMetadata {
                scope_label: "system",
                owner: "lead",
                expensive: false,
                notes: Some("documentation indexing integration coverage"),
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
                notes: Some("search query parsing and preprocessing"),
            },
        ),
        (
            "tools-search-unified",
            ExpectedBucketMetadata {
                scope_label: "tooling",
                owner: "lead",
                expensive: false,
                notes: Some("unified search schema, ranking, reranker, and query-path coverage"),
            },
        ),
        (
            "tools-search-promotion",
            ExpectedBucketMetadata {
                scope_label: "tooling",
                owner: "lead",
                expensive: false,
                notes: Some("promotion ranking coverage for the unified search path"),
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
