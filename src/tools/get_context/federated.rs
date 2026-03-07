//! Federated get_context: cross-project pipeline fan-out and merge.
//!
//! When `workspace="all"` is passed to `get_context`, this module fans out
//! `run_pipeline` across all Ready workspaces in the daemon, then merges
//! results with project headers and a global token budget.

use anyhow::Result;
use tracing::debug;

use crate::database::SymbolDatabase;
use crate::handler::JulieServerHandler;

use super::pipeline::run_pipeline;

/// Derive a human-readable project name from a filesystem path.
///
/// Uses the last path component (directory name), which matches common
/// project naming conventions across all languages/layouts.
pub fn project_name_from_path(path: &std::path::Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            if path.as_os_str().is_empty() {
                "unknown".to_string()
            } else {
                "root".to_string()
            }
        })
}

/// Federated get_context: fan out `run_pipeline` across all Ready workspaces
/// in the daemon, then merge results with project headers.
///
/// Requires daemon mode (`handler.daemon_state` is `Some`). Returns an error
/// in stdio mode (single-workspace, no federation available).
pub(super) async fn run_federated(
    query: String,
    max_tokens: Option<u32>,
    language: Option<String>,
    file_pattern: Option<String>,
    format: Option<String>,
    handler: &JulieServerHandler,
) -> Result<String> {
    use crate::daemon_state::WorkspaceLoadStatus;

    // 1. Require daemon mode
    let daemon_state = handler
        .daemon_state
        .as_ref()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "get_context(workspace=\"all\") requires daemon mode. \
                 In stdio mode, only the primary workspace is available."
            )
        })?;

    // 2. Read-lock DaemonState, collect Ready workspaces
    //    We extract Arc<Mutex<...>> handles so we can drop the lock before blocking.
    struct WorkspaceHandle {
        project_name: String,
        db: std::sync::Arc<std::sync::Mutex<SymbolDatabase>>,
        search_index: std::sync::Arc<std::sync::Mutex<crate::search::SearchIndex>>,
    }

    let handles: Vec<WorkspaceHandle> = {
        let state = daemon_state.read().await;
        state
            .workspaces
            .iter()
            .filter(|(_, lw)| lw.status == WorkspaceLoadStatus::Ready)
            .filter_map(|(_, lw)| {
                let db = lw.workspace.db.as_ref()?.clone();
                let search_index = lw.workspace.search_index.as_ref()?.clone();
                let project_name = project_name_from_path(&lw.path);
                Some(WorkspaceHandle {
                    project_name,
                    db,
                    search_index,
                })
            })
            .collect()
    };
    // DaemonState read lock dropped here

    if handles.is_empty() {
        let output_format = super::formatting::OutputFormat::from_option(format.as_deref());
        return Ok(super::formatting::format_federated_context(
            &query,
            &[],
            output_format,
        ));
    }

    // 3. Compute per-project token budget
    let num_projects = handles.len() as u32;
    let per_project_tokens = max_tokens.map(|t| t / num_projects.max(1));

    // 4. Run pipelines in parallel (one spawn_blocking per workspace)
    let mut join_set = tokio::task::JoinSet::new();

    for handle in handles {
        let query = query.clone();
        let language = language.clone();
        let file_pattern = file_pattern.clone();
        let format = format.clone();

        join_set.spawn(async move {
            let project_name = handle.project_name.clone();
            let result = tokio::task::spawn_blocking(move || -> Result<String> {
                let db_guard = handle.db.lock().map_err(|e| {
                    anyhow::anyhow!("Failed to lock DB for {}: {}", handle.project_name, e)
                })?;
                let si_guard = handle.search_index.lock().map_err(|e| {
                    anyhow::anyhow!(
                        "Failed to lock SearchIndex for {}: {}",
                        handle.project_name,
                        e
                    )
                })?;
                run_pipeline(
                    &query,
                    per_project_tokens,
                    language,
                    file_pattern,
                    format,
                    &db_guard,
                    &si_guard,
                    None, // No embedding provider for federated search
                )
            })
            .await
            .map_err(|e| anyhow::anyhow!("spawn_blocking error: {}", e))??;

            Ok::<(String, String), anyhow::Error>((project_name, result))
        });
    }

    // 5. Collect results
    let mut per_project_results: Vec<(String, String)> = Vec::new();

    while let Some(join_result) = join_set.join_next().await {
        match join_result {
            Ok(Ok((project_name, output))) => {
                per_project_results.push((project_name, output));
            }
            Ok(Err(e)) => {
                debug!("Federated get_context: workspace pipeline failed: {e}");
            }
            Err(e) => {
                debug!("Federated get_context: task join failed: {e}");
            }
        }
    }

    // Sort by project name for deterministic output
    per_project_results.sort_by(|a, b| a.0.cmp(&b.0));

    // 6. Format merged output
    let output_format = super::formatting::OutputFormat::from_option(format.as_deref());
    Ok(super::formatting::format_federated_context(
        &query,
        &per_project_results,
        output_format,
    ))
}
