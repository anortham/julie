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
            "xtask-eval",
            ExpectedBucket {
                expected_seconds: 30,
                timeout_seconds: 120,
                commands: &["cargo nextest run -p xtask-eval"],
            },
        ),
        (
            "core-database",
            ExpectedBucket {
                expected_seconds: 5,
                timeout_seconds: 90,
                commands: &["cargo nextest run -p julie-core"],
            },
        ),
        (
            "core-embeddings",
            ExpectedBucket {
                expected_seconds: 15,
                timeout_seconds: 60,
                commands: &[
                    "cargo nextest run --lib tests::core::embedding_provider -- --skip search_quality",
                    "cargo nextest run --lib tests::core::embedding_sidecar_provider -- --skip search_quality",
                    "cargo nextest run --lib tests::core::sidecar_embedding_tests -- --skip search_quality",
                ],
            },
        ),
        (
            "core-index",
            ExpectedBucket {
                expected_seconds: 10,
                timeout_seconds: 60,
                commands: &["cargo nextest run -p julie-index"],
            },
        ),
        (
            "core-pipeline",
            ExpectedBucket {
                expected_seconds: 10,
                timeout_seconds: 60,
                commands: &["cargo nextest run -p julie-pipeline"],
            },
        ),
        (
            "core-runtime",
            ExpectedBucket {
                expected_seconds: 10,
                timeout_seconds: 60,
                commands: &["cargo nextest run -p julie-runtime"],
            },
        ),
        (
            "core-fast",
            ExpectedBucket {
                expected_seconds: 26,
                timeout_seconds: 120,
                commands: &[
                    "cargo nextest run --lib utils::paths::tests -- --skip search_quality",
                    "cargo nextest run -p julie-runtime --lib tests::watcher_filtering",
                    "cargo nextest run --lib tests::core::handler -- --skip search_quality",
                    "cargo nextest run --lib tests::core::language -- --skip search_quality",
                    "cargo nextest run --lib tests::core::paths -- --skip search_quality",
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
            "extractor-dep-integration",
            ExpectedBucket {
                expected_seconds: 60,
                timeout_seconds: 180,
                commands: &[
                    "cargo nextest run --lib test_semantic_index_engine_version_includes_extraction_contract",
                    "cargo nextest run --lib real_world_parser_upgrade_contracts_assert_expected_outputs",
                    "cargo nextest run --lib current_parser_release_contracts_parse_without_diagnostics",
                ],
            },
        ),
        (
            "registry",
            ExpectedBucket {
                // Bumped to 60s/180s after the 2026-05 daemon-split bucket additions.
                // Discovery-file tests were deleted in Phase 3d.3; token_file_test,
                // app_test, and shutdown_drain_test were deleted in Phase 3d.2b.
                expected_seconds: 60,
                timeout_seconds: 180,
                commands: &[
                    "cargo nextest run --lib tests::registry -- --skip search_quality",
                    "cargo nextest run --lib tests::external_extract -- --skip search_quality",
                ],
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
            "system-health",
            ExpectedBucket {
                expected_seconds: 30,
                timeout_seconds: 120,
                commands: &["cargo nextest run --lib tests::integration::system_health"],
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
                    "cargo nextest run -p julie-tools --lib tests::get_context_pipeline_tests",
                    "cargo nextest run -p julie-tools --lib tests::get_context_pipeline_relevance_tests",
                    "cargo nextest run -p julie-tools --lib tests::get_context_relevance_tests",
                    "cargo nextest run -p julie-tools --lib tests::get_context_scoring_tests",
                    "cargo nextest run -p julie-tools --lib tests::get_context_quality_tests",
                ],
            },
        ),
        (
            "tools-get-context-format",
            ExpectedBucket {
                expected_seconds: 12,
                timeout_seconds: 45,
                commands: &[
                    "cargo nextest run -p julie-tools --lib tests::get_context_allocation_tests",
                    "cargo nextest run -p julie-tools --lib tests::get_context_formatting_tests",
                    "cargo nextest run -p julie-tools --lib tests::get_context_token_budget_tests",
                    "cargo nextest run -p julie-tools --lib tests::get_context_tests",
                ],
            },
        ),
        (
            "tools-get-context-graph",
            ExpectedBucket {
                expected_seconds: 25,
                timeout_seconds: 90,
                commands: &[
                    "cargo nextest run -p julie-tools --lib tests::get_context_graph_expansion_tests",
                    "cargo nextest run -p julie-tools --lib tests::get_context_task_inputs_tests",
                    "cargo nextest run --lib tests::tools::get_context_primary_rebind_tests -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::get_context_target_workspace_metrics_tests -- --skip search_quality",
                ],
            },
        ),
        (
            "tools-editing",
            ExpectedBucket {
                expected_seconds: 200,
                timeout_seconds: 300,
                commands: &[
                    "cargo nextest run --lib tests::tools::editing::edit_file_tests -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::editing::rewrite_symbol_cross_language_tests -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::editing::rewrite_symbol_tests -- --skip search_quality",
                    "cargo nextest run --lib commit_creates_temp_file_in_same_directory_for_relative_paths -- --skip search_quality",
                    "cargo nextest run -p julie-tools --lib tests::editing_markdown_section_tests",
                    "cargo nextest run -p julie-tools --lib tests::editing_security_tests",
                    "cargo nextest run -p julie-tools --lib tests::editing_transactional_editing_tests",
                    "cargo nextest run -p julie-tools --lib tests::editing_validation_tests",
                ],
            },
        ),
        (
            "tools-format-filter",
            ExpectedBucket {
                expected_seconds: 15,
                timeout_seconds: 60,
                commands: &[
                    "cargo nextest run -p julie-tools --lib tests::filtering_tests",
                    "cargo nextest run -p julie-tools --lib tests::formatting_tests",
                    "cargo nextest run -p julie-tools --lib tests::query_classification_tests",
                    "cargo nextest run -p julie-tools --lib tests::phase4_token_savings",
                ],
            },
        ),
        (
            "tools-get-symbols",
            ExpectedBucket {
                expected_seconds: 40,
                timeout_seconds: 300,
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
                    "cargo nextest run --lib tests::tools::metrics::session_metrics_tests tests::tools::metrics::query_tests -- --skip search_quality",
                    "cargo nextest run -p julie-tools --lib tests::metrics_file_size_query_tests",
                    "cargo nextest run -p julie-tools --lib tests::metrics_migration_tests",
                    "cargo nextest run -p julie-tools --lib tests::metrics_tool_calls_db_tests",
                ],
            },
        ),
        (
            "tools-deep-dive",
            ExpectedBucket {
                expected_seconds: 12,
                timeout_seconds: 60,
                commands: &[
                    "cargo nextest run -p julie-tools --lib tests::deep_dive_tests",
                    "cargo nextest run --lib tests::tools::deep_dive_primary_rebind_tests -- --skip search_quality",
                    "cargo nextest run -p julie-tools --lib tests::deep_dive_regression_tests",
                ],
            },
        ),
        (
            "tools-call-path",
            ExpectedBucket {
                expected_seconds: 80,
                timeout_seconds: 120,
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
                    "cargo nextest run -p julie-tools --lib tests::blast_radius_formatting_tests",
                    "cargo nextest run --lib tests::tools::spillover_tests -- --skip search_quality",
                ],
            },
        ),
        (
            "tools-refactoring",
            ExpectedBucket {
                expected_seconds: 45,
                timeout_seconds: 90,
                commands: &[
                    "cargo nextest run --lib tests::tools::refactoring::rename_symbol tests::tools::refactoring::smart_refactor -- --skip search_quality",
                    "cargo nextest run -p julie-tools --lib tests::refactoring_ast_aware",
                    "cargo nextest run -p julie-tools --lib tests::refactoring_compute_line_changes_tests",
                    "cargo nextest run -p julie-tools --lib tests::refactoring_import_update_tests",
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
                    "cargo nextest run -p julie-tools --lib tests::hybrid_search_tests",
                    "cargo nextest run --lib tests::tools::search::backend_param_tests -- --skip search_quality",
                ],
            },
        ),
        (
            "tools-search-line-core",
            ExpectedBucket {
                expected_seconds: 40,
                timeout_seconds: 90,
                commands: &[
                    "cargo nextest run -j 1 --lib tests::tools::search::line_mode::basic -- --skip search_quality",
                ],
            },
        ),
        (
            "tools-search-line-filters",
            ExpectedBucket {
                expected_seconds: 150,
                timeout_seconds: 240,
                commands: &[
                    "cargo nextest run -j 1 --lib tests::tools::search::line_mode::filters -- --skip search_quality",
                    "cargo nextest run -j 1 --lib tests::tools::search::line_mode_or_fallback_tests -- --skip search_quality",
                    "cargo nextest run -j 1 --lib tests::tools::search::line_mode_second_pass_tests -- --skip search_quality",
                    "cargo nextest run -j 1 --lib tests::tools::search::source_regions -- --skip search_quality",
                ],
            },
        ),
        (
            "tools-search-line-primary",
            ExpectedBucket {
                expected_seconds: 75,
                timeout_seconds: 120,
                commands: &[
                    "cargo nextest run -j 1 --lib tests::tools::search::line_mode::missing_index -- --skip search_quality",
                    "cargo nextest run -j 1 --lib tests::tools::search::line_mode::primary_rebind -- --skip search_quality",
                ],
            },
        ),
        (
            "tools-search-file-mode",
            ExpectedBucket {
                expected_seconds: 60,
                timeout_seconds: 120,
                commands: &[
                    "cargo nextest run -j 1 --lib tests::tools::search::file_ -- --skip search_quality",
                ],
            },
        ),
        (
            "tools-search-zero-hit",
            ExpectedBucket {
                expected_seconds: 40,
                timeout_seconds: 90,
                commands: &[
                    "cargo nextest run --lib tests::tools::search::primary_workspace_bug -- --skip search_quality",
                    "cargo nextest run -p julie-tools --lib tests::search_zero_hit_reason_tests",
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
                    "cargo nextest run -p julie-tools --lib search::query_preprocessor::tests",
                ],
            },
        ),
        (
            "tools-search-unified",
            ExpectedBucket {
                expected_seconds: 60,
                timeout_seconds: 120,
                commands: &[
                    "cargo nextest run --lib tests::tools::search::fast_search_unified_cutover_test -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::search::nl_embeddings_daemon_tests -- --skip search_quality",
                    "cargo nextest run -p julie-tools --lib tests::search_nl_path_prior_pipeline_tests",
                    "cargo nextest run -p julie-tools --lib tests::search_nl_symbol_query_latency_tests",
                    "cargo nextest run -p julie-tools --lib tests::search_pretokenized_emit_test",
                    "cargo nextest run --lib tests::tools::search::relationship_text_test -- --skip search_quality",
                    "cargo nextest run -p julie-tools --lib tests::search_title_exact_boost_tests",
                    "cargo nextest run --lib tests::tools::search::unified_ -- --skip search_quality",
                ],
            },
        ),
        (
            "tools-search-promotion",
            ExpectedBucket {
                expected_seconds: 15,
                timeout_seconds: 60,
                commands: &["cargo nextest run -p julie-tools --lib tests::search_promotion_tests"],
            },
        ),
        (
            "tools-search-format-quality",
            ExpectedBucket {
                expected_seconds: 100,
                timeout_seconds: 180,
                commands: &[
                    "cargo nextest run -p julie-tools --lib tests::search_annotation_search_tests",
                    "cargo nextest run --lib tests::tools::search::content_scoring_tests -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::search::fast_search_regression_tests -- --skip search_quality",
                    "cargo nextest run -p julie-tools --lib tests::search_lean_format_tests",
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
                commands: &["cargo nextest run -p julie-tools --lib tests::tantivy_"],
            },
        ),
        (
            "tools-search-text",
            ExpectedBucket {
                expected_seconds: 40,
                timeout_seconds: 90,
                commands: &[
                    "cargo nextest run --lib tests::tools::text_search_tantivy -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::search::structural_facts_text_test -- --skip search_quality",
                ],
            },
        ),
        (
            "tools-workspace-discovery",
            ExpectedBucket {
                expected_seconds: 60,
                timeout_seconds: 120,
                commands: &[
                    "cargo nextest run --lib tests::tools::workspace::discovery -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::workspace::utils -- --skip search_quality",
                ],
            },
        ),
        (
            "tools-workspace-indexing",
            ExpectedBucket {
                expected_seconds: 200,
                timeout_seconds: 360,
                commands: &[
                    "cargo nextest run --lib tests::tools::workspace::file_policy -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::workspace::index_embedding_tests -- --skip search_quality",
                    "cargo nextest run -j 1 --lib tests::tools::workspace::mod_tests -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::workspace::processor -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::workspace::resolver -- --skip search_quality",
                ],
            },
        ),
        (
            "tools-workspace-management",
            ExpectedBucket {
                expected_seconds: 40,
                timeout_seconds: 120,
                commands: &[
                    "cargo nextest run --lib tests::tools::workspace::isolation -- --skip search_quality",
                    "cargo nextest run --lib tests::tools::workspace::management_token -- --skip search_quality",
                ],
            },
        ),
        (
            "tools-workspace-targeting",
            ExpectedBucket {
                expected_seconds: 170,
                timeout_seconds: 240,
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
                expected_seconds: 20,
                timeout_seconds: 60,
                commands: &["cargo nextest run -p julie-runtime --lib tests::workspace::registry"],
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
            "xtask-eval",
            ExpectedBucketMetadata {
                scope_label: "tooling",
                owner: "lead",
                expensive: false,
                notes: Some("product-linked search-matrix / eval harness"),
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
                notes: Some("embedding stack (top-crate survivors; bulk moved to core-pipeline)"),
            },
        ),
        (
            "core-index",
            ExpectedBucketMetadata {
                scope_label: "core",
                owner: "lead",
                expensive: false,
                notes: Some("julie-index crate: search + analysis layer above julie-core"),
            },
        ),
        (
            "core-pipeline",
            ExpectedBucketMetadata {
                scope_label: "core",
                owner: "lead",
                expensive: false,
                notes: Some("julie-pipeline crate: indexing + embedding engine above julie-index"),
            },
        ),
        (
            "core-runtime",
            ExpectedBucketMetadata {
                scope_label: "core",
                owner: "lead",
                expensive: false,
                notes: Some(
                    "julie-runtime crate: watcher + workspace lifecycle layer above julie-pipeline",
                ),
            },
        ),
        (
            "core-fast",
            ExpectedBucketMetadata {
                scope_label: "core",
                owner: "lead",
                expensive: false,
                notes: Some("misc fast core coverage (Task 6 warm p95≈23s → expected=26)"),
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
            "extractor-dep-integration",
            ExpectedBucketMetadata {
                scope_label: "extractor-dep",
                owner: "lead",
                expensive: false,
                notes: Some(
                    "extractor dependency integration: contract anchor + output and current-syntax parse contracts",
                ),
            },
        ),
        (
            "registry",
            ExpectedBucketMetadata {
                scope_label: "registry",
                owner: "lead",
                expensive: false,
                notes: Some("registry-mode protocol coverage"),
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
            "system-health",
            ExpectedBucketMetadata {
                scope_label: "system",
                owner: "lead",
                expensive: false,
                notes: Some("system health latency / health-report gate"),
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
                notes: Some(
                    "editing tools split by submodule; broad filter exceeds Windows bucket timeout",
                ),
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
            "tools-search-line-core",
            ExpectedBucketMetadata {
                scope_label: "tooling",
                owner: "lead",
                expensive: false,
                notes: Some(
                    "line-mode basic behavior coverage; serialized because tests mutate process env and spin per-test indexers",
                ),
            },
        ),
        (
            "tools-search-line-filters",
            ExpectedBucketMetadata {
                scope_label: "tooling",
                owner: "lead",
                expensive: false,
                notes: Some("line-mode filter, fallback, and second-pass coverage"),
            },
        ),
        (
            "tools-search-line-primary",
            ExpectedBucketMetadata {
                scope_label: "tooling",
                owner: "lead",
                expensive: false,
                notes: Some("line-mode primary workspace rebinding and missing-index coverage"),
            },
        ),
        (
            "tools-search-file-mode",
            ExpectedBucketMetadata {
                scope_label: "tooling",
                owner: "lead",
                expensive: false,
                notes: Some(
                    "file-mode search coverage; serialized on Windows because these tests spin per-test Tantivy indexers",
                ),
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
            "tools-workspace-discovery",
            ExpectedBucketMetadata {
                scope_label: "tooling",
                owner: "lead",
                expensive: false,
                notes: Some("workspace discovery, path, and utility coverage"),
            },
        ),
        (
            "tools-workspace-indexing",
            ExpectedBucketMetadata {
                scope_label: "tooling",
                owner: "lead",
                expensive: false,
                notes: Some(
                    "workspace indexing policy, processor, resolver, and serialized mod_tests coverage",
                ),
            },
        ),
        (
            "tools-workspace-management",
            ExpectedBucketMetadata {
                scope_label: "tooling",
                owner: "lead",
                expensive: false,
                notes: Some("workspace isolation and management output coverage"),
            },
        ),
        (
            "tools-workspace-targeting",
            ExpectedBucketMetadata {
                scope_label: "tooling",
                owner: "lead",
                expensive: false,
                notes: Some(
                    "workspace global-targeting and refresh-routing coverage; Windows runs include long activation cases",
                ),
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
                    "workspace registry (pool/watcher + workspace_cleanup deleted in 3d.2b; cleanup -> 3d.3)",
                ),
            },
        ),
    ])
}
