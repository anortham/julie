use super::*;

pub(super) fn buckets_for_path(path: &str) -> Option<&'static [&'static str]> {
    let buckets = mapped_buckets_for_path(path);
    (!buckets.is_empty()).then_some(buckets)
}

fn mapped_buckets_for_path(path: &str) -> &'static [&'static str] {
    if matches_prefix(path, &["xtask/"]) {
        return &["xtask-runner"];
    }

    if matches_prefix(path, &["xtask-eval/"]) {
        return &["xtask-eval"];
    }

    if let Some(buckets) = handler_tool_buckets_for_path(path) {
        return buckets;
    }

    // src/extractors/ is the thin re-export wrapper over the external julie-extractors
    // crate; a change there runs the julie-owned dep-integration gate. Root
    // Cargo.toml/Cargo.lock are intentionally NOT routed here — they fall back to the
    // full dev tier (which includes extractor-dep-integration), so a general dependency
    // bump is not under-tested by a thin extractor gate.
    if matches_prefix(path, &["src/extractors/"]) {
        return &["extractor-dep-integration"];
    }

    // Handler cross-cutting subfiles map to specific buckets so an edit doesn't drag the
    // whole dev tier in.
    if path == "src/handler/session_workspace.rs" {
        return &["registry"];
    }
    if path == "src/handler/tool_metrics.rs" {
        return &["tools-metrics", "registry"];
    }
    if path == "src/handler/tool_targets.rs" {
        return &[
            "tools-workspace-discovery",
            "tools-workspace-indexing",
            "tools-workspace-management",
            "tools-workspace-targeting",
            "registry",
        ];
    }
    if path == "src/handler/tools/mod.rs" {
        // Pure module declaration file. Re-routes to registry (handler trait surface);
        // any added tool also touches its dedicated handler/tools/<tool>.rs file which
        // pulls the right bucket independently.
        return &["registry"];
    }

    // Startup routing touches DaemonDatabase, workspace registry, and indexing;
    // it no longer needs to force the full dev tier.
    if path == "src/startup.rs" {
        return &[
            "tools-workspace-discovery",
            "tools-workspace-indexing",
            "tools-workspace-management",
            "lifecycle",
            "workspace-runtime",
        ];
    }

    if matches_exact(
        path,
        &[
            "README.md",
            "AGENTS.md",
            "CLAUDE.md",
            ".cargo/config.toml",
            "docs/TESTING_GUIDE.md",
            "docs/plans/verification-ledger-template.md",
        ],
    ) {
        return &["xtask-runner"];
    }

    if matches_exact(
        path,
        &[
            "src/cli.rs",
            "src/tests/cli_tests.rs",
            "src/tests/cli_execution_tests.rs",
            "src/tests/cli_tools_tests.rs",
            "src/tests/cli/cli_search_no_target_test.rs",
        ],
    ) {
        return &["cli"];
    }

    if matches_prefix(path, &["src/embeddings/"])
        || matches_prefix(path, &["src/tests/core/embedding"])
        || matches_prefix(path, &["src/tests/core/sidecar"])
    {
        return &["core-embeddings"];
    }

    // julie-core leaf crate (Phase 0 crate split). Editing any leaf source compiles +
    // runs the crate's own test binary via `core-database` (`cargo nextest run -p
    // julie-core`, which holds the relocated DB/vector tests). The three leaf files
    // whose *behavioral* tests still live in top-crate buckets ALSO pull that bucket,
    // so a localized edit to moved leaf code keeps its original regression coverage
    // (the pre-split mappings were src/registry/* -> registry, src/embeddings/* ->
    // core-embeddings, src/utils/* -> core-fast) instead of silently running only the
    // DB slice. Specific files must precede the catch-all prefix (first match wins).
    &[]
}

fn handler_tool_buckets_for_path(path: &str) -> Option<&'static [&'static str]> {
    match path {
        "src/handler/tools/fast_search.rs" => Some(SEARCH_TOOL_BUCKETS),
        "src/handler/tools/fast_refs.rs" => Some(&["tools-fast-refs"]),
        "src/handler/tools/call_path.rs" => Some(&["tools-call-path"]),
        "src/handler/tools/deep_dive.rs" => Some(&["tools-deep-dive"]),
        "src/handler/tools/blast_radius.rs" | "src/handler/tools/spillover_get.rs" => {
            Some(&["tools-blast-spillover"])
        }
        "src/handler/tools/get_symbols.rs" => Some(&["tools-get-symbols"]),
        "src/handler/tools/get_context.rs" => Some(&[
            "tools-get-context-pipeline",
            "tools-get-context-format",
            "tools-get-context-graph",
        ]),
        "src/handler/tools/rename_symbol.rs" => Some(&["tools-refactoring"]),
        "src/handler/tools/manage_workspace.rs" => Some(&[
            "tools-workspace-discovery",
            "tools-workspace-indexing",
            "tools-workspace-management",
            "tools-workspace-targeting",
            "workspace-init",
        ]),
        "src/handler/tools/edit_file.rs" | "src/handler/tools/rewrite_symbol.rs" => {
            Some(&["tools-editing"])
        }
        "src/handler/search_telemetry.rs" => Some(SEARCH_TOOL_BUCKETS_WITH_HANDLER_TELEMETRY),
        _ => None,
    }
}
