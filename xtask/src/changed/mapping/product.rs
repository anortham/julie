use super::*;

pub(super) fn buckets_for_path(path: &str) -> &'static [&'static str] {
    if path == "src/tests/core/handler_telemetry.rs" {
        return &["core-handler-telemetry"];
    }

    // Per-test file routing for get_context split.
    if let Some(buckets) = get_context_test_buckets_for_path(path) {
        return buckets;
    }

    // src/tools/get_context/ source edits run all three slices conservatively.
    if matches_prefix(path, &["src/tools/get_context/"]) {
        return &[
            "tools-get-context-pipeline",
            "tools-get-context-format",
            "tools-get-context-graph",
        ];
    }

    if matches_exact(
        path,
        &[
            "src/search/projection.rs",
            "src/database/projections.rs",
            "src/health/projection.rs",
            "src/health/evaluation.rs",
            "src/tools/workspace/indexing/index.rs",
            "src/tools/workspace/indexing/pipeline.rs",
            "src/tests/integration/projection_repair.rs",
        ],
    ) {
        return &["projection"];
    }

    if matches_exact(path, &["src/registry/mod.rs", "src/registry/lifecycle.rs"]) {
        return &["registry"];
    }

    if matches_exact(
        path,
        &[
            "src/registry/workspace_registry_store.rs",
            "src/registry/workspace_session_attachment.rs",
            "src/registry/workspace_cleanup.rs",
            "src/workspace/registry.rs",
            "src/tests/registry/workspace_cleanup.rs",
            "src/tests/tools/workspace/registry.rs",
        ],
    ) || matches_prefix(path, &["src/tools/workspace/commands/registry/"])
    {
        return &["workspace-runtime"];
    }

    if matches_exact(
        path,
        &[
            "src/tools/workspace/discovery.rs",
            "src/tools/workspace/language.rs",
            "src/tools/workspace/paths.rs",
            "src/tools/workspace/utils.rs",
        ],
    ) {
        return &["tools-workspace-discovery"];
    }

    if matches_prefix(path, &["src/tools/workspace/indexing/"]) {
        return &["tools-workspace-indexing", "workspace-init"];
    }

    // Note: "src/workspace/" removed — workspace moved to crates/julie-runtime (T2c.2);
    // edits to the top-crate re-export shim at src/workspace/ now route via the
    // crates/julie-runtime/src/workspace/ arm above when editing the real source.
    // Broad workspace command/module edits keep all workspace tool slices.
    if matches_prefix(path, &["src/tools/workspace/"]) {
        return &[
            "tools-workspace-discovery",
            "tools-workspace-indexing",
            "tools-workspace-management",
            "tools-workspace-targeting",
            "workspace-init",
        ];
    }

    // Heavy targeting fixtures are isolated in tools-workspace-targeting.
    if matches_exact(
        path,
        &[
            "src/tests/tools/workspace/global_targeting.rs",
            "src/tests/tools/workspace/refresh_routing.rs",
        ],
    ) || matches_prefix(path, &["src/tests/tools/workspace/global_targeting/"])
    {
        return &["tools-workspace-targeting"];
    }

    if matches_exact(
        path,
        &[
            "src/tests/tools/workspace/discovery.rs",
            "src/tests/tools/workspace/utils.rs",
        ],
    ) || matches_prefix(path, &["src/tests/tools/workspace/discovery/"])
    {
        return &["tools-workspace-discovery"];
    }

    if matches_exact(
        path,
        &[
            "src/tests/tools/workspace/file_policy.rs",
            "src/tests/tools/workspace/index_embedding_tests.rs",
            "src/tests/tools/workspace/mod_tests.rs",
            "src/tests/tools/workspace/processor.rs",
            "src/tests/tools/workspace/resolver.rs",
        ],
    ) || matches_prefix(path, &["src/tests/tools/workspace/mod_tests/"])
    {
        return &["tools-workspace-indexing"];
    }

    if matches_exact(
        path,
        &[
            "src/tests/tools/workspace/isolation.rs",
            "src/tests/tools/workspace/management_token.rs",
        ],
    ) {
        return &["tools-workspace-management"];
    }

    if matches_prefix(path, &["src/tests/tools/workspace/"]) {
        return WORKSPACE_TOOL_BUCKETS;
    }

    if path == "src/tools/search/query_preprocessor.rs" {
        return &["tools-search-query"];
    }

    if matches_prefix(path, &["src/tools/search/"]) {
        return SEARCH_TOOL_BUCKETS;
    }

    if let Some(search_test_buckets) = search_test_buckets_for_path(path) {
        return search_test_buckets;
    }

    if matches_prefix(path, &["src/search/"])
        || matches_prefix(path, &["src/tests/tools/search_quality/"])
    {
        return SEARCH_TOOL_BUCKETS_WITH_QUALITY;
    }

    if matches_exact(
        path,
        &["src/tests/tools/get_symbols_target_filtering_dogfood.rs"],
    ) {
        return &["tools-dogfood-repo-index"];
    }

    if matches_prefix(path, &["src/tools/symbols/"])
        || path.starts_with("src/tests/tools/get_symbols")
    {
        return &["tools-get-symbols"];
    }

    if matches_prefix(path, &["src/tools/editing/", "src/tests/tools/editing/"]) {
        return &["tools-editing"];
    }

    // src/tools/deep_dive/ and deep_dive test files
    if matches_prefix(
        path,
        &["src/tools/deep_dive/", "src/tests/tools/deep_dive_tests/"],
    ) || matches_exact(
        path,
        &[
            "src/tests/tools/deep_dive_tests.rs",
            "src/tests/tools/deep_dive_primary_rebind_tests.rs",
            "src/tests/tools/deep_dive_regression_tests.rs",
        ],
    ) {
        return &["tools-deep-dive"];
    }

    // call_path tool source + tests
    if path == "src/tools/navigation/call_path.rs"
        || matches_exact(
            path,
            &[
                "src/tests/tools/call_path_tests.rs",
                "src/tests/tools/call_path_disambiguation_tests.rs",
            ],
        )
    {
        return &["tools-call-path"];
    }

    // fast_refs tool source + tests (target_workspace.rs is the cross-workspace
    // binding for refs; group with fast-refs).
    if matches_exact(
        path,
        &[
            "src/tools/navigation/fast_refs.rs",
            "src/tools/navigation/target_workspace.rs",
            "src/tests/tools/fast_refs_primary_rebind_tests.rs",
            "src/tests/tools/target_workspace_fast_refs_tests.rs",
        ],
    ) || matches_prefix(
        path,
        &["src/tests/tools/target_workspace_fast_refs_tests/tests/"],
    ) {
        return &["tools-fast-refs"];
    }

    // blast_radius (impact) and spillover share graph traversal infrastructure
    if matches_prefix(path, &["src/tools/impact/", "src/tools/spillover/"])
        || path == "src/tests/tools/spillover_tests.rs"
        || path.starts_with("src/tests/tools/blast_radius")
    {
        return &["tools-blast-spillover"];
    }

    // src/tools/navigation/{mod,formatting,resolution}.rs are shared across all
    // navigation buckets. An edit there is rare and we conservatively run all four.
    if matches_prefix(path, &["src/tools/navigation/"]) {
        return &[
            "tools-deep-dive",
            "tools-call-path",
            "tools-fast-refs",
            "tools-blast-spillover",
        ];
    }

    if matches_prefix(
        path,
        &["src/tools/refactoring/", "src/tests/tools/refactoring/"],
    ) {
        return &["tools-refactoring"];
    }

    if matches_prefix(path, &["src/tools/metrics/", "src/tests/tools/metrics/"]) {
        return &["tools-metrics"];
    }

    // Note: filtering_tests, formatting_tests, query_classification_tests, phase4_token_savings
    // were all relocated to crates/julie-tools/src/tests/ (T2b.6 or earlier). Routing for their
    // julie-tools paths is handled by the crates/julie-tools/src/tests/ prefix checks above.

    // Note: "src/watcher/" removed — watcher moved to crates/julie-runtime (T2c.2);
    // "src/tests/integration/watcher_filtering.rs" removed — tests moved to
    // crates/julie-runtime/src/tests/ (T2c.3). Both now route via the
    // crates/julie-runtime/src/ arms above.
    if matches_prefix(
        path,
        &["src/utils/", "src/tracing/", "src/tests/core/handler/"],
    ) || matches_exact(
        path,
        &[
            "src/language.rs",
            "src/paths.rs",
            "src/tests/core/handler.rs",
            "src/tests/core/language.rs",
            "src/tests/core/paths.rs",
            "src/tests/core/tracing.rs",
        ],
    ) || matches_prefix(path, &["src/tests/utils/"])
    {
        return &["core-fast"];
    }

    if matches_prefix(path, &["src/registry/", "src/tests/registry/"]) {
        return &["registry"];
    }

    if matches_prefix(path, &["src/dashboard/", "src/tests/dashboard/"]) {
        return &["dashboard"];
    }

    if matches_prefix(path, &["src/health/"])
        || matches_exact(path, &["src/tests/integration/system_health.rs"])
    {
        return &["system-health"];
    }

    if matches_exact(path, &["src/tests/core/workspace_init.rs"])
        || matches_prefix(path, &["src/tests/core/workspace_init/"])
    {
        return &["workspace-init"];
    }

    if matches_prefix(path, &["src/tests/integration/"]) {
        return &["integration"];
    }

    &[]
}

fn search_test_buckets_for_path(path: &str) -> Option<&'static [&'static str]> {
    if matches_exact(path, &["src/tests/tools/search/mod.rs"]) {
        return Some(SEARCH_TOOL_BUCKETS);
    }

    if matches_prefix(path, &["src/tests/tools/search/tantivy_"]) {
        return Some(&["tools-search-tantivy"]);
    }

    if path == "src/tests/tools/search/line_mode.rs"
        || path == "src/tests/tools/search/line_mode/mod.rs"
        || path == "src/tests/tools/search/line_mode/basic.rs"
    {
        return Some(&["tools-search-line-core"]);
    }

    if path == "src/tests/tools/search/line_mode/filters.rs"
        || path == "src/tests/tools/search/line_mode_or_fallback_tests.rs"
        || path == "src/tests/tools/search/line_mode_second_pass_tests.rs"
    {
        return Some(&["tools-search-line-filters"]);
    }

    if path == "src/tests/tools/search/line_mode/missing_index.rs"
        || path == "src/tests/tools/search/line_mode/primary_rebind.rs"
    {
        return Some(&["tools-search-line-primary"]);
    }

    if matches_prefix(path, &["src/tests/tools/search/file_"]) {
        return Some(&["tools-search-file-mode"]);
    }

    if matches_exact(
        path,
        &[
            "src/tests/tools/search/primary_workspace_bug.rs",
            "src/tests/tools/search/zero_hit_reason_tests.rs",
            "src/tests/tools/search/zero_hit_reason_propagation_tests.rs",
        ],
    ) {
        return Some(&["tools-search-zero-hit"]);
    }

    if matches_prefix(path, &["src/tests/tools/search/definition_"])
        || path == "src/tests/tools/search/promotion_tests.rs"
    {
        return Some(&["tools-search-promotion"]);
    }

    if matches_exact(
        path,
        &[
            "src/tests/tools/search/annotation_search_tests.rs",
            "src/tests/tools/search/content_scoring_tests.rs",
            "src/tests/tools/search/fast_search_regression_tests.rs",
            "src/tests/tools/search/lean_format_tests.rs",
            "src/tests/tools/search/quality.rs",
            "src/tests/tools/search/race_condition.rs",
        ],
    ) {
        return Some(&["tools-search-format-quality"]);
    }

    if matches_exact(
        path,
        &[
            "src/tests/tools/search/c3_enriched_schema_tests.rs",
            "src/tests/tools/search/compat_marker_v4_test.rs",
            "src/tests/tools/search/fast_search_unified_cutover_test.rs",
            "src/tests/tools/search/nl_path_prior_pipeline_tests.rs",
            "src/tests/tools/search/nl_symbol_query_latency_tests.rs",
            "src/tests/tools/search/pretokenized_emit_test.rs",
            "src/tests/tools/search/projection_search_doc_test.rs",
            "src/tests/tools/search/relationship_text_test.rs",
            "src/tests/tools/search/reranker_ordering_tests.rs",
            "src/tests/tools/search/schema_phase2_fields_test.rs",
            "src/tests/tools/search/title_exact_boost_tests.rs",
            "src/tests/tools/search/tokenizer_simple_test.rs",
        ],
    ) || matches_prefix(path, &["src/tests/tools/search/unified_"])
    {
        return Some(&["tools-search-unified"]);
    }

    if matches_exact(path, &["src/tests/tools/search_context_lines.rs"]) {
        return Some(&["tools-search-context"]);
    }

    if matches_exact(
        path,
        &[
            "src/tests/tools/text_search_tantivy.rs",
            "src/tests/tools/search/structural_facts_text_test.rs",
        ],
    ) {
        return Some(&["tools-search-text"]);
    }

    if matches_exact(path, &["src/tests/tools/hybrid_search_tests.rs"])
        || matches_prefix(path, &["src/tests/tools/hybrid_search_tests/"])
    {
        return Some(&["tools-search-hybrid"]);
    }

    if matches_prefix(path, &["src/tests/tools/search/"]) {
        return Some(SEARCH_TOOL_BUCKETS);
    }

    None
}
