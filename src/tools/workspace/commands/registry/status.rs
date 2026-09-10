use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};

use super::{ManageWorkspaceTool, registry_store_for_handler};
use crate::handler::JulieServerHandler;
use crate::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use crate::workspace::registry::generate_workspace_id;

/// Facts about one checkout, computed on request from the registry row, the
/// index directory on disk, and the loaded workspace's watcher.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckoutStatus {
    pub workspace_id: String,
    pub root: String,
    pub root_exists: bool,
    pub watcher: String,
    pub last_file_event_at: Option<String>,
    pub file_count: i64,
    pub symbol_count: i64,
    pub db_bytes: u64,
    pub tantivy: String,
    pub tantivy_age_seconds: Option<u64>,
    pub vector_count: i64,
    pub last_write_at: Option<String>,
}

impl ManageWorkspaceTool {
    /// Handle status command - report every known checkout, or one selected by
    /// `workspace_id` or `path`.
    pub(crate) async fn handle_status_command(
        &self,
        handler: &JulieServerHandler,
        workspace_id: Option<String>,
        path: Option<String>,
    ) -> Result<CallToolResult> {
        let targets = status_targets(handler, workspace_id, path)?;
        let mut checkouts = Vec::with_capacity(targets.len());
        for (workspace_id, root) in targets {
            checkouts.push(checkout_status(handler, workspace_id, root).await?);
        }
        let mut result = CallToolResult::text_content(vec![Content::text(render(&checkouts))]);
        result.structured_content = Some(serde_json::json!({ "checkouts": checkouts }));
        Ok(result)
    }
}

fn status_targets(
    handler: &JulieServerHandler,
    workspace_id: Option<String>,
    path: Option<String>,
) -> Result<Vec<(String, PathBuf)>> {
    let store = registry_store_for_handler(handler)?;
    if let Some(path) = path {
        let root = PathBuf::from(shellexpand::tilde(&path).to_string());
        let root = root.canonicalize().unwrap_or(root);
        let root_str = root.to_string_lossy().to_string();
        let known = store
            .as_ref()
            .and_then(|store| store.get_workspace_by_path(&root_str).ok().flatten());
        let id = match known {
            Some(row) => row.workspace_id,
            None => generate_workspace_id(&root_str)?,
        };
        return Ok(vec![(id, root)]);
    }
    if let Some(id) = workspace_id {
        let row = store
            .as_ref()
            .map(|store| store.get_workspace(&id))
            .transpose()?
            .flatten()
            .ok_or_else(|| anyhow!("Workspace not found: {id}"))?;
        return Ok(vec![(row.workspace_id, PathBuf::from(row.path))]);
    }
    match store {
        Some(store) => Ok(store
            .list_workspaces()?
            .into_iter()
            .map(|row| (row.workspace_id, PathBuf::from(row.path)))
            .collect()),
        None => {
            let root = handler.current_workspace_root();
            let id = match handler.current_workspace_id() {
                Some(id) => id,
                None => generate_workspace_id(&root.to_string_lossy())?,
            };
            Ok(vec![(id, root)])
        }
    }
}

async fn checkout_status(
    handler: &JulieServerHandler,
    workspace_id: String,
    root: PathBuf,
) -> Result<CheckoutStatus> {
    let index_dir = handler.workspace_index_dir_for(&workspace_id).await?;
    let db_path = index_dir.join("db").join("symbols.db");
    let db_bytes = file_len(&db_path) + file_len(&index_dir.join("db").join("symbols.db-wal"));
    let (file_count, symbol_count, vector_count) = if db_path.exists() {
        let db_path = db_path.clone();
        tokio::task::spawn_blocking(move || -> Result<(i64, i64, i64)> {
            let db = crate::database::SymbolDatabase::new(&db_path)?;
            Ok((
                db.get_file_count_for_workspace()?,
                db.get_symbol_count_for_workspace()?,
                db.embedding_count()?,
            ))
        })
        .await??
    } else {
        (0, 0, 0)
    };
    let tantivy_dir = index_dir.join("tantivy");
    let tantivy_meta_mtime = mtime(&tantivy_dir.join("meta.json"));
    let tantivy = if !tantivy_dir.exists() {
        "absent"
    } else if tantivy_meta_mtime.is_some() {
        "present"
    } else {
        "building"
    };
    let (watcher, last_file_event_at) = watcher_state(handler, &root).await;
    Ok(CheckoutStatus {
        workspace_id,
        root_exists: root.exists(),
        root: root.to_string_lossy().to_string(),
        watcher,
        last_file_event_at,
        file_count,
        symbol_count,
        db_bytes,
        tantivy: tantivy.to_string(),
        tantivy_age_seconds: tantivy_meta_mtime
            .and_then(|at| at.elapsed().ok())
            .map(|age| age.as_secs()),
        vector_count,
        last_write_at: mtime(&db_path).map(rfc3339),
    })
}

async fn watcher_state(handler: &JulieServerHandler, root: &Path) -> (String, Option<String>) {
    let workspace = handler.workspace.read().await;
    let watcher = workspace
        .as_ref()
        .filter(|ws| ws.root.canonicalize().unwrap_or_else(|_| ws.root.clone()) == root)
        .and_then(|ws| ws.watcher.as_ref());
    match watcher {
        Some(watcher) if watcher.is_running() => (
            "running".to_string(),
            watcher.last_file_event_at().await.map(rfc3339),
        ),
        _ => ("stopped".to_string(), None),
    }
}

fn file_len(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

fn rfc3339(at: SystemTime) -> String {
    chrono::DateTime::<chrono::Utc>::from(at).to_rfc3339()
}

fn render(checkouts: &[CheckoutStatus]) -> String {
    let mut out = format!("Checkouts: {}\n", checkouts.len());
    for c in checkouts {
        let missing = if c.root_exists { "" } else { " [MISSING]" };
        let age = c
            .tantivy_age_seconds
            .map(|s| format!(" ({s}s old)"))
            .unwrap_or_default();
        out.push_str(&format!(
            "\n{} {}{}\n  watcher: {} | files: {} | symbols: {} | vectors: {} | db: {} bytes | tantivy: {}{}\n  last write: {} | last file event: {}\n",
            c.workspace_id,
            c.root,
            missing,
            c.watcher,
            c.file_count,
            c.symbol_count,
            c.vector_count,
            c.db_bytes,
            c.tantivy,
            age,
            c.last_write_at.as_deref().unwrap_or("never"),
            c.last_file_event_at.as_deref().unwrap_or("none"),
        ));
    }
    out
}
