//! Handler for file created or modified events.

use super::FileIndexOutcome;
use anyhow::Result;
use julie_core::workspace::mutation_gate::MutationGuard;
use julie_index::checkout_store::CheckoutStore;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Store writes happen in the queue processor via `CheckoutStore::apply`.
pub async fn handle_file_created_or_modified_static(
    _path: PathBuf,
    _store: &Arc<CheckoutStore>,
    _workspace_root: &Path,
    _guard: &MutationGuard<'_>,
) -> Result<FileIndexOutcome> {
    Ok(FileIndexOutcome::clean())
}
