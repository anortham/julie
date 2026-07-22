mod diff;
mod mapping;
mod policy;
mod rendering;

#[cfg(test)]
mod tests;

pub use diff::collect_changed_paths;
pub use policy::{apply_changed_scale, select_changed_buckets};
pub use rendering::render_changed_selection;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangedSelectionMode {
    NoChanges,
    Buckets,
    OverBudget,
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
