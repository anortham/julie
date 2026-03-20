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
///
/// If the embedding provider has not been initialized yet (deferred from
/// workspace startup to avoid blocking indexing), this function initializes
/// it here - right before the first embedding run.
pub(crate) async fn spawn_workspace_embedding(
    handler: &JulieServerHandler,
    workspace_id: String,
) -> usize {
    // Check if provider is already initialized (fast read lock path)
    let provider = {
        let ws_guard = handler.workspace.read().await;
        ws_guard
            .as_ref()
            .and_then(|ws| ws.embedding_provider.clone())
    };

    let provider = match provider {
        Some(p) => p,
        None => {
            // Provider not yet initialized - do it now (deferred from workspace init
            // to avoid blocking symbol extraction and Tantivy indexing).
            info!("Initializing embedding provider (deferred from workspace startup)...");

            let (workspace_identity_root, workspace_for_init) = {
                let ws_guard = handler.workspace.read().await;
                match ws_guard.as_ref() {
                    Some(ws) => (ws.root.clone(), ws.clone()),
                    None => return 0,
                }
            };

            // Run heavy provider initialization off runtime worker threads.
            let init_result = tokio::task::spawn_blocking(move || {
                let mut workspace = workspace_for_init;
                workspace.initialize_embedding_provider();
                (
                    workspace.embedding_provider.clone(),
                    workspace.embedding_runtime_status.clone(),
                )
            })
            .await;

            let (initialized_provider, initialized_runtime_status) = match init_result {
                Ok(result) => result,
                Err(e) => {
                    warn!("Embedding provider init task panicked: {e}");
                    return 0;
                }
            };

            // Publish initialized state with short write-lock scope.
            let mut ws_guard = handler.workspace.write().await;
            let ws = match ws_guard.as_mut() {
                Some(ws) => ws,
                None => return 0,
            };

            if ws.root != workspace_identity_root {
                debug!(
                    expected_workspace_root = %workspace_identity_root.display(),
                    active_workspace_root = %ws.root.display(),
                    "Discarding stale embedding init result after workspace switch"
                );
            } else if ws.embedding_provider.is_none() {
                ws.embedding_provider = initialized_provider.clone();
                ws.embedding_runtime_status = initialized_runtime_status;
            }

            match ws.embedding_provider.clone() {
                Some(provider) => provider,
                None => {
                    debug!(
                        "Embedding provider unavailable after init, skipping workspace embedding"
                    );
                    return 0;
                }
            }
        }
    };

    let db_path = {
        let ws_guard = handler.workspace.read().await;
        match ws_guard.as_ref() {
            Some(ws) => ws.workspace_db_path(&workspace_id),
            None => return 0,
        }
    };
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
        let lang_configs = crate::search::language_config::LanguageConfigs::load_embedded();
        let result = tokio::task::spawn_blocking(move || {
            run_embedding_pipeline(&db_clone, provider.as_ref(), Some(&lang_configs))
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
                warn!("Workspace {workspace_id} embedding failed: {e:#}");
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
