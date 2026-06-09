use xtask::changed::{ChangedSelectionMode, render_changed_selection, select_changed_buckets};
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
        vec![
            "xtask-runner",
            "tools-workspace-discovery",
            "tools-workspace-indexing",
            "tools-workspace-management",
            "tools-workspace-targeting",
            "workspace-init"
        ]
    );
    assert!(selection.fallback_paths.is_empty());
}

#[test]
fn changed_tests_workspace_targeting_files_route_to_targeting_bucket() {
    let manifest = sample_manifest();

    for path in [
        "src/tests/tools/workspace/global_targeting.rs",
        "src/tests/tools/workspace/refresh_routing.rs",
    ] {
        let selection = select_changed_buckets(&manifest, &[path.to_string()]);
        assert_eq!(selection.mode, ChangedSelectionMode::Buckets, "{}", path);
        assert_eq!(
            selection.bucket_names,
            vec!["tools-workspace-targeting"],
            "{}",
            path
        );
        assert!(selection.fallback_paths.is_empty(), "{}", path);
    }
}

#[test]
fn changed_tests_workspace_test_changes_do_not_pull_workspace_init() {
    let manifest = sample_manifest();

    let selection = select_changed_buckets(
        &manifest,
        &["src/tests/tools/workspace/mod_tests.rs".to_string()],
    );

    assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
    assert_eq!(selection.bucket_names, vec!["tools-workspace-indexing"]);
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
        vec![
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
            "search-quality"
        ]
    );
}

#[test]
fn changed_tests_checked_in_manifest_routes_representative_paths_to_production_buckets() {
    let manifest = load_checked_in_manifest();

    for (path, expected_buckets) in [
        (
            "crates/julie-core/src/database/connection.rs",
            vec!["core-database"],
        ),
        // Moved leaf files keep their pre-split behavioral coverage in addition to the
        // leaf crate's own `-p julie-core` test binary (regression guard for the
        // ADR-0006 crate split — a localized edit must not silently run only the DB
        // slice). Bucket order is the canonical sort order from sort_bucket_names.
        (
            "crates/julie-core/src/connection_pool.rs",
            vec!["core-database", "registry"],
        ),
        (
            "crates/julie-core/src/embeddings_contract.rs",
            vec!["core-database", "core-embeddings"],
        ),
        (
            "crates/julie-core/src/paths.rs",
            vec!["core-database", "core-fast"],
        ),
        ("src/tools/editing/edit_file.rs", vec!["tools-editing"]),
        ("src/dashboard/mod.rs", vec!["dashboard"]),
        ("src/registry/lifecycle.rs", vec!["registry"]),
        // Phase 1 T4: julie-index crate split. Editing search source pulls core-index
        // (the crate's own test binary) AND all search tool buckets whose retained
        // tests still cover the moved code (Phase 0 lesson: localized edits must not
        // silently skip behavioral coverage). Analysis source pulls core-index only
        // (the top-crate analysis bucket was removed by T6 — all analysis tests now
        // live in the julie-index binary itself).
        // Bucket order is the canonical sort order from sort_bucket_names.
        (
            "crates/julie-index/src/search/index.rs",
            vec![
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
            ],
        ),
        (
            "crates/julie-index/src/analysis/early_warnings.rs",
            vec!["core-index"],
        ),
        ("crates/julie-index/src/lib.rs", vec!["core-index"]),
    ] {
        let selection = select_changed_buckets(&manifest, &[path.to_string()]);

        assert_eq!(selection.mode, ChangedSelectionMode::Buckets, "{path}");
        assert_eq!(selection.bucket_names, expected_buckets, "{path}");
        assert!(selection.fallback_paths.is_empty(), "{path}");
    }
}

#[test]
fn changed_tests_julie_runtime_crate_split_routing() {
    let manifest = load_checked_in_manifest();

    for (path, expected_buckets) in [
        // Watcher source → core-runtime + workspace-runtime (daemon pool tests
        // exercise the watcher lifecycle from above — R6 co-target).
        (
            "crates/julie-runtime/src/watcher/mod.rs",
            vec!["core-runtime", "workspace-runtime"],
        ),
        // Workspace source → core-runtime + workspace-runtime + workspace-init.
        (
            "crates/julie-runtime/src/workspace/registry.rs",
            vec!["core-runtime", "workspace-runtime", "workspace-init"],
        ),
        // lib.rs / catch-all → core-runtime only.
        ("crates/julie-runtime/src/lib.rs", vec!["core-runtime"]),
    ] {
        let selection = select_changed_buckets(&manifest, &[path.to_string()]);
        assert_eq!(selection.mode, ChangedSelectionMode::Buckets, "{path}");
        assert_eq!(selection.bucket_names, expected_buckets, "{path}");
        assert!(selection.fallback_paths.is_empty(), "{path}");
    }
}

#[test]
fn changed_tests_julie_tools_crate_split_routing() {
    let manifest = sample_manifest();
    for (path, expected_buckets) in [
        // Specific query_preprocessor subpath → search-query only.
        (
            "crates/julie-tools/src/search/query_preprocessor.rs",
            vec!["tools-search-query"],
        ),
        // Broad search subpath → search tool buckets (canonical sort order).
        (
            "crates/julie-tools/src/search/execution.rs",
            vec![
                "tools-search-promotion",
                "tools-search-format-quality",
                "tools-search-context",
                "tools-search-text",
                "tools-search-hybrid",
                "tools-search-query",
                "tools-search-unified",
            ],
        ),
        // symbols/ → format-filter.
        (
            "crates/julie-tools/src/symbols/filtering.rs",
            vec!["tools-format-filter"],
        ),
        // impact/ → blast-spillover.
        (
            "crates/julie-tools/src/impact/mod.rs",
            vec!["tools-blast-spillover"],
        ),
        // spillover/ → blast-spillover.
        (
            "crates/julie-tools/src/spillover/store.rs",
            vec!["tools-blast-spillover"],
        ),
        // navigation/ → fast-refs + format-filter (canonical sort order).
        (
            "crates/julie-tools/src/navigation/formatting.rs",
            vec!["tools-fast-refs", "tools-format-filter"],
        ),
        // catch-all lib.rs → broad tool coverage (canonical sort order).
        (
            "crates/julie-tools/src/lib.rs",
            vec![
                "tools-search-hybrid",
                "tools-search-query",
                "tools-blast-spillover",
                "tools-format-filter",
            ],
        ),
    ] {
        let selection = select_changed_buckets(&manifest, &[path.to_string()]);

        assert_eq!(selection.mode, ChangedSelectionMode::Buckets, "{path}");
        assert_eq!(selection.bucket_names, expected_buckets, "{path}");
        assert!(selection.fallback_paths.is_empty(), "{path}");
    }
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

#[test]
fn changed_tests_root_manifest_changes_fall_back_to_dev() {
    // The external julie-extractors crate is now a git dependency; a re-pin (or any
    // other dependency bump) shows up as a root Cargo.toml/Cargo.lock edit, which falls
    // back to the full dev tier rather than a thin extractor-only bucket. The dev tier
    // includes extractor-dep-integration, so the extractor gate still runs.
    let manifest = sample_manifest();

    let selection = select_changed_buckets(
        &manifest,
        &["Cargo.toml".to_string(), "Cargo.lock".to_string()],
    );

    assert_eq!(selection.mode, ChangedSelectionMode::FallbackToDev);
    assert!(selection.fallback_paths.contains(&"Cargo.toml".to_string()));
    assert!(selection.fallback_paths.contains(&"Cargo.lock".to_string()));
    assert!(
        selection
            .bucket_names
            .contains(&"extractor-dep-integration".to_string())
    );
}

#[test]
fn changed_tests_reports_fallback_prefix_rationale() {
    let manifest = sample_manifest();

    let selection =
        select_changed_buckets(&manifest, &["src/tests/fixtures/example.rs".to_string()]);
    let output = render_changed_selection(&selection);

    assert_eq!(selection.mode, ChangedSelectionMode::FallbackToDev);
    assert!(output.contains(
        "CHANGED: rationale: src/tests/fixtures/example.rs -> dev (fallback prefix: src/tests/fixtures/)"
    ));
}

#[test]
fn changed_tests_reports_path_to_bucket_rationale() {
    let manifest = sample_manifest();

    let selection = select_changed_buckets(&manifest, &["src/tools/search/mod.rs".to_string()]);
    let output = render_changed_selection(&selection);

    assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
    assert_eq!(
        selection.bucket_names,
        vec![
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
        ]
    );
    assert!(output.contains("CHANGED: rationale: src/tools/search/mod.rs -> tools-search-tantivy, tools-search-line-core, tools-search-line-filters, tools-search-line-primary, tools-search-file-mode, tools-search-zero-hit, tools-search-promotion, tools-search-format-quality, tools-search-context, tools-search-text, tools-search-hybrid, tools-search-query"));
}

#[test]
fn changed_tests_search_paths_select_split_search_buckets() {
    let manifest = sample_manifest();

    let selection = select_changed_buckets(
        &manifest,
        &[
            "src/tests/tools/search/tantivy_tokenizer_tests.rs".to_string(),
            "src/tests/tools/search/line_mode/basic.rs".to_string(),
            "src/tests/tools/search/line_mode/filters.rs".to_string(),
            "src/tests/tools/search/line_mode/primary_rebind.rs".to_string(),
            "src/tests/tools/search/file_mode_tests.rs".to_string(),
            "src/tests/tools/search/primary_workspace_bug.rs".to_string(),
            "src/tests/tools/search/definition_overfetch_tests.rs".to_string(),
            "src/tests/tools/search/content_scoring_tests.rs".to_string(),
            "src/tests/tools/search_context_lines.rs".to_string(),
            "src/tests/tools/text_search_tantivy.rs".to_string(),
            "src/tests/tools/hybrid_search_tests.rs".to_string(),
            "src/tools/search/query_preprocessor.rs".to_string(),
        ],
    );

    assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
    assert_eq!(
        selection.bucket_names,
        vec![
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
        ]
    );
}

#[test]
fn changed_tests_xtask_paths_select_xtask_runner_bucket() {
    let manifest = sample_manifest();

    let selection = select_changed_buckets(&manifest, &["xtask/src/runner.rs".to_string()]);

    assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
    assert_eq!(selection.bucket_names, vec!["xtask-runner"]);
}

#[test]
fn changed_tests_harness_docs_select_xtask_runner_bucket() {
    let manifest = sample_manifest();

    let selection = select_changed_buckets(
        &manifest,
        &[
            "AGENTS.md".to_string(),
            "CLAUDE.md".to_string(),
            ".cargo/config.toml".to_string(),
            "docs/TESTING_GUIDE.md".to_string(),
            "docs/plans/verification-ledger-template.md".to_string(),
        ],
    );

    assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
    assert_eq!(selection.bucket_names, vec!["xtask-runner"]);
    assert!(selection.ignored_paths.is_empty());
}

#[test]
fn changed_tests_misc_tool_paths_select_split_tool_buckets() {
    let manifest = sample_manifest();

    let selection = select_changed_buckets(
        &manifest,
        &[
            "src/tools/symbols/mod.rs".to_string(),
            "src/tools/editing/rewrite.rs".to_string(),
            "src/tools/deep_dive/mod.rs".to_string(),
            "src/tools/refactoring/rename.rs".to_string(),
            "src/tools/metrics/centrality.rs".to_string(),
            // filtering_tests relocated to julie-tools (T2b.6); use new path
            "crates/julie-tools/src/tests/filtering_tests.rs".to_string(),
        ],
    );

    assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
    assert_eq!(
        selection.bucket_names,
        vec![
            "tools-get-symbols",
            "tools-editing",
            "tools-deep-dive",
            "tools-refactoring",
            "tools-metrics",
            "tools-format-filter",
        ]
    );
}

#[test]
fn changed_tests_handler_tool_fast_search_selects_search_buckets() {
    let manifest = sample_manifest();

    let selection =
        select_changed_buckets(&manifest, &["src/handler/tools/fast_search.rs".to_string()]);

    assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
    assert_eq!(
        selection.bucket_names,
        vec![
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
        ]
    );
    assert!(selection.fallback_paths.is_empty());
}

#[test]
fn changed_tests_handler_tool_navigation_files_route_per_tool() {
    let manifest = sample_manifest();

    let cases: &[(&str, &[&str])] = &[
        ("src/handler/tools/fast_refs.rs", &["tools-fast-refs"]),
        ("src/handler/tools/call_path.rs", &["tools-call-path"]),
        ("src/handler/tools/deep_dive.rs", &["tools-deep-dive"]),
        (
            "src/handler/tools/blast_radius.rs",
            &["tools-blast-spillover"],
        ),
        (
            "src/handler/tools/spillover_get.rs",
            &["tools-blast-spillover"],
        ),
    ];

    for (path, expected) in cases {
        let selection = select_changed_buckets(&manifest, &[path.to_string()]);
        assert_eq!(selection.mode, ChangedSelectionMode::Buckets, "{}", path);
        let expected_owned: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
        assert_eq!(selection.bucket_names, expected_owned, "{}", path);
        assert!(selection.fallback_paths.is_empty(), "{}", path);
    }

    // All five together should yield the union (sorted).
    let all_paths: Vec<String> = cases.iter().map(|(p, _)| p.to_string()).collect();
    let selection = select_changed_buckets(&manifest, &all_paths);
    assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
    assert_eq!(
        selection.bucket_names,
        vec![
            "tools-deep-dive",
            "tools-call-path",
            "tools-fast-refs",
            "tools-blast-spillover",
        ]
    );
}

#[test]
fn changed_tests_deep_dive_split_modules_route_to_deep_dive_bucket() {
    let manifest = sample_manifest();

    for path in [
        "src/tests/tools/deep_dive_tests/deserialization_tests.rs",
        "src/tests/tools/deep_dive_tests/formatting_tests/callable_core.rs",
        "src/tests/tools/deep_dive_tests/data_tests/identifiers_query_similarity.rs",
    ] {
        let selection = select_changed_buckets(&manifest, &[path.to_string()]);
        assert_eq!(selection.mode, ChangedSelectionMode::Buckets, "{path}");
        assert_eq!(selection.bucket_names, vec!["tools-deep-dive"], "{path}");
        assert!(selection.fallback_paths.is_empty(), "{path}");
    }
}

#[test]
fn changed_tests_handler_tool_files_select_specific_buckets() {
    let manifest = sample_manifest();

    let selection = select_changed_buckets(
        &manifest,
        &[
            "src/handler/tools/get_symbols.rs".to_string(),
            "src/handler/tools/get_context.rs".to_string(),
            "src/handler/tools/rename_symbol.rs".to_string(),
            "src/handler/tools/manage_workspace.rs".to_string(),
            "src/handler/tools/edit_file.rs".to_string(),
            "src/handler/tools/rewrite_symbol.rs".to_string(),
        ],
    );

    assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
    assert_eq!(
        selection.bucket_names,
        vec![
            "tools-get-context-pipeline",
            "tools-get-context-format",
            "tools-get-context-graph",
            "tools-workspace-discovery",
            "tools-workspace-indexing",
            "tools-workspace-management",
            "tools-workspace-targeting",
            "tools-get-symbols",
            "tools-editing",
            "tools-refactoring",
            "workspace-init",
        ]
    );
    assert!(selection.fallback_paths.is_empty());
}

#[test]
fn changed_tests_route_handler_split_modules_to_core_fast_bucket() {
    let manifest = sample_manifest();

    for path in [
        "src/tests/core/handler/public_surface.rs",
        "src/tests/core/handler/editing_metrics.rs",
        "src/tests/core/handler/workspace_binding_metrics.rs",
    ] {
        let selection = select_changed_buckets(&manifest, &[path.to_string()]);
        assert_eq!(selection.mode, ChangedSelectionMode::Buckets, "{path}");
        assert_eq!(selection.bucket_names, vec!["core-fast"], "{path}");
        assert!(selection.fallback_paths.is_empty(), "{path}");
    }
}

#[test]
fn changed_tests_handler_search_telemetry_selects_search_buckets() {
    let manifest = sample_manifest();

    let selection =
        select_changed_buckets(&manifest, &["src/handler/search_telemetry.rs".to_string()]);

    assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
    assert_eq!(
        selection.bucket_names,
        vec![
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
        ]
    );
}

#[test]
fn changed_tests_handler_cross_cutting_files_route_to_specific_buckets() {
    let manifest = sample_manifest();

    let cases: &[(&str, &[&str])] = &[
        ("src/handler/session_workspace.rs", &["registry"]),
        (
            "src/handler/tool_metrics.rs",
            &["tools-metrics", "registry"],
        ),
        (
            "src/handler/tool_targets.rs",
            &[
                "tools-workspace-discovery",
                "tools-workspace-indexing",
                "tools-workspace-management",
                "tools-workspace-targeting",
                "registry",
            ],
        ),
        ("src/handler/tools/mod.rs", &["registry"]),
    ];

    for (path, expected) in cases {
        let selection = select_changed_buckets(&manifest, &[path.to_string()]);
        assert_eq!(
            selection.mode,
            ChangedSelectionMode::Buckets,
            "expected {} to route to specific buckets",
            path
        );
        assert!(
            selection.fallback_paths.is_empty(),
            "{} should not fall back",
            path
        );
        let expected_owned: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
        assert_eq!(
            selection.bucket_names, expected_owned,
            "unexpected buckets for {}",
            path
        );
    }
}

#[test]
fn changed_tests_handler_central_handler_rs_still_falls_back_to_dev() {
    let manifest = sample_manifest();

    let selection = select_changed_buckets(&manifest, &["src/handler.rs".to_string()]);
    assert_eq!(selection.mode, ChangedSelectionMode::FallbackToDev);
    assert_eq!(selection.fallback_paths, vec!["src/handler.rs"]);
}

#[test]
fn changed_tests_deleted_migration_paths_fall_back_to_dev() {
    let manifest = sample_manifest();

    for path in ["src/migration.rs", "src/tests/migration.rs"] {
        let selection = select_changed_buckets(&manifest, &[path.to_string()]);
        assert_eq!(
            selection.mode,
            ChangedSelectionMode::FallbackToDev,
            "{}",
            path
        );
        assert_eq!(selection.fallback_paths, vec![path.to_string()], "{}", path);
    }
}

#[test]
fn changed_tests_startup_routes_to_lifecycle_workspace_runtime_and_workspace() {
    let manifest = sample_manifest();

    let selection = select_changed_buckets(&manifest, &["src/startup.rs".to_string()]);
    assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
    assert_eq!(
        selection.bucket_names,
        vec![
            "tools-workspace-discovery",
            "tools-workspace-indexing",
            "tools-workspace-management",
            "lifecycle",
            "workspace-runtime"
        ]
    );
    assert!(selection.fallback_paths.is_empty());
}

#[test]
fn changed_tests_src_extractors_reexport_routes_to_extractor_dep_integration_bucket() {
    let manifest = sample_manifest();

    let selection = select_changed_buckets(&manifest, &["src/extractors/mod.rs".to_string()]);
    assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
    assert_eq!(selection.bucket_names, vec!["extractor-dep-integration"]);
    assert!(selection.fallback_paths.is_empty());
}

#[test]
fn changed_tests_routes_projection_paths_to_projection_bucket() {
    let manifest = sample_manifest();

    let selection = select_changed_buckets(&manifest, &["src/search/projection.rs".to_string()]);

    assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
    assert_eq!(selection.bucket_names, vec!["projection"]);
}

#[test]
fn changed_tests_routes_projection_pipeline_paths_to_projection_bucket() {
    let manifest = sample_manifest();

    let selection = select_changed_buckets(
        &manifest,
        &["src/tools/workspace/indexing/pipeline.rs".to_string()],
    );

    assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
    assert_eq!(selection.bucket_names, vec!["projection"]);
}

#[test]
fn changed_tests_routes_lifecycle_paths_to_daemon_bucket() {
    let manifest = sample_manifest();

    let selection = select_changed_buckets(&manifest, &["src/registry/lifecycle.rs".to_string()]);

    assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
    assert_eq!(selection.bucket_names, vec!["registry"]);
}

#[test]
fn changed_tests_routes_daemon_mod_to_daemon_bucket() {
    let manifest = sample_manifest();

    let selection = select_changed_buckets(&manifest, &["src/registry/mod.rs".to_string()]);

    assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
    assert_eq!(selection.bucket_names, vec!["registry"]);
}

#[test]
fn changed_tests_deleted_transport_paths_fall_back_to_dev() {
    let manifest = sample_manifest();

    for path in ["src/adapter/mod.rs", "src/registry/transport.rs"] {
        let selection = select_changed_buckets(&manifest, &[path.to_string()]);

        assert_eq!(
            selection.mode,
            ChangedSelectionMode::FallbackToDev,
            "{path}"
        );
        assert_eq!(selection.fallback_paths, vec![path.to_string()], "{path}");
    }
}

#[test]
fn changed_tests_deleted_http_transport_paths_fall_back_to_dev() {
    let manifest = sample_manifest();

    let selection = select_changed_buckets(
        &manifest,
        &[
            "src/registry/http_transport.rs".to_string(),
            "src/tests/registry/http_transport/tests/restart_pending.rs".to_string(),
        ],
    );

    assert_eq!(selection.mode, ChangedSelectionMode::FallbackToDev);
    assert_eq!(
        selection.fallback_paths,
        vec![
            "src/registry/http_transport.rs".to_string(),
            "src/tests/registry/http_transport/tests/restart_pending.rs".to_string(),
        ]
    );
}

#[test]
fn changed_tests_deleted_workspace_pool_paths_fall_back_to_dev() {
    let manifest = sample_manifest();

    let selection =
        select_changed_buckets(&manifest, &["src/registry/workspace_pool.rs".to_string()]);

    assert_eq!(selection.mode, ChangedSelectionMode::FallbackToDev);
    assert_eq!(
        selection.fallback_paths,
        vec!["src/registry/workspace_pool.rs".to_string()]
    );
}

#[test]
fn changed_tests_routes_workspace_registry_commands_to_workspace_runtime_bucket() {
    let manifest = sample_manifest();

    let selection = select_changed_buckets(
        &manifest,
        &["src/tools/workspace/commands/registry/open.rs".to_string()],
    );

    assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
    assert_eq!(selection.bucket_names, vec!["workspace-runtime"]);
}

#[test]
fn changed_tests_ignored_docs_only_output_remains_concise() {
    let manifest = sample_manifest();

    let selection = select_changed_buckets(
        &manifest,
        &[
            "docs/PRE-RELEASE-FINDINGS.md".to_string(),
            ".memories/checkpoints/example.md".to_string(),
        ],
    );
    let output = render_changed_selection(&selection);

    assert_eq!(selection.mode, ChangedSelectionMode::NoChanges);
    assert_eq!(
        output,
        "CHANGED: no code/test buckets matched local changes\n\
CHANGED: ignored non-executable paths: .memories/checkpoints/example.md, docs/PRE-RELEASE-FINDINGS.md\n"
    );
}

fn load_checked_in_manifest() -> TestManifest {
    TestManifest::load(format!("{}/test_tiers.toml", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn sample_manifest() -> TestManifest {
    TestManifest::from_str(
        r#"
[tiers]
dev = [
  "cli",
  "xtask-runner",
  "tools-workspace-discovery",
  "tools-workspace-indexing",
  "tools-workspace-management",
  "tools-workspace-targeting",
  "workspace-init",
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
  "tools-get-symbols",
  "tools-get-context-pipeline",
  "tools-get-context-format",
  "tools-get-context-graph",
  "tools-editing",
  "tools-deep-dive",
  "tools-call-path",
  "tools-fast-refs",
  "tools-blast-spillover",
  "tools-refactoring",
  "tools-metrics",
  "tools-format-filter",
  "core-fast",
  "search-quality",
  "extractor-dep-integration",
]
dogfood = ["tools-dogfood-repo-index", "search-quality"]

[buckets.cli]
expected_seconds = 5
timeout_seconds = 30
commands = ["cargo test --lib tests::cli_tests"]

[buckets.xtask-runner]
expected_seconds = 5
timeout_seconds = 30
commands = ["cargo nextest run -p xtask"]

[buckets.core-database]
expected_seconds = 5
timeout_seconds = 30
commands = ["cargo nextest run --lib tests::core::database -- --skip search_quality"]

[buckets.core-fast]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo nextest run --lib tests::core::handler -- --skip search_quality"]

[buckets.tools-workspace-discovery]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::tools::workspace::discovery"]

[buckets.tools-workspace-indexing]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::tools::workspace::mod_tests"]

[buckets.tools-workspace-management]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::tools::workspace::isolation"]

[buckets.tools-workspace-targeting]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::tools::workspace::global_targeting"]

[buckets.workspace-init]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::core::workspace_init"]

[buckets.tools-search-tantivy]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::tools::search::tantivy_"]

[buckets.tools-search-line-core]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::tools::search::line_mode::basic"]

[buckets.tools-search-line-filters]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::tools::search::line_mode::filters"]

[buckets.tools-search-line-primary]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::tools::search::line_mode::primary_rebind"]

[buckets.tools-search-file-mode]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::tools::search::file_"]

[buckets.tools-search-zero-hit]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::tools::search::primary_workspace_bug"]

[buckets.tools-search-promotion]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::tools::search::promotion_tests"]

[buckets.tools-search-format-quality]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::tools::search::content_scoring_tests"]

[buckets.tools-search-context]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::tools::search_context_lines"]

[buckets.tools-search-text]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::tools::text_search_tantivy"]

[buckets.tools-search-hybrid]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::tools::hybrid_search_tests"]

[buckets.tools-search-query]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tools::search::query_preprocessor::tests"]

[buckets.tools-search-unified]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo nextest run -p julie-tools --lib tests::unified_search_tests"]

[buckets.tools-get-symbols]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::tools::get_symbols::"]

[buckets.tools-get-context-pipeline]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::tools::get_context_pipeline_tests"]

[buckets.tools-get-context-format]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::tools::get_context_formatting_tests"]

[buckets.tools-get-context-graph]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::tools::get_context_graph_expansion_tests"]

[buckets.tools-editing]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::tools::editing::"]

[buckets.tools-deep-dive]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::tools::deep_dive_tests"]

[buckets.tools-call-path]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::tools::call_path_tests"]

[buckets.tools-fast-refs]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::tools::fast_refs_primary_rebind_tests"]

[buckets.tools-blast-spillover]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::tools::blast_radius"]

[buckets.tools-refactoring]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::tools::refactoring::"]

[buckets.tools-metrics]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::tools::metrics::"]

[buckets.tools-format-filter]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::tools::filtering_tests"]

[buckets.search-quality]
expected_seconds = 60
timeout_seconds = 120
commands = ["cargo test --lib search_quality"]

[buckets.tools-dogfood-repo-index]
expected_seconds = 200
timeout_seconds = 450
commands = ["cargo nextest run --lib tests::tools::get_symbols_target_filtering_dogfood -- --skip search_quality"]

[buckets.extractor-dep-integration]
expected_seconds = 60
timeout_seconds = 180
commands = [
  "cargo nextest run --lib test_semantic_index_engine_version_includes_extraction_contract",
  "cargo nextest run --lib real_world_parser_upgrade_contracts_assert_expected_outputs",
]

[buckets.projection]
expected_seconds = 40
timeout_seconds = 90
commands = ["cargo nextest run --lib tests::integration::projection_repair -- --skip search_quality"]

[buckets.transport]
expected_seconds = 40
timeout_seconds = 90
commands = ["cargo nextest run --lib tests::registry::transport -- --skip search_quality"]

[buckets.lifecycle]
expected_seconds = 40
timeout_seconds = 90
commands = ["cargo nextest run --lib tests::registry::lifecycle -- --skip search_quality"]

[buckets.registry]
expected_seconds = 40
timeout_seconds = 90
commands = ["cargo nextest run --lib tests::registry -- --skip search_quality"]

[buckets.workspace-runtime]
expected_seconds = 40
timeout_seconds = 90
commands = ["cargo nextest run --lib tests::registry::workspace_pool -- --skip search_quality"]

    "#,
    )
    .unwrap()
}
