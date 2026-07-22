const DEV_FALLBACK_FILES: &[&str] = &[
    "Cargo.toml",
    "Cargo.lock",
    "src/registry/http_transport.rs",
    "src/registry/transport.rs",
    "src/registry/workspace_pool.rs",
    "src/handler.rs",
    "src/lib.rs",
    "src/main.rs",
    "src/tests/mod.rs",
    "src/tests/test_utils.rs",
];

const DEV_FALLBACK_PREFIXES: &[&str] = &[
    "fixtures/",
    "src/adapter/",
    "src/tests/registry/http_transport/",
    "src/tests/fixtures/",
    "src/tests/helpers/",
];

const SEARCH_TOOL_BUCKETS: &[&str] = &[
    "tools-search-tantivy",
    "tools-search-line-core",
    "tools-search-line-filters",
    "tools-search-line-primary",
    "tools-search-file-mode",
    "tools-search-zero-hit",
    "tools-search-promotion",
    "tools-search-format-quality",
    "tools-search-context",
    "tools-search-text",
    "tools-search-hybrid",
    "tools-search-query",
    "tools-search-unified",
];

const SEARCH_TOOL_BUCKETS_WITH_QUALITY: &[&str] = &[
    "tools-search-tantivy",
    "tools-search-line-core",
    "tools-search-line-filters",
    "tools-search-line-primary",
    "tools-search-file-mode",
    "tools-search-zero-hit",
    "tools-search-promotion",
    "tools-search-format-quality",
    "tools-search-context",
    "tools-search-text",
    "tools-search-hybrid",
    "tools-search-query",
    "tools-search-unified",
    "search-quality",
];

const SEARCH_TOOL_BUCKETS_WITH_HANDLER_TELEMETRY: &[&str] = &[
    "tools-search-tantivy",
    "tools-search-line-core",
    "tools-search-line-filters",
    "tools-search-line-primary",
    "tools-search-file-mode",
    "tools-search-zero-hit",
    "tools-search-promotion",
    "tools-search-format-quality",
    "tools-search-context",
    "tools-search-text",
    "tools-search-hybrid",
    "tools-search-query",
    "tools-search-unified",
    "core-handler-telemetry",
];

/// Buckets for `crates/julie-index/src/search/**` edits (Phase 1 crate split).
/// Editing search code in julie-index compiles and runs the crate's own binary
/// (`core-index`) PLUS all top-crate search buckets whose retained tests still
/// exercise the moved search code — mirroring the `src/search/` pre-split routing
/// (which used SEARCH_TOOL_BUCKETS_WITH_QUALITY). Keeping search-quality in this
/// list ensures high-impact search changes are not silently under-tested.
const JULIE_INDEX_SEARCH_BUCKETS: &[&str] = &[
    "core-index",
    "tools-search-tantivy",
    "tools-search-line-core",
    "tools-search-line-filters",
    "tools-search-line-primary",
    "tools-search-file-mode",
    "tools-search-zero-hit",
    "tools-search-promotion",
    "tools-search-format-quality",
    "tools-search-context",
    "tools-search-text",
    "tools-search-hybrid",
    "tools-search-query",
    "tools-search-unified",
    "search-quality",
];

/// Buckets for `crates/julie-pipeline/src/indexing_core/**`, `src/resolver*`, and
/// `src/finalize.rs` edits (Phase 2 PR 2a crate split). Editing the indexing /
/// relationship-resolution engine compiles + runs the crate's own binary
/// (`core-pipeline`, which holds the ~142 relocated pipeline unit tests + the
/// dep-direction tripwire) PLUS the top-crate behavioral buckets whose RETAINED
/// tests still drive the moved indexing pipeline end-to-end. Without these
/// co-targets a localized edit to moved indexing code would silently skip its
/// behavioral regression coverage (Phase 0/1 lesson, R6).
const JULIE_PIPELINE_INDEXING_BUCKETS: &[&str] = &[
    "core-pipeline",
    "workspace-init",
    "integration",
    "tools-workspace-discovery",
    "tools-workspace-indexing",
    "tools-workspace-management",
];

/// Buckets for `crates/julie-pipeline/src/embeddings/**` edits. The embedding
/// stack's behavioral tests (`tests::core::embedding_provider`,
/// `embedding_sidecar_provider`, `sidecar_embedding_tests`) stayed in the top
/// crate and exercise the moved embedding code via the shim re-exports, so an
/// embeddings edit co-targets `core-embeddings` alongside `core-pipeline` (R6).
const JULIE_PIPELINE_EMBEDDINGS_BUCKETS: &[&str] = &["core-pipeline", "core-embeddings"];

/// Buckets for `crates/julie-runtime/src/watcher/**` edits (Phase 2c crate split).
/// Editing watcher source compiles + runs the crate's own test binary via
/// `core-runtime`. The daemon watcher-pool tests (`workspace-runtime` bucket)
/// are behavioral tests that exercise the watcher lifecycle from above (R6).
const JULIE_RUNTIME_WATCHER_BUCKETS: &[&str] = &["core-runtime", "workspace-runtime"];

/// Buckets for `crates/julie-runtime/src/workspace/**` edits (Phase 2c crate split).
/// Editing workspace source compiles + runs the crate's own test binary via
/// `core-runtime`. Co-targets cover the top-crate workspace-runtime (daemon pool
/// tests) and workspace-init (handler binding tests) slices (R6).
const JULIE_RUNTIME_WORKSPACE_BUCKETS: &[&str] =
    &["core-runtime", "workspace-runtime", "workspace-init"];

const WORKSPACE_TOOL_BUCKETS: &[&str] = &[
    "tools-workspace-discovery",
    "tools-workspace-indexing",
    "tools-workspace-management",
];

/// Buckets for general `crates/julie-tools/src/**` edits (Phase 2b crate split).
/// Covers the tool-specific test buckets whose commands now include `-p julie-tools`
/// entries. A catch-all for tool source not covered by the subpath arms below.
const JULIE_TOOLS_BUCKETS: &[&str] = &[
    "tools-format-filter",
    "tools-search-query",
    "tools-search-hybrid",
    "tools-blast-spillover",
];

/// Buckets for `crates/julie-tools/src/search/**` edits. Covers all search-facing
/// test buckets whose commands target either the top-crate test binary or the
/// julie-tools binary directly (R6: a localized edit must not skip its coverage).
const JULIE_TOOLS_SEARCH_BUCKETS: &[&str] = &[
    "tools-search-query",
    "tools-search-hybrid",
    "tools-search-text",
    "tools-search-context",
    "tools-search-unified",
    "tools-search-format-quality",
    "tools-search-promotion",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FallbackRule {
    ExactFile,
    Prefix,
    ManifestLevel,
    Unknown,
}

pub(super) fn fallback_rule_for_path(path: &str) -> Option<(FallbackRule, String)> {
    if let Some(exact_file) = DEV_FALLBACK_FILES
        .iter()
        .copied()
        .find(|candidate| path == *candidate)
    {
        return Some((FallbackRule::ExactFile, exact_file.to_string()));
    }

    if let Some(prefix) = DEV_FALLBACK_PREFIXES
        .iter()
        .copied()
        .find(|prefix| path.starts_with(prefix))
    {
        return Some((FallbackRule::Prefix, prefix.to_string()));
    }

    None
}

mod crates;
mod front;
mod product;

pub(super) fn buckets_for_path(path: &str) -> &'static [&'static str] {
    for select in [
        front::buckets_for_path,
        crates::buckets_for_path,
        product::buckets_for_path,
    ] {
        let buckets = select(path);
        if !buckets.is_empty() {
            return buckets;
        }
    }

    &[]
}

pub(super) fn get_context_test_buckets_for_path(path: &str) -> Option<&'static [&'static str]> {
    // Top-crate paths (primary_rebind + target_workspace_metrics STAY; the rest relocated to julie-tools T2b.6).
    // Julie-tools paths (T2b.6 relocated tests) share the same bucket assignments.
    if matches_exact(
        path,
        &[
            "src/tests/tools/get_context_pipeline_tests.rs",
            "src/tests/tools/get_context_pipeline_relevance_tests.rs",
            "src/tests/tools/get_context_relevance_tests.rs",
            "src/tests/tools/get_context_scoring_tests.rs",
            "src/tests/tools/get_context_quality_tests.rs",
            "crates/julie-tools/src/tests/get_context_pipeline_tests.rs",
            "crates/julie-tools/src/tests/get_context_pipeline_relevance_tests.rs",
            "crates/julie-tools/src/tests/get_context_relevance_tests.rs",
            "crates/julie-tools/src/tests/get_context_scoring_tests.rs",
            "crates/julie-tools/src/tests/get_context_quality_tests.rs",
        ],
    ) {
        return Some(&["tools-get-context-pipeline"]);
    }

    if matches_exact(
        path,
        &[
            "src/tests/tools/get_context_allocation_tests.rs",
            "src/tests/tools/get_context_formatting_tests.rs",
            "src/tests/tools/get_context_token_budget_tests.rs",
            "src/tests/tools/get_context_tests.rs",
            "crates/julie-tools/src/tests/get_context_allocation_tests.rs",
            "crates/julie-tools/src/tests/get_context_formatting_tests.rs",
            "crates/julie-tools/src/tests/get_context_token_budget_tests.rs",
            "crates/julie-tools/src/tests/get_context_tests.rs",
        ],
    ) {
        return Some(&["tools-get-context-format"]);
    }

    if matches_exact(
        path,
        &[
            "src/tests/tools/get_context_graph_expansion_tests.rs",
            "src/tests/tools/get_context_task_inputs_tests.rs",
            "src/tests/tools/get_context_primary_rebind_tests.rs",
            "src/tests/tools/get_context_target_workspace_metrics_tests.rs",
            "crates/julie-tools/src/tests/get_context_graph_expansion_tests.rs",
            "crates/julie-tools/src/tests/get_context_task_inputs_tests.rs",
        ],
    ) {
        return Some(&["tools-get-context-graph"]);
    }

    // Any other get_context_*.rs in either location runs all three slices.
    if path.starts_with("src/tests/tools/get_context")
        || path.starts_with("crates/julie-tools/src/tests/get_context_")
    {
        return Some(&[
            "tools-get-context-pipeline",
            "tools-get-context-format",
            "tools-get-context-graph",
        ]);
    }

    None
}

pub(super) fn matches_exact(path: &str, candidates: &[&str]) -> bool {
    candidates.iter().any(|candidate| path == *candidate)
}

pub(super) fn matches_prefix(path: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|prefix| path.starts_with(prefix))
}
