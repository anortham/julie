//! Embedding helpers for workspace indexing.
//!
//! Spawns the embedding pipeline for any registered workspace (primary or
//! reference) using the active embedding provider and a fresh database
//! connection.

use std::sync::{Arc, Mutex};

use tracing::{debug, info, warn};

use crate::database::SymbolDatabase;
use crate::embeddings::pipeline::run_embedding_pipeline;
use crate::handler::JulieServerHandler;

/// Spawn the embedding pipeline for a workspace (fire-and-forget).
///
/// Returns the symbol count so the caller can include it in response messages.
/// Returns 0 if embedding is skipped (no provider, no workspace, etc.).
pub(crate) async fn spawn_workspace_embedding(
    handler: &JulieServerHandler,
    workspace_id: String,
) -> usize {
    let workspace = match handler.get_workspace().await {
        Ok(Some(ws)) => ws,
        _ => return 0,
    };

    let provider = match &workspace.embedding_provider {
        Some(p) => p.clone(),
        None => {
            debug!("No embedding provider, skipping workspace embedding");
            return 0;
        }
    };

    let db_path = workspace.workspace_db_path(&workspace_id);
    if !db_path.exists() {
        warn!("Reference workspace DB not found at {}", db_path.display());
        return 0;
    }

    // Open a fresh database connection in a blocking context
    let db = match tokio::task::spawn_blocking({
        let path = db_path.clone();
        move || SymbolDatabase::new(path)
    })
    .await
    {
        Ok(Ok(db)) => db,
        Ok(Err(e)) => {
            warn!("Failed to open workspace DB for embedding: {e}");
            return 0;
        }
        Err(e) => {
            warn!("Workspace DB open task panicked: {e}");
            return 0;
        }
    };

    // Get symbol count before wrapping in Arc<Mutex> and spawning
    let total_symbols = db
        .get_stats()
        .map(|s| s.total_symbols as usize)
        .unwrap_or(0);

    let db_arc = Arc::new(Mutex::new(db));

    // Fire-and-forget: spawn the pipeline in the background
    tokio::spawn(async move {
        info!("Starting workspace embedding for {workspace_id} ({total_symbols} symbols)...");
        let db_clone = db_arc.clone();
        let result = tokio::task::spawn_blocking(move || {
            run_embedding_pipeline(&db_clone, provider.as_ref())
        })
        .await;

        match result {
            Ok(Ok(stats)) => {
                info!(
                    "Workspace {workspace_id} embedding complete: {}/{} symbols embedded ({} skipped)",
                    stats.symbols_embedded, stats.symbols_scanned, stats.symbols_skipped
                );
            }
            Ok(Err(e)) => {
                warn!("Workspace {workspace_id} embedding failed: {e}");
            }
            Err(e) => {
                warn!("Workspace {workspace_id} embedding task panicked: {e}");
            }
        }
    });

    total_symbols
}

/// Backward-compatible wrapper kept for call sites that are reference-specific.
pub(crate) async fn spawn_reference_embedding(
    handler: &JulieServerHandler,
    workspace_id: String,
) -> usize {
    spawn_workspace_embedding(handler, workspace_id).await
}
