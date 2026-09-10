//! Tests for fast_search line-level output mode.

mod basic;
mod filters;
mod primary_rebind;

use crate::handler::JulieServerHandler;
use std::sync::atomic::Ordering;

async fn mark_index_ready(handler: &JulieServerHandler) {
    handler
        .indexing_status
        .search_ready
        .store(true, Ordering::Relaxed);
    *handler.is_indexed.write().await = true;
}

async fn ensure_primary_projection_current(handler: &JulieServerHandler) {
    let snapshot = handler
        .primary_workspace_snapshot()
        .await
        .expect("primary snapshot");
    let search_index = snapshot.search_index.expect("primary search index");
    let mut db = snapshot
        .database
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let idx = search_index;
    crate::search::SearchProjection::tantivy(snapshot.binding.workspace_id)
        .ensure_current_with_gate(&mut db, &idx, &handler.indexing_status.search_ready)
        .expect("projection current");
}
