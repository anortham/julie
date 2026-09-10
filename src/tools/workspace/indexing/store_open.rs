//! Open the checkout store. A version mismatch or a failed open deletes
//! `indexes/<id>/` and opens a new empty one; facts are never migrated.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use julie_index::checkout_store::{CheckoutStore, FACTS_FILE};
use tracing::warn;

use crate::handler::JulieServerHandler;

/// `<index_root>` — facts.sqlite and the Tantivy projection live here.
pub(crate) fn store_dir(index_root: &Path) -> PathBuf {
    index_root.to_path_buf()
}

/// Open `<store_dir>` (facts.sqlite + tantivy). On any open failure, delete
/// the directory and create a new store in its place.
pub(crate) fn open_or_recreate(store_dir: &Path, root: &Path) -> Result<CheckoutStore> {
    match CheckoutStore::open(store_dir, root) {
        Ok(store) => Ok(store),
        Err(err) => {
            warn!(
                error = %err,
                store_dir = %store_dir.display(),
                "checkout store open failed; deleting indexes/<id>/ and reopening"
            );
            delete_store_dir(store_dir)?;
            CheckoutStore::open(store_dir, root)
        }
    }
}

pub(crate) fn delete_store_dir(store_dir: &Path) -> Result<()> {
    if store_dir.exists() {
        std::fs::remove_dir_all(store_dir)?;
    }
    Ok(())
}

/// The writer for this checkout: the loaded primary's store, or one opened
/// from `indexes/<id>/`. A failed open deletes `indexes/<id>/` and retries.
pub(crate) async fn store_for_workspace(
    handler: &JulieServerHandler,
    workspace_id: &str,
    root: &Path,
) -> Result<Arc<CheckoutStore>> {
    let store_dir = store_dir(&handler.workspace_index_dir_for(workspace_id).await?);
    if !store_dir.join(FACTS_FILE).exists() {
        let root = root.to_path_buf();
        let store =
            tokio::task::spawn_blocking(move || open_or_recreate(&store_dir, &root)).await??;
        return Ok(Arc::new(store));
    }
    if handler.loaded_workspace_id().as_deref() == Some(workspace_id) {
        if let Some(ws) = handler.get_workspace().await? {
            return Ok(ws.store);
        }
    }
    match handler
        .checkout_store_for_workspace(workspace_id, root)
        .await
    {
        Ok(store) => Ok(store),
        Err(err) => {
            warn!(
                workspace_id,
                error = %err,
                "checkout store open failed; deleting indexes/<id>/ and reopening"
            );
            let root = root.to_path_buf();
            let store =
                tokio::task::spawn_blocking(move || open_or_recreate(&store_dir, &root)).await??;
            Ok(Arc::new(store))
        }
    }
}
