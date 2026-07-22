use super::{ChangedSelectionMode, select_changed_buckets};
use crate::manifest::TestManifest;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask manifest dir has repo parent")
        .to_path_buf()
}

fn manifest() -> TestManifest {
    TestManifest::load(repo_root().join("xtask/test_tiers.toml")).expect("load test manifest")
}

fn expected_mapped_mode(manifest: &TestManifest, bucket_names: &[String]) -> ChangedSelectionMode {
    let declared =
        crate::runner::declared_expected_seconds(manifest, bucket_names.iter().map(String::as_str));
    if declared > 60 {
        ChangedSelectionMode::OverBudget
    } else {
        ChangedSelectionMode::Buckets
    }
}

fn bucket_commands<'a>(manifest: &'a TestManifest, bucket_name: &str) -> &'a [String] {
    manifest
        .buckets
        .get(bucket_name)
        .unwrap_or_else(|| panic!("missing bucket {bucket_name}"))
        .commands
        .as_slice()
}

fn command_covers_module(command: &str, module: &str) -> bool {
    let Some(filter) = command
        .split("tests::tools::search::")
        .nth(1)
        .and_then(|tail| tail.split_whitespace().next())
    else {
        return false;
    };

    module.starts_with(filter) || filter.starts_with(module)
}

fn declared_search_modules(path: &Path) -> Vec<String> {
    let source = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    source
        .lines()
        .filter_map(|line| {
            line.trim()
                .strip_prefix("mod ")
                .and_then(|rest| rest.strip_suffix(';'))
                .map(str::to_string)
        })
        .collect()
}

#[test]
fn changed_tests_preserve_mapped_buckets_when_other_paths_fallback_to_dev() {
    let manifest = manifest();
    let selection = select_changed_buckets(
        &manifest,
        &[
            "src/tests/integration/stale_index_detection.rs".to_string(),
            "unmapped/tooling-note.txt".to_string(),
        ],
    );

    assert_eq!(selection.mode, ChangedSelectionMode::FallbackToDev);
    assert!(
        selection
            .bucket_names
            .iter()
            .any(|bucket| bucket == "integration"),
        "mapped integration coverage must survive fallback; buckets={:?}, rationale={:?}",
        selection.bucket_names,
        selection.rationale
    );
    assert!(
        selection
            .fallback_paths
            .iter()
            .any(|path| path == "unmapped/tooling-note.txt"),
        "fallback path should still be reported"
    );
}

#[test]
fn changed_tests_route_hybrid_search_split_modules_to_hybrid_bucket() {
    let manifest = manifest();
    let selection = select_changed_buckets(
        &manifest,
        &[
            "src/tests/tools/hybrid_search_tests/orchestrator.rs".to_string(),
            "src/tests/tools/hybrid_search_tests/knn_conversion.rs".to_string(),
        ],
    );

    assert_eq!(
        selection.mode,
        expected_mapped_mode(&manifest, &selection.bucket_names)
    );
    assert_eq!(selection.bucket_names, vec!["tools-search-hybrid"]);
    assert!(
        selection.fallback_paths.is_empty(),
        "split hybrid test modules should not force dev fallback; rationale={:?}",
        selection.rationale
    );
}

#[test]
fn changed_tests_route_global_targeting_split_modules_to_targeting_bucket() {
    let manifest = manifest();
    let selection = select_changed_buckets(
        &manifest,
        &["src/tests/tools/workspace/global_targeting/target_activation.rs".to_string()],
    );

    assert_eq!(
        selection.mode,
        expected_mapped_mode(&manifest, &selection.bucket_names)
    );
    assert_eq!(selection.bucket_names, vec!["tools-workspace-targeting"]);
    assert!(
        selection.fallback_paths.is_empty(),
        "split global targeting modules should not route through the broad workspace bucket; rationale={:?}",
        selection.rationale
    );
}

#[test]
fn changed_tests_route_workspace_split_modules_to_narrow_buckets() {
    let manifest = manifest();

    for (path, expected_bucket) in [
        (
            "src/tests/tools/workspace/discovery/file_filtering.rs",
            "tools-workspace-discovery",
        ),
        (
            "src/tests/tools/workspace/mod_tests/part1.rs",
            "tools-workspace-indexing",
        ),
        (
            "src/tests/tools/workspace/isolation.rs",
            "tools-workspace-management",
        ),
    ] {
        let selection = select_changed_buckets(&manifest, &[path.to_string()]);
        assert_eq!(
            selection.mode,
            expected_mapped_mode(&manifest, &selection.bucket_names),
            "{path}"
        );
        assert_eq!(selection.bucket_names, vec![expected_bucket], "{path}");
        assert!(
            selection.fallback_paths.is_empty(),
            "{path} should not fall back; rationale={:?}",
            selection.rationale
        );
    }
}

#[test]
fn changed_tests_route_line_mode_split_modules_to_narrow_buckets() {
    let manifest = manifest();

    for (path, expected_bucket) in [
        (
            "src/tests/tools/search/line_mode/basic.rs",
            "tools-search-line-core",
        ),
        (
            "src/tests/tools/search/line_mode/filters.rs",
            "tools-search-line-filters",
        ),
        (
            "src/tests/tools/search/line_mode_or_fallback_tests.rs",
            "tools-search-line-filters",
        ),
        (
            "src/tests/tools/search/line_mode_second_pass_tests.rs",
            "tools-search-line-filters",
        ),
        (
            "src/tests/tools/search/line_mode/missing_index.rs",
            "tools-search-line-primary",
        ),
        (
            "src/tests/tools/search/line_mode/primary_rebind.rs",
            "tools-search-line-primary",
        ),
    ] {
        let selection = select_changed_buckets(&manifest, &[path.to_string()]);
        assert_eq!(
            selection.mode,
            expected_mapped_mode(&manifest, &selection.bucket_names),
            "{path}"
        );
        assert_eq!(selection.bucket_names, vec![expected_bucket], "{path}");
        assert!(
            selection.fallback_paths.is_empty(),
            "{path} should not fall back; rationale={:?}",
            selection.rationale
        );
    }
}

#[test]
fn changed_tests_route_target_workspace_fast_refs_split_modules_to_fast_refs_bucket() {
    let manifest = manifest();
    let selection = select_changed_buckets(
        &manifest,
        &["src/tests/tools/target_workspace_fast_refs_tests/tests/limits.rs".to_string()],
    );

    assert_eq!(
        selection.mode,
        expected_mapped_mode(&manifest, &selection.bucket_names)
    );
    assert_eq!(selection.bucket_names, vec!["tools-fast-refs"]);
    assert!(
        selection.fallback_paths.is_empty(),
        "split target-workspace fast_refs modules should route through tools-fast-refs; rationale={:?}",
        selection.rationale
    );
}

#[test]
fn changed_tests_route_watcher_filtering_to_core_runtime_bucket() {
    // T2c.2/T2c.3: watcher source + tests relocated to crates/julie-runtime.
    // Editing watcher source routes to core-runtime (the crate's own test binary)
    // plus workspace-runtime (daemon watcher-pool behavioral co-target).
    let manifest = manifest();
    let selection = select_changed_buckets(
        &manifest,
        &["crates/julie-runtime/src/watcher/filtering.rs".to_string()],
    );

    assert_eq!(
        selection.mode,
        expected_mapped_mode(&manifest, &selection.bucket_names)
    );
    assert_eq!(
        selection.bucket_names,
        vec!["core-runtime", "workspace-runtime"]
    );
    assert!(
        selection.fallback_paths.is_empty(),
        "watcher source should not force dev fallback; rationale={:?}",
        selection.rationale
    );
}

#[test]
fn changed_tests_route_pipeline_embeddings_to_core_pipeline_and_embeddings() {
    let manifest = manifest();
    let selection = select_changed_buckets(
        &manifest,
        &["crates/julie-pipeline/src/embeddings/sidecar_supervisor.rs".to_string()],
    );

    assert_eq!(
        selection.mode,
        expected_mapped_mode(&manifest, &selection.bucket_names)
    );
    // Embedding behavioral tests stayed in the top crate (core-embeddings) and
    // exercise the moved code via shim re-exports, so both buckets must run.
    assert_eq!(
        selection.bucket_names,
        vec!["core-embeddings", "core-pipeline"],
        "rationale={:?}",
        selection.rationale
    );
}

#[test]
fn changed_tests_route_pipeline_indexing_core_to_behavioral_buckets() {
    let manifest = manifest();
    let selection = select_changed_buckets(
        &manifest,
        &["crates/julie-pipeline/src/indexing_core/extraction.rs".to_string()],
    );

    assert_eq!(
        selection.mode,
        expected_mapped_mode(&manifest, &selection.bucket_names)
    );
    // indexing engine: crate unit tests (core-pipeline) + the retained
    // end-to-end indexing guards (R6 co-targeting).
    assert_eq!(
        selection.bucket_names,
        vec![
            "core-pipeline",
            "tools-workspace-discovery",
            "tools-workspace-indexing",
            "tools-workspace-management",
            "workspace-init",
            "integration"
        ],
        "rationale={:?}",
        selection.rationale
    );
}

#[test]
fn changed_tests_route_pipeline_resolver_and_finalize_to_behavioral_buckets() {
    let manifest = manifest();
    for path in [
        "crates/julie-pipeline/src/resolver.rs",
        "crates/julie-pipeline/src/resolver/pending.rs",
        "crates/julie-pipeline/src/finalize.rs",
    ] {
        let selection = select_changed_buckets(&manifest, &[path.to_string()]);
        assert_eq!(
            selection.mode,
            expected_mapped_mode(&manifest, &selection.bucket_names),
            "path={path}"
        );
        assert_eq!(
            selection.bucket_names,
            vec![
                "core-pipeline",
                "tools-workspace-discovery",
                "tools-workspace-indexing",
                "tools-workspace-management",
                "workspace-init",
                "integration"
            ],
            "path={path} rationale={:?}",
            selection.rationale
        );
    }
}

#[test]
fn changed_tests_route_pipeline_catch_all_to_core_pipeline_only() {
    let manifest = manifest();
    let selection =
        select_changed_buckets(&manifest, &["crates/julie-pipeline/src/lib.rs".to_string()]);

    assert_eq!(
        selection.mode,
        expected_mapped_mode(&manifest, &selection.bucket_names)
    );
    assert_eq!(
        selection.bucket_names,
        vec!["core-pipeline"],
        "rationale={:?}",
        selection.rationale
    );
}

#[test]
fn changed_tests_route_cli_no_target_regression_to_cli_bucket() {
    let manifest = manifest();
    let selection = select_changed_buckets(
        &manifest,
        &["src/tests/cli/cli_search_no_target_test.rs".to_string()],
    );

    assert_eq!(
        selection.mode,
        expected_mapped_mode(&manifest, &selection.bucket_names)
    );
    assert_eq!(selection.bucket_names, vec!["cli"]);
}

#[test]
fn changed_tests_cli_bucket_runs_cli_no_target_regression() {
    let manifest = manifest();
    let commands = bucket_commands(&manifest, "cli");

    assert!(
        commands
            .iter()
            .any(|command| command.contains("tests::cli::cli_search_no_target_test")),
        "cli bucket must run non-ignored --target parser regression; commands={commands:?}"
    );
}

#[test]
fn changed_tests_route_handler_telemetry_to_dedicated_bucket() {
    let manifest = manifest();
    let selection = select_changed_buckets(
        &manifest,
        &["src/tests/core/handler_telemetry.rs".to_string()],
    );

    assert_eq!(
        selection.mode,
        expected_mapped_mode(&manifest, &selection.bucket_names)
    );
    assert_eq!(selection.bucket_names, vec!["core-handler-telemetry"]);
}

#[test]
fn changed_tests_system_health_selection_prices_at_declared_seconds() {
    let manifest = manifest();
    let selection = select_changed_buckets(&manifest, &["src/health/report.rs".to_string()]);

    assert_eq!(
        selection.mode,
        expected_mapped_mode(&manifest, &selection.bucket_names)
    );
    assert_eq!(selection.bucket_names, vec!["system-health".to_string()]);
    assert_eq!(
        crate::runner::declared_expected_seconds(
            &manifest,
            selection.bucket_names.iter().map(String::as_str),
        ),
        30,
        "system-health selection must price at declared 30s via shared resolver"
    );
}

#[test]
fn changed_tests_handler_telemetry_bucket_runs_module() {
    let manifest = manifest();
    let commands = bucket_commands(&manifest, "core-handler-telemetry");

    assert!(
        commands
            .iter()
            .any(|command| command.contains("tests::core::handler_telemetry")),
        "core-handler-telemetry bucket must run handler telemetry tests; commands={commands:?}"
    );
}

#[test]
fn changed_tests_full_search_buckets_cover_declared_search_modules() {
    let manifest = manifest();
    let full_buckets = manifest.tiers.get("full").expect("full tier exists");
    let full_search_commands: Vec<&String> = full_buckets
        .iter()
        .filter(|bucket| bucket.starts_with("tools-search-"))
        .flat_map(|bucket| bucket_commands(&manifest, bucket))
        .collect();
    let modules = declared_search_modules(&repo_root().join("src/tests/tools/search/mod.rs"));

    let uncovered: Vec<String> = modules
        .into_iter()
        .filter(|module| {
            !full_search_commands
                .iter()
                .any(|command| command_covers_module(command, module))
        })
        .collect();

    assert!(
        uncovered.is_empty(),
        "declared search modules must be covered by full search bucket commands; uncovered={uncovered:?}"
    );
}
