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
    "src/migration.rs",
    "src/startup.rs",
    "src/tests/mod.rs",
    "src/tests/migration.rs",
    "src/tests/test_utils.rs",
];

const DEV_FALLBACK_PREFIXES: &[&str] = &[
    "crates/",
    "fixtures/",
    "src/analysis/",
    "src/extractors/",
    "src/handler/",
    "src/tests/fixtures/",
    "src/tests/helpers/",
];

const SEARCH_TOOL_BUCKETS: &[&str] = &[
    "tools-search-tantivy",
    "tools-search-line-file",
    "tools-search-ranking-format",
    "tools-search-context",
    "tools-search-text",
    "tools-search-hybrid",
    "tools-search-query",
];

const SEARCH_TOOL_BUCKETS_WITH_QUALITY: &[&str] = &[
    "tools-search-tantivy",
    "tools-search-line-file",
    "tools-search-ranking-format",
    "tools-search-context",
    "tools-search-text",
    "tools-search-hybrid",
    "tools-search-query",
    "search-quality",
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
        return ChangedSelection {
            mode: ChangedSelectionMode::FallbackToDev,
            changed_paths,
            bucket_names: manifest.tiers.get("dev").cloned().unwrap_or_default(),
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

    if matches_prefix(path, &["src/database/"])
        || matches_exact(
            path,
            &[
                "src/tests/core/database.rs",
                "src/tests/core/database_lightweight_query.rs",
            ],
        )
    {
        return &["core-database"];
    }

    if matches_prefix(path, &["src/tools/get_context/"])
        || matches_prefix(path, &["src/tests/tools/get_context"])
    {
        return &["tools-get-context"];
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

    if matches_prefix(path, &["src/adapter/", "src/tests/adapter/"])
        || matches_exact(
            path,
            &[
                "src/daemon/ipc.rs",
                "src/daemon/ipc_session.rs",
                "src/daemon/http_transport.rs",
                "src/daemon/transport.rs",
                "src/tests/daemon/ipc.rs",
                "src/tests/daemon/ipc_headers.rs",
                "src/tests/daemon/ipc_session.rs",
                "src/tests/daemon/http_transport.rs",
                "src/tests/daemon/transport.rs",
            ],
        )
    {
        return &["transport"];
    }

    if matches_exact(
        path,
        &[
            "src/daemon/mod.rs",
            "src/daemon/lifecycle.rs",
            "src/tests/daemon/lifecycle.rs",
            "src/tests/integration/daemon_lifecycle.rs",
        ],
    ) {
        if path == "src/daemon/mod.rs" {
            return &["lifecycle", "daemon"];
        }
        return &["lifecycle"];
    }

    if matches_exact(
        path,
        &[
            "src/daemon/workspace_pool.rs",
            "src/daemon/workspace_registry_store.rs",
            "src/daemon/workspace_session_attachment.rs",
            "src/daemon/watcher_pool.rs",
            "src/daemon/workspace_cleanup.rs",
            "src/workspace/registry.rs",
            "src/tests/daemon/workspace_pool.rs",
            "src/tests/daemon/watcher_pool.rs",
            "src/tests/daemon/workspace_cleanup.rs",
            "src/tests/tools/workspace/registry.rs",
        ],
    ) || matches_prefix(path, &["src/tools/workspace/commands/registry/"])
    {
        return &["workspace-runtime"];
    }

    if matches_prefix(path, &["src/tools/workspace/", "src/workspace/"]) {
        return &["tools-workspace", "workspace-init"];
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

    if matches_prefix(
        path,
        &[
            "src/tools/deep_dive/",
            "src/tools/impact/",
            "src/tools/navigation/",
            "src/tools/spillover/",
        ],
    ) || matches_exact(
        path,
        &[
            "src/tests/tools/deep_dive_primary_rebind_tests.rs",
            "src/tests/tools/deep_dive_regression_tests.rs",
            "src/tests/tools/deep_dive_tests.rs",
            "src/tests/tools/fast_refs_primary_rebind_tests.rs",
            "src/tests/tools/target_workspace_fast_refs_tests.rs",
            "src/tests/tools/call_path_tests.rs",
            "src/tests/tools/call_path_disambiguation_tests.rs",
            "src/tests/tools/spillover_tests.rs",
        ],
    ) || path.starts_with("src/tests/tools/blast_radius")
    {
        return &["tools-navigation"];
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

    if matches_exact(
        path,
        &[
            "src/tests/tools/filtering_tests.rs",
            "src/tests/tools/formatting_tests.rs",
            "src/tests/tools/query_classification_tests.rs",
            "src/tests/tools/phase4_token_savings.rs",
            "src/tests/tools/smart_read.rs",
        ],
    ) {
        return &["tools-format-filter"];
    }

    if matches_prefix(path, &["src/watcher/", "src/utils/", "src/tracing/"])
        || matches_exact(
            path,
            &[
                "src/language.rs",
                "src/paths.rs",
                "src/tests/main_error_handling.rs",
                "src/tests/regression_prevention_tests.rs",
                "src/tests/core/handler.rs",
                "src/tests/core/language.rs",
                "src/tests/core/memory_vectors.rs",
                "src/tests/core/paths.rs",
                "src/tests/core/tracing.rs",
                "src/tests/core/vector_storage.rs",
            ],
        )
        || matches_prefix(path, &["src/tests/utils/"])
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

    if matches_exact(path, &["src/tests/core/workspace_init.rs"]) {
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

    if matches_prefix(
        path,
        &[
            "src/tests/tools/search/line_",
            "src/tests/tools/search/file_",
        ],
    ) || matches_exact(
        path,
        &[
            "src/tests/tools/search/primary_workspace_bug.rs",
            "src/tests/tools/search/zero_hit_reason_tests.rs",
            "src/tests/tools/search/zero_hit_reason_propagation_tests.rs",
        ],
    ) {
        return Some(&["tools-search-line-file"]);
    }

    if matches_exact(
        path,
        &[
            "src/tests/tools/search/annotation_search_tests.rs",
            "src/tests/tools/search/content_scoring_tests.rs",
            "src/tests/tools/search/definition_overfetch_tests.rs",
            "src/tests/tools/search/definition_promotion_tests.rs",
            "src/tests/tools/search/fast_search_regression_tests.rs",
            "src/tests/tools/search/lean_format_tests.rs",
            "src/tests/tools/search/promotion_tests.rs",
            "src/tests/tools/search/quality.rs",
            "src/tests/tools/search/race_condition.rs",
        ],
    ) {
        return Some(&["tools-search-ranking-format"]);
    }

    if matches_exact(path, &["src/tests/tools/search_context_lines.rs"]) {
        return Some(&["tools-search-context"]);
    }

    if matches_exact(path, &["src/tests/tools/text_search_tantivy.rs"]) {
        return Some(&["tools-search-text"]);
    }

    if matches_exact(path, &["src/tests/tools/hybrid_search_tests.rs"]) {
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
        "projection",
        "tools-get-context",
        "tools-search-tantivy",
        "tools-search-line-file",
        "tools-search-ranking-format",
        "tools-search-context",
        "tools-search-text",
        "tools-search-hybrid",
        "tools-search-query",
        "tools-workspace",
        "tools-get-symbols",
        "tools-editing",
        "tools-navigation",
        "tools-refactoring",
        "tools-metrics",
        "tools-format-filter",
        "core-fast",
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
