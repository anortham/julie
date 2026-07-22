use crate::manifest::TestManifest;
use crate::runner::declared_expected_seconds;

use super::diff::{normalize_paths, should_ignore};
use super::mapping::{FallbackRule, buckets_for_path, fallback_rule_for_path};
use super::rendering::render_fallback_rationale;
use super::{ChangedSelection, ChangedSelectionMode};

/// Declared-seconds ceiling for a mapped `changed` selection before OverBudget.
const FAST_BUDGET_SECS: u64 = 60;

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
            if manifest.buckets.contains_key(*bucket_name) {
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

    let bucket_names = sort_bucket_names(bucket_names);
    let declared = declared_expected_seconds(manifest, bucket_names.iter().map(String::as_str));
    if declared > FAST_BUDGET_SECS {
        let mut rationale = rationale;
        rationale.push(format!(
            "CHANGED: declared expected_seconds = {declared} (fast budget {FAST_BUDGET_SECS})"
        ));
        rationale.push("CHANGED: next: cargo xtask test changed --scale".to_string());
        rationale.push("CHANGED: next: cargo xtask test fast".to_string());
        return ChangedSelection {
            mode: ChangedSelectionMode::OverBudget,
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
        bucket_names,
        fallback_paths,
        rationale,
        ignored_paths,
    }
}

pub fn apply_changed_scale(
    mut selection: ChangedSelection,
    manifest: &TestManifest,
) -> ChangedSelection {
    if selection.mode != ChangedSelectionMode::OverBudget {
        return selection;
    }

    let mapped = selection.bucket_names.clone();
    let mut bucket_names = mapped.clone();
    for dev_bucket in manifest.tiers.get("dev").cloned().unwrap_or_default() {
        maybe_push_bucket(&mut bucket_names, manifest, &dev_bucket);
    }

    selection.bucket_names = sort_bucket_names(bucket_names);
    selection.mode = ChangedSelectionMode::Buckets;
    selection
        .rationale
        .retain(|line| line != "CHANGED: next: cargo xtask test changed --scale");
    selection.rationale.push(format!(
        "CHANGED: scale union: mapped ∪ dev (preserving {})",
        mapped.join(", ")
    ));
    selection
}

fn maybe_push_bucket(bucket_names: &mut Vec<String>, manifest: &TestManifest, bucket_name: &str) {
    if !manifest.buckets.contains_key(bucket_name) {
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
        "xtask-eval",
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
        "tools-workspace-discovery",
        "tools-workspace-indexing",
        "tools-workspace-management",
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
        "registry",
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
