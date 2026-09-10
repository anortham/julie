//! Apply a `PathChange` list to the checkout store. This is the only persist
//! step on the index path.

use std::sync::Arc;

use anyhow::Result;
use tracing::info;

use super::route::IndexRoute;
use super::store_open::store_for_workspace;
use crate::handler::JulieServerHandler;
use julie_core::workspace::mutation_gate::MutationGuard;
use julie_index::checkout_store::{Applied, CheckoutStore, PathChange};

pub(crate) async fn store_for_route(
    handler: &JulieServerHandler,
    route: &IndexRoute,
) -> Result<Arc<CheckoutStore>> {
    store_for_workspace(handler, &route.workspace_id, &route.workspace_root).await
}

pub(crate) fn apply_changes(
    store: &CheckoutStore,
    changes: &[PathChange],
    guard: &MutationGuard<'_>,
) -> Result<Applied> {
    if changes.is_empty() {
        return Ok(Applied::default());
    }
    let applied = store.apply(changes, guard)?;
    info!(
        new_blobs = applied.new_blobs,
        reused_blobs = applied.reused_blobs,
        removed_paths = applied.removed_paths,
        "Checkout store applied path changes"
    );
    Ok(applied)
}
