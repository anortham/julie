use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Result, anyhow};
use julie_index::checkout_store::{FACTS_FILE, StoreStatus, TantivyState};
use serde::{Deserialize, Serialize};

use super::{ManageWorkspaceTool, registry_store_for_handler};
use crate::handler::JulieServerHandler;
use crate::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use crate::workspace::registry::generate_workspace_id;

/// Facts about one checkout, computed on request from the registry row, the
/// store on disk, and the loaded workspace's watcher.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckoutStatus {
    pub workspace_id: String,
    pub root: String,
    pub root_exists: bool,
    pub watcher: String,
    pub last_file_event_at: Option<String>,
    pub file_count: i64,
    pub symbol_count: i64,
    pub blob_count: u64,
    pub facts_bytes: u64,
    pub tantivy: String,
    pub tantivy_age_seconds: Option<u64>,
    pub graph_symbols: u64,
    pub graph_load_millis: u64,
    pub graph_resident_bytes: u64,
    pub vector_count: i64,
    pub vector_scan_millis: Option<u64>,
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
    let facts_path = index_dir.join(FACTS_FILE);
    let facts_mtime = mtime(&facts_path);
    let (store_status, file_count, vector_scan_millis) = if facts_path.exists() {
        match handler
            .checkout_store_for_workspace(&workspace_id, &root)
            .await
        {
            Ok(store) => {
                let snapshot = store.current();
                let file_count = snapshot.graph().paths().len() as i64;
                let vector_scan_millis = snapshot
                    .vectors()
                    .as_ref()
                    .last_scan_micros()
                    .map(|m| m / 1000);
                (Some(store.status()), file_count, vector_scan_millis)
            }
            Err(_) => (None, 0, None),
        }
    } else {
        (None, 0, None)
    };
    let (watcher, last_file_event_at) = watcher_state(handler, &root).await;
    Ok(status_from_store(
        workspace_id,
        root,
        watcher,
        last_file_event_at,
        store_status,
        file_count,
        facts_mtime,
        vector_scan_millis,
    ))
}

fn status_from_store(
    workspace_id: String,
    root: PathBuf,
    watcher: String,
    last_file_event_at: Option<String>,
    store_status: Option<StoreStatus>,
    file_count: i64,
    facts_mtime: Option<SystemTime>,
    vector_scan_millis: Option<u64>,
) -> CheckoutStatus {
    let root_exists = root.exists();
    let Some(status) = store_status else {
        return CheckoutStatus {
            workspace_id,
            root_exists,
            root: root.to_string_lossy().to_string(),
            watcher,
            last_file_event_at,
            file_count: 0,
            symbol_count: 0,
            blob_count: 0,
            facts_bytes: 0,
            tantivy: "absent".to_string(),
            tantivy_age_seconds: None,
            graph_symbols: 0,
            graph_load_millis: 0,
            graph_resident_bytes: 0,
            vector_count: 0,
            vector_scan_millis: None,
            last_write_at: None,
        };
    };
    CheckoutStatus {
        workspace_id,
        root_exists,
        root: root.to_string_lossy().to_string(),
        watcher,
        last_file_event_at,
        file_count,
        symbol_count: status.graph.symbols as i64,
        blob_count: status.blob_count,
        facts_bytes: status.facts_bytes,
        tantivy: tantivy_label(status.tantivy).to_string(),
        tantivy_age_seconds: status.tantivy_age_secs,
        graph_symbols: status.graph.symbols as u64,
        graph_load_millis: status.graph.load_millis,
        graph_resident_bytes: status.graph.resident_bytes as u64,
        vector_count: status.vector_count as i64,
        vector_scan_millis,
        last_write_at: status.last_write_at.or(facts_mtime).map(rfc3339),
    }
}

fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

fn tantivy_label(state: TantivyState) -> &'static str {
    match state {
        TantivyState::Present => "present",
        TantivyState::Building => "building",
        TantivyState::Stale => "stale",
        TantivyState::Absent => "absent",
    }
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
        let scan = c
            .vector_scan_millis
            .map(|ms| format!(" (last scan: {ms}ms)"))
            .unwrap_or_default();
        out.push_str(&format!(
            "\n{} {}{}\n  watcher: {} | files: {} | symbols: {} | blobs: {} | vectors: {}{} | facts: {} bytes | tantivy: {}{}\n  graph: {} symbols, {} ms, {} bytes | last write: {} | last file event: {}\n",
            c.workspace_id,
            c.root,
            missing,
            c.watcher,
            c.file_count,
            c.symbol_count,
            c.blob_count,
            c.vector_count,
            scan,
            c.facts_bytes,
            c.tantivy,
            age,
            c.graph_symbols,
            c.graph_load_millis,
            c.graph_resident_bytes,
            c.last_write_at.as_deref().unwrap_or("never"),
            c.last_file_event_at.as_deref().unwrap_or("none"),
        ));
    }
    out
}
