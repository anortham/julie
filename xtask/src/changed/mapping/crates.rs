use super::*;

pub(super) fn buckets_for_path(path: &str) -> &'static [&'static str] {
    if path == "crates/julie-core/src/connection_pool.rs" {
        return &["core-database", "registry"];
    }
    if path == "crates/julie-core/src/embeddings_contract.rs" {
        return &["core-database", "core-embeddings"];
    }
    if path == "crates/julie-core/src/paths.rs" {
        return &["core-database", "core-fast"];
    }
    if matches_prefix(path, &["crates/julie-core/src/"]) {
        // database/**, lib.rs, test_support/**, tests/** — covered by `-p julie-core`.
        return &["core-database"];
    }

    // julie-index leaf crate (Phase 1 crate split). Editing any leaf source compiles +
    // runs the crate's own test binary via `core-index` (`cargo nextest run -p
    // julie-index`, which holds the tripwire + any tests relocated there by T3). The
    // two subpaths whose behavioral tests still live in top-crate buckets ALSO pull
    // those buckets (Phase 0 lesson: a localized edit to moved code must not silently
    // skip its behavioral coverage):
    //   crates/julie-index/src/search/**   -> core-index + all search tool buckets + search-quality
    //   crates/julie-index/src/analysis/** -> core-index (analysis bucket removed by T6; tests relocated to julie-index)
    // Subpath checks must precede the catch-all prefix (first match wins).
    if matches_prefix(path, &["crates/julie-index/src/search/"]) {
        return JULIE_INDEX_SEARCH_BUCKETS;
    }
    if matches_prefix(path, &["crates/julie-index/src/analysis/"]) {
        return &["core-index"];
    }
    if matches_prefix(path, &["crates/julie-index/src/"]) {
        // lib.rs, tests/**, other top-level files — covered by `-p julie-index`.
        return &["core-index"];
    }

    // julie-pipeline crate (Phase 2 PR 2a crate split). Editing any pipeline source
    // compiles + runs the crate's own test binary via `core-pipeline` (`cargo
    // nextest run -p julie-pipeline`, which holds the ~142 relocated tests + the
    // dep-direction tripwire). The two engine subpaths whose behavioral tests still
    // live in top-crate buckets ALSO pull those buckets (Phase 0/1 lesson — a
    // localized edit to moved code must not silently skip its behavioral coverage):
    //   crates/julie-pipeline/src/embeddings/**    -> core-pipeline + core-embeddings
    //   crates/julie-pipeline/src/indexing_core/** -> core-pipeline + workspace-init + integration + split workspace tool buckets
    //   crates/julie-pipeline/src/{resolver*,finalize.rs} -> same indexing behavioral set
    // Subpath checks must precede the catch-all prefix (first match wins).
    if matches_prefix(path, &["crates/julie-pipeline/src/embeddings/"]) {
        return JULIE_PIPELINE_EMBEDDINGS_BUCKETS;
    }
    if matches_prefix(path, &["crates/julie-pipeline/src/indexing_core/"])
        || matches_prefix(path, &["crates/julie-pipeline/src/resolver"])
        || path == "crates/julie-pipeline/src/finalize.rs"
    {
        return JULIE_PIPELINE_INDEXING_BUCKETS;
    }
    if matches_prefix(path, &["crates/julie-pipeline/src/"]) {
        // lib.rs, tests/**, other top-level files — covered by `-p julie-pipeline`.
        return &["core-pipeline"];
    }

    // julie-runtime crate (Phase 2c crate split): watcher + workspace lifecycle
    // layer above julie-pipeline. Editing any runtime source compiles + runs the
    // crate's own test binary via `core-runtime` (`cargo nextest run -p
    // julie-runtime`, which holds the ~80 relocated tests + the dep-direction
    // tripwire). The two subpaths whose behavioral tests still live in top-crate
    // buckets ALSO pull those buckets (Phase 0/1/2a lesson — a localized edit to
    // moved code must not silently skip its behavioral coverage):
    //   crates/julie-runtime/src/watcher/**    -> core-runtime + workspace-runtime
    //   crates/julie-runtime/src/workspace/**  -> core-runtime + workspace-runtime + workspace-init
    // Subpath checks must precede the catch-all prefix (first match wins).
    if matches_prefix(path, &["crates/julie-runtime/src/watcher/"]) {
        return JULIE_RUNTIME_WATCHER_BUCKETS;
    }
    if matches_prefix(path, &["crates/julie-runtime/src/workspace/"]) {
        return JULIE_RUNTIME_WORKSPACE_BUCKETS;
    }
    if matches_prefix(path, &["crates/julie-runtime/src/"]) {
        // lib.rs, tests/**, other top-level files — covered by `-p julie-runtime`.
        return &["core-runtime"];
    }

    // julie-tools crate (Phase 2b crate split): handler-free tool implementations.
    // Changes to tool source should trigger the relevant behavioral test buckets.
    // Subpath checks must precede the catch-all prefix (first match wins).
    if matches_prefix(path, &["crates/julie-tools/src/search/query_preprocessor"]) {
        return &["tools-search-query"];
    }
    if matches_prefix(path, &["crates/julie-tools/src/search/"]) {
        return JULIE_TOOLS_SEARCH_BUCKETS;
    }
    if matches_prefix(path, &["crates/julie-tools/src/symbols/"]) {
        return &["tools-format-filter"];
    }
    if matches_prefix(path, &["crates/julie-tools/src/impact/"]) {
        return &["tools-blast-spillover"];
    }
    if matches_prefix(path, &["crates/julie-tools/src/spillover/"]) {
        return &["tools-blast-spillover"];
    }
    if matches_prefix(path, &["crates/julie-tools/src/navigation/"]) {
        return &["tools-format-filter", "tools-fast-refs"];
    }
    // Per-file routing for relocated handler-free tool tests (T2b.6).
    // These checks must precede the broad crates/julie-tools/src/ catch-all.
    if matches_prefix(path, &["crates/julie-tools/src/tests/get_context_"]) {
        // Delegate to the per-file get_context routing helper (also covers julie-tools paths).
        if let Some(buckets) = get_context_test_buckets_for_path(path) {
            return buckets;
        }
        return &[
            "tools-get-context-pipeline",
            "tools-get-context-format",
            "tools-get-context-graph",
        ];
    }
    if matches_prefix(path, &["crates/julie-tools/src/tests/deep_dive_"]) {
        return &["tools-deep-dive"];
    }
    if matches_prefix(path, &["crates/julie-tools/src/tests/editing_"]) {
        return &["tools-editing"];
    }
    if matches_prefix(path, &["crates/julie-tools/src/tests/refactoring_"]) {
        return &["tools-refactoring"];
    }
    if matches_prefix(path, &["crates/julie-tools/src/tests/metrics_"]) {
        return &["tools-metrics"];
    }
    if matches_prefix(
        path,
        &[
            "crates/julie-tools/src/tests/search_",
            "crates/julie-tools/src/tests/tantivy_",
        ],
    ) {
        return JULIE_TOOLS_SEARCH_BUCKETS;
    }
    if matches_exact(
        path,
        &[
            "crates/julie-tools/src/tests/formatting_tests.rs",
            "crates/julie-tools/src/tests/filtering_tests.rs",
            "crates/julie-tools/src/tests/query_classification_tests.rs",
            "crates/julie-tools/src/tests/phase4_token_savings.rs",
        ],
    ) {
        return &["tools-format-filter"];
    }
    if matches_prefix(path, &["crates/julie-tools/src/"]) {
        // lib.rs, tests/**, other top-level modules — broad tool coverage.
        return JULIE_TOOLS_BUCKETS;
    }

    &[]
}
