use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::manifest::TestManifest;

const DEV_FALLBACK_FILES: &[&str] = &[
    "Cargo.toml",
    "Cargo.lock",
    "src/handler.rs",
    "src/lib.rs",
    "src/main.rs",
    "src/tests/mod.rs",
    "src/tests/test_utils.rs",
];

const DEV_FALLBACK_PREFIXES: &[&str] = &[
    "fixtures/",
    "src/tests/fixtures/",
    "src/tests/helpers/",
];

const SEARCH_TOOL_BUCKETS: &[&str] = &[
    "tools-search-tantivy",
    "tools-search-line",
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
    "tools-search-line",
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
    "tools-search-line",
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
    "tools-search-line",
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
    "tools-workspace",
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangedSelectionMode {
    NoChanges,
    Buckets,
    FallbackToDev,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedSelection {
    pub mode: ChangedSelectionMode,
    pub changed_paths: Vec<String>,
    pub bucket_names: Vec<String>,
    pub fallback_paths: Vec<String>,
    pub rationale: Vec<String>,
    pub ignored_paths: Vec<String>,
}

pub fn collect_changed_paths(workspace_root: &Path) -> Result<Vec<String>> {
    if let Some(paths) = std::env::var_os("XTASK_CHANGED_PATHS") {
        return Ok(normalize_paths(
            paths
                .to_string_lossy()
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(ToOwned::to_owned)
                .collect(),
        ));
    }

    let tracked_paths = if has_head(workspace_root)? {
        git_lines(
            workspace_root,
            &["diff", "--name-only", "--relative", "HEAD", "--"],
        )?
    } else {
        let mut paths = git_lines(workspace_root, &["diff", "--name-only", "--relative", "--"])?;
        paths.extend(git_lines(
            workspace_root,
            &["diff", "--name-only", "--relative", "--cached", "--"],
        )?);
        paths
    };
    let untracked_paths = git_lines(
        workspace_root,
        &["ls-files", "--others", "--exclude-standard"],
    )?;

    Ok(normalize_paths(
        tracked_paths
            .into_iter()
            .chain(untracked_paths)
            .collect::<Vec<_>>(),
    ))
}

pub fn select_changed_buckets(manifest: &TestManifest, paths: &[String]) -> ChangedSelection {
    let changed_paths = normalize_paths(paths.iter().cloned().collect());
    let mut bucket_names = Vec::new();
    let mut fallback_paths = Vec::new();
    let mut rationale = Vec::new();
    let mut ignored_paths = Vec::new();

    for path in &changed_paths {
        if should_ignore(path) {
            ignored_paths.push(path.clone());
            continue;
        }

        if let Some((rule, trigger)) = fallback_rule_for_path(path) {
            fallback_paths.push(path.clone());
            rationale.push(render_fallback_rationale(path, rule, &trigger));
            continue;
        }

        let matched_buckets = buckets_for_path(path);
        if matched_buckets.is_empty() {
            fallback_paths.push(path.clone());
            rationale.push(render_fallback_rationale(
                path,
                FallbackRule::Unknown,
                "no bucket mapping matched this path",
            ));
            continue;
        }

        let mut path_bucket_names = Vec::new();
        for bucket_name in matched_buckets {
            if manifest.buckets.contains_key(*bucket_name) || *bucket_name == "system-health" {
                path_bucket_names.push((*bucket_name).to_string());
            }
            maybe_push_bucket(&mut bucket_names, manifest, bucket_name);
        }

        if path_bucket_names.is_empty() {
            fallback_paths.push(path.clone());
            rationale.push(render_fallback_rationale(
                path,
                FallbackRule::ManifestLevel,
                &format!(
                    "mapped buckets missing from manifest: {}",
                    matched_buckets.join(", ")
                ),
            ));
            continue;
        }

        rationale.push(format!(
            "CHANGED: rationale: {} -> {}",
            path,
            path_bucket_names.join(", ")
        ));
    }

    if !fallback_paths.is_empty() {
        let mut dev_buckets = manifest.tiers.get("dev").cloned().unwrap_or_default();
        if bucket_names.is_empty() {
            return ChangedSelection {
                mode: ChangedSelectionMode::FallbackToDev,
                changed_paths,
                bucket_names: dev_buckets,
                fallback_paths,
                rationale,
                ignored_paths,
            };
        }

        for dev_bucket in dev_buckets.drain(..) {
            maybe_push_bucket(&mut bucket_names, manifest, &dev_bucket);
        }

        return ChangedSelection {
            mode: ChangedSelectionMode::FallbackToDev,
            changed_paths,
            bucket_names: sort_bucket_names(bucket_names),
            fallback_paths,
            rationale,
            ignored_paths,
        };
    }

    if bucket_names.is_empty() {
        return ChangedSelection {
            mode: ChangedSelectionMode::NoChanges,
            changed_paths,
            bucket_names,
            fallback_paths,
            rationale,
            ignored_paths,
        };
    }

    ChangedSelection {
        mode: ChangedSelectionMode::Buckets,
        changed_paths,
        bucket_names: sort_bucket_names(bucket_names),
        fallback_paths,
        rationale,
        ignored_paths,
    }
}

pub fn render_changed_selection(selection: &ChangedSelection) -> String {
    let mut output = match selection.mode {
        ChangedSelectionMode::NoChanges => {
            "CHANGED: no code/test buckets matched local changes\n".to_string()
        }
        ChangedSelectionMode::Buckets => format!(
            "CHANGED: selected buckets from local diff: {}\n",
            selection.bucket_names.join(", ")
        ),
        ChangedSelectionMode::FallbackToDev => format!(
            "CHANGED: shared or unmapped paths hit the diff, falling back to dev: {}\n",
            selection.fallback_paths.join(", ")
        ),
    };

    for line in &selection.rationale {
        output.push_str(line);
        output.push('\n');
    }

    if !selection.ignored_paths.is_empty() {
        output.push_str(&format!(
            "CHANGED: ignored non-executable paths: {}\n",
            selection.ignored_paths.join(", ")
        ));
    }

    output
}

fn has_head(workspace_root: &Path) -> Result<bool> {
    let output = Command::new("git")
        .args(["rev-parse", "--verify", "HEAD"])
        .current_dir(workspace_root)
        .output()
        .with_context(|| format!("failed to check git HEAD in {}", workspace_root.display()))?;
    Ok(output.status.success())
}

fn git_lines(workspace_root: &Path, args: &[&str]) -> Result<Vec<String>> {
    let output = Command::new("git")
        .args(args)
        .current_dir(workspace_root)
        .output()
        .with_context(|| {
            format!(
                "failed to run git {:?} in {}",
                args,
                workspace_root.display()
            )
        })?;

    if !output.status.success() {
        bail!(
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}

fn normalize_paths(paths: Vec<String>) -> Vec<String> {
    paths
        .into_iter()
        .map(|path| normalize_path(&path))
        .filter(|path| !path.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn normalize_path(path: &str) -> String {
    path.trim()
        .trim_start_matches("./")
        .replace('\\', "/")
        .trim()
        .to_string()
}

fn should_ignore(path: &str) -> bool {
    if matches_exact(
        path,
        &[
            "docs/TESTING_GUIDE.md",
            "docs/plans/verification-ledger-template.md",
        ],
    ) {
        return false;
    }

    matches_prefix(path, &[".julie/", ".memories/", "docs/"])
        || matches_exact(path, &[".DS_Store"])
        || path.starts_with("target/")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FallbackRule {
    ExactFile,
    Prefix,
    ManifestLevel,
    Unknown,
}

fn fallback_rule_for_path(path: &str) -> Option<(FallbackRule, String)> {
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

fn render_fallback_rationale(path: &str, rule: FallbackRule, trigger: &str) -> String {
    match rule {
        FallbackRule::ExactFile => format!(
            "CHANGED: rationale: {} -> dev (fallback exact file: {})",
            path, trigger
        ),
        FallbackRule::Prefix => format!(
            "CHANGED: rationale: {} -> dev (fallback prefix: {})",
            path, trigger
        ),
        FallbackRule::ManifestLevel => format!(
            "CHANGED: rationale: {} -> dev (fallback manifest-level: {})",
            path, trigger
        ),
        FallbackRule::Unknown => format!(
            "CHANGED: rationale: {} -> dev (fallback unknown: {})",
            path, trigger
        ),
    }
}

fn buckets_for_path(path: &str) -> &'static [&'static str] {
    if matches_prefix(path, &["xtask/"]) {
        return &["xtask-runner"];
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
        return &["daemon"];
    }
    if path == "src/handler/tool_metrics.rs" {
        return &["tools-metrics", "daemon"];
    }
    if path == "src/handler/tool_targets.rs" {
        return &["tools-workspace", "tools-workspace-targeting", "daemon"];
    }
    if path == "src/handler/tools/mod.rs" {
        // Pure module declaration file. Re-routes to daemon (handler trait surface);
        // any added tool also touches its dedicated handler/tools/<tool>.rs file which
        // pulls the right bucket independently.
        return &["daemon"];
    }

    // Migration and startup routing — both touch DaemonDatabase, workspace registry,
    // and indexing; they no longer need to force the full dev tier.
    if matches_exact(path, &["src/migration.rs", "src/tests/migration.rs"]) {
        return &["core-database", "workspace-init"];
    }
    if path == "src/startup.rs" {
        return &["lifecycle", "workspace-runtime", "tools-workspace"];
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
    // (the pre-split mappings were src/daemon/* -> daemon, src/embeddings/* ->
    // core-embeddings, src/utils/* -> core-fast) instead of silently running only the
    // DB slice. Specific files must precede the catch-all prefix (first match wins).
    if path == "crates/julie-core/src/connection_pool.rs" {
        return &["core-database", "daemon"];
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
    //   crates/julie-pipeline/src/indexing_core/** -> core-pipeline + workspace-init + integration + tools-workspace
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

    if matches_exact(path, &["src/daemon/mod.rs", "src/daemon/lifecycle.rs"]) {
        return &["daemon"];
    }

    if matches_exact(
        path,
        &[
            "src/daemon/workspace_registry_store.rs",
            "src/daemon/workspace_session_attachment.rs",
            "src/daemon/workspace_cleanup.rs",
            "src/workspace/registry.rs",
            "src/tests/daemon/workspace_cleanup.rs",
            "src/tests/tools/workspace/registry.rs",
        ],
    ) || matches_prefix(path, &["src/tools/workspace/commands/registry/"])
    {
        return &["workspace-runtime"];
    }

    // Note: "src/workspace/" removed — workspace moved to crates/julie-runtime (T2c.2);
    // edits to the top-crate re-export shim at src/workspace/ now route via the
    // crates/julie-runtime/src/workspace/ arm above when editing the real source.
    if matches_prefix(path, &["src/tools/workspace/"]) {
        return &[
            "tools-workspace",
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

    if matches_prefix(path, &["src/tests/tools/workspace/"]) {
        return &["tools-workspace"];
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
        &[
            "src/utils/",
            "src/tracing/",
            "src/tests/core/handler/",
        ],
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

    if matches_prefix(path, &["src/daemon/", "src/tests/daemon/"]) {
        return &["daemon"];
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
            "tools-workspace",
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

fn get_context_test_buckets_for_path(path: &str) -> Option<&'static [&'static str]> {
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

fn search_test_buckets_for_path(path: &str) -> Option<&'static [&'static str]> {
    if matches_exact(path, &["src/tests/tools/search/mod.rs"]) {
        return Some(SEARCH_TOOL_BUCKETS);
    }

    if matches_prefix(path, &["src/tests/tools/search/tantivy_"]) {
        return Some(&["tools-search-tantivy"]);
    }

    if matches_prefix(path, &["src/tests/tools/search/line_"]) {
        return Some(&["tools-search-line"]);
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

    if matches_exact(path, &["src/tests/tools/text_search_tantivy.rs"]) {
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

fn maybe_push_bucket(bucket_names: &mut Vec<String>, manifest: &TestManifest, bucket_name: &str) {
    if !manifest.buckets.contains_key(bucket_name) && bucket_name != "system-health" {
        return;
    }

    if bucket_names.iter().any(|existing| existing == bucket_name) {
        return;
    }

    bucket_names.push(bucket_name.to_string());
}

fn sort_bucket_names(bucket_names: Vec<String>) -> Vec<String> {
    let order = [
        "cli",
        "xtask-runner",
        "core-database",
        "core-embeddings",
        "core-index",
        "core-pipeline",
        "core-runtime",
        "extractor-dep-integration",
        "projection",
        "tools-get-context-pipeline",
        "tools-get-context-format",
        "tools-get-context-graph",
        "tools-search-tantivy",
        "tools-search-line",
        "tools-search-file-mode",
        "tools-search-zero-hit",
        "tools-search-promotion",
        "tools-search-format-quality",
        "tools-search-context",
        "tools-search-text",
        "tools-search-hybrid",
        "tools-search-query",
        "tools-search-unified",
        "tools-workspace",
        "tools-workspace-targeting",
        "tools-get-symbols",
        "tools-editing",
        "tools-deep-dive",
        "tools-call-path",
        "tools-fast-refs",
        "tools-blast-spillover",
        "tools-refactoring",
        "tools-metrics",
        "tools-format-filter",
        "core-fast",
        "core-handler-telemetry",
        "transport",
        "lifecycle",
        "workspace-runtime",
        "daemon",
        "dashboard",
        "tools-dogfood-repo-index",
        "workspace-init",
        "integration",
        "search-quality",
        "system-health",
    ];

    let mut sorted = bucket_names;
    sorted.sort_by_key(|bucket_name| {
        order
            .iter()
            .position(|candidate| bucket_name == candidate)
            .unwrap_or(order.len())
    });
    sorted
}

fn matches_exact(path: &str, candidates: &[&str]) -> bool {
    candidates.iter().any(|candidate| path == *candidate)
}

fn matches_prefix(path: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|prefix| path.starts_with(prefix))
}

#[cfg(test)]
mod tests {
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

        assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
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

        assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
        assert_eq!(selection.bucket_names, vec!["tools-workspace-targeting"]);
        assert!(
            selection.fallback_paths.is_empty(),
            "split global targeting modules should not route through the broad workspace bucket; rationale={:?}",
            selection.rationale
        );
    }

    #[test]
    fn changed_tests_route_target_workspace_fast_refs_split_modules_to_fast_refs_bucket() {
        let manifest = manifest();
        let selection = select_changed_buckets(
            &manifest,
            &["src/tests/tools/target_workspace_fast_refs_tests/tests/limits.rs".to_string()],
        );

        assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
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

        assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
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

        assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
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

        assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
        // indexing engine: crate unit tests (core-pipeline) + the retained
        // end-to-end indexing guards (R6 co-targeting).
        assert_eq!(
            selection.bucket_names,
            vec!["core-pipeline", "tools-workspace", "workspace-init", "integration"],
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
            assert_eq!(selection.mode, ChangedSelectionMode::Buckets, "path={path}");
            assert_eq!(
                selection.bucket_names,
                vec!["core-pipeline", "tools-workspace", "workspace-init", "integration"],
                "path={path} rationale={:?}",
                selection.rationale
            );
        }
    }

    #[test]
    fn changed_tests_route_pipeline_catch_all_to_core_pipeline_only() {
        let manifest = manifest();
        let selection = select_changed_buckets(
            &manifest,
            &["crates/julie-pipeline/src/lib.rs".to_string()],
        );

        assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
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

        assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
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

        assert_eq!(selection.mode, ChangedSelectionMode::Buckets);
        assert_eq!(selection.bucket_names, vec!["core-handler-telemetry"]);
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
    fn changed_tests_dev_search_buckets_cover_declared_search_modules() {
        let manifest = manifest();
        let dev_buckets = manifest.tiers.get("dev").expect("dev tier exists");
        let dev_search_commands: Vec<&String> = dev_buckets
            .iter()
            .filter(|bucket| bucket.starts_with("tools-search-"))
            .flat_map(|bucket| bucket_commands(&manifest, bucket))
            .collect();
        let modules = declared_search_modules(&repo_root().join("src/tests/tools/search/mod.rs"));

        let uncovered: Vec<String> = modules
            .into_iter()
            .filter(|module| {
                !dev_search_commands
                    .iter()
                    .any(|command| command_covers_module(command, module))
            })
            .collect();

        assert!(
            uncovered.is_empty(),
            "declared search modules must be covered by dev search bucket commands; uncovered={uncovered:?}"
        );
    }
}
