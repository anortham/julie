use super::mapping::FallbackRule;
use super::{ChangedSelection, ChangedSelectionMode};

pub fn render_changed_selection(selection: &ChangedSelection) -> String {
    let mut output = match selection.mode {
        ChangedSelectionMode::NoChanges => {
            "CHANGED: no code/test buckets matched local changes\n".to_string()
        }
        ChangedSelectionMode::Buckets => format!(
            "CHANGED: selected buckets from local diff: {}\n",
            selection.bucket_names.join(", ")
        ),
        ChangedSelectionMode::OverBudget => format!(
            "CHANGED: over budget — mapped buckets exceed fast budget: {}\n",
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

/// Escalate an [`ChangedSelectionMode::OverBudget`] selection by running
/// `unique(mapped ∪ dev)` with an explicit scale-union rationale.
pub(super) fn render_fallback_rationale(path: &str, rule: FallbackRule, trigger: &str) -> String {
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
