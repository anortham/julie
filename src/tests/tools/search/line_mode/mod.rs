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
    snapshot
        .store
        .rebuild_tantivy_if_needed(
            &julie_core::workspace::mutation_gate::acquire_gate(&snapshot.binding.workspace_id)
                .await,
        )
        .expect("tantivy current");
    mark_index_ready(handler).await;
}
