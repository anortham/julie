//! Handler for file delete and rename events.

use super::FileIndexOutcome;
use anyhow::Result;
use julie_core::workspace::mutation_gate::MutationGuard;
use julie_index::checkout_store::CheckoutStore;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Store writes happen in the queue processor via `CheckoutStore::apply`.
pub async fn handle_file_deleted_static(
    _path: PathBuf,
    _store: &Arc<CheckoutStore>,
    _workspace_root: &Path,
    _guard: &MutationGuard<'_>,
) -> Result<()> {
    Ok(())
}

pub(crate) async fn handle_file_renamed_static(
    _from: PathBuf,
    _to: PathBuf,
    _store: &Arc<CheckoutStore>,
    _workspace_root: &Path,
    _guard: &MutationGuard<'_>,
) -> Result<FileIndexOutcome> {
    Ok(FileIndexOutcome::clean())
}
