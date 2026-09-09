//! Test helpers for julie-runtime test suites.

use julie_core::workspace::mutation_gate::{MutationGuard, acquire_gate};
use julie_core::workspace::registry::generate_workspace_id;
use std::path::Path;

/// Acquire the workspace mutation gate for `workspace_root` in a test.
pub async fn test_mutation_guard(workspace_root: impl AsRef<Path>) -> MutationGuard<'static> {
    let root = workspace_root.as_ref();
    let _ = std::fs::create_dir_all(root);
    let workspace_key = root.to_string_lossy();
    let workspace_id =
        generate_workspace_id(&workspace_key).unwrap_or_else(|_| workspace_key.into_owned());
    acquire_gate(&workspace_id).await
}
