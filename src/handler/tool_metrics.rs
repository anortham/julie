use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use julie_index::checkout_store::CheckoutStore;
use serde_json::json;
use tokio::sync::RwLock;
use tracing::warn;

use crate::dashboard::state::DashboardEvent;
use crate::handler::{JulieServerHandler, PrimaryWorkspaceBinding};
use crate::registry::database::CallClient;
use crate::tools::metrics::session::{SessionMetrics, ToolCallReport, ToolKind};
use crate::workspace::JulieWorkspace;

tokio::task_local! {
    pub(crate) static CALL_CLIENT: Option<CallClient>;
}

/// Indexed size for `paths`: `blobs.byte_len` when the path is in facts, else
/// the on-disk file size under `root`.
pub(crate) fn source_bytes_for_paths(
    store: &CheckoutStore,
    root: &Path,
    paths: &[String],
) -> Option<u64> {
    if paths.is_empty() {
        return None;
    }
    let facts = store.current().facts().ok()?;
    let conn = facts.conn();
    let mut total = 0u64;
    for path in paths {
        let from_facts: rusqlite::Result<i64> = conn.query_row(
            "SELECT b.byte_len FROM paths p JOIN blobs b ON b.hash = p.blob_hash WHERE p.path = ?1",
            [path.as_str()],
            |row| row.get(0),
        );
        match from_facts {
            Ok(n) => total += n as u64,
            Err(_) => {
                total += std::fs::metadata(root.join(path))
                    .map(|m| m.len())
                    .unwrap_or(0);
            }
        }
    }
    Some(total)
}

/// Data for a single metrics write, sent via bounded channel to the background writer.
/// Avoids spawning a new task per tool call.
pub(crate) struct MetricsTask {
    pub workspace: Arc<RwLock<Option<JulieWorkspace>>>,
    pub session_metrics: Arc<SessionMetrics>,
    pub session_id: String,
    pub tool_name: String,
    pub duration_ms: f64,
    pub result_count: Option<u32>,
    pub source_bytes: Option<u64>,
    pub source_file_paths: Vec<String>,
    pub input_bytes: Option<u64>,
    pub output_bytes: u64,
    pub success: bool,
    pub metadata_str: Option<String>,
    pub daemon_db: Option<Arc<crate::registry::database::DaemonDatabase>>,
    pub workspace_id: Option<String>,
    pub client: Option<CallClient>,
}

/// Single background task that drains the metrics channel and writes to SQLite.
pub(crate) async fn run_metrics_writer(mut rx: tokio::sync::mpsc::Receiver<MetricsTask>) {
    while let Some(task) = rx.recv().await {
        let mut source_bytes: Option<u64> = task.source_bytes;
        let resolved_workspace = task.workspace.read().await.clone();

        if let Some(ws) = resolved_workspace.as_ref() {
            if source_bytes.is_none() {
                source_bytes = source_bytes_for_paths(&ws.store, &ws.root, &task.source_file_paths);
            }
        }
        if let Some(sb) = source_bytes {
            task.session_metrics
                .total_source_bytes
                .fetch_add(sb, Ordering::Relaxed);
        }

        if let (Some(db), Some(ref workspace_id)) = (&task.daemon_db, task.workspace_id.as_ref()) {
            let db = Arc::clone(db);
            let workspace_id = workspace_id.to_string();
            let session_id = task.session_id.clone();
            let tool_name = task.tool_name.clone();
            let duration_ms = task.duration_ms;
            let result_count = task.result_count;
            let input_bytes = task.input_bytes;
            let output_bytes = task.output_bytes;
            let success = task.success;
            let metadata_str = task.metadata_str.clone();
            let client = task.client.clone();

            let _ = tokio::task::spawn_blocking(move || {
                if let Err(e) = db.insert_tool_call_with_input_bytes(
                    &workspace_id,
                    &session_id,
                    &tool_name,
                    duration_ms,
                    result_count,
                    source_bytes,
                    input_bytes,
                    Some(output_bytes),
                    success,
                    metadata_str.as_deref(),
                    client.as_ref(),
                ) {
                    warn!("Failed to write tool call to daemon.db: {}", e);
                }
            });
        }
    }
}

impl JulieServerHandler {
    pub(crate) fn input_bytes_from_metadata(metadata: &serde_json::Value) -> Option<u64> {
        serde_json::to_vec(metadata)
            .ok()
            .map(|bytes| bytes.len() as u64)
    }

    /// Backward-compatible success wrapper.
    pub(crate) fn record_tool_call(
        &self,
        tool_name: &str,
        duration: std::time::Duration,
        report: &ToolCallReport,
        workspace_snapshot: Option<&PrimaryWorkspaceBinding>,
    ) {
        self.record_tool_call_outcome(tool_name, duration, report, workspace_snapshot, true);
    }

    pub(crate) fn record_tool_call_outcome(
        &self,
        tool_name: &str,
        duration: std::time::Duration,
        report: &ToolCallReport,
        workspace_snapshot: Option<&PrimaryWorkspaceBinding>,
        success: bool,
    ) {
        let duration_us = duration.as_micros() as u64;
        let output_bytes = report.output_bytes;
        let workspace_id = workspace_snapshot
            .map(|binding| binding.workspace_id.clone())
            .or_else(|| self.current_workspace_id());
        if let Some(kind) = ToolKind::from_name(tool_name) {
            self.session_metrics
                .record(kind, duration_us, 0, output_bytes);
        }
        if let Some(ref log) = self.project_log {
            log.tool_call(tool_name, duration.as_secs_f64() * 1000.0, output_bytes);
        }
        if let Some(ref tx) = self.dashboard_tx {
            let _ = tx.send(DashboardEvent::ToolCall {
                tool_name: tool_name.to_string(),
                workspace: workspace_id.clone().unwrap_or_default(),
                duration_ms: duration.as_secs_f64() * 1000.0,
            });
        }

        let metadata = report.metadata.to_string();
        let _ = self.metrics_tx.try_send(MetricsTask {
            workspace: self.workspace.clone(),
            session_metrics: self.session_metrics.clone(),
            session_id: self.session_metrics.session_id.clone(),
            tool_name: tool_name.to_string(),
            duration_ms: duration.as_secs_f64() * 1000.0,
            result_count: report.result_count,
            source_bytes: report.source_bytes,
            source_file_paths: report.source_file_paths.clone(),
            input_bytes: report.input_bytes,
            output_bytes,
            success,
            metadata_str: if metadata == "null" {
                None
            } else {
                Some(metadata)
            },
            daemon_db: self.daemon_db.clone(),
            workspace_id,
            client: CALL_CLIENT.try_with(|c| c.clone()).ok().flatten(),
        });
    }

    pub(crate) fn record_tool_failure(
        &self,
        tool_name: &str,
        duration: std::time::Duration,
        workspace_snapshot: Option<&PrimaryWorkspaceBinding>,
        metadata: serde_json::Value,
        source_file_paths: Vec<String>,
        input_bytes: Option<u64>,
        error_message: &str,
    ) {
        let mut failure_metadata = metadata;
        if failure_metadata.is_null() {
            failure_metadata = json!({});
        }
        if let Some(obj) = failure_metadata.as_object_mut() {
            obj.insert("error.message".to_string(), json!(error_message));
        } else {
            failure_metadata = json!({ "error.message": error_message });
        }
        let report = ToolCallReport {
            result_count: None,
            input_bytes,
            source_bytes: None,
            output_bytes: 0,
            metadata: failure_metadata,
            source_file_paths,
        };
        self.record_tool_call_outcome(tool_name, duration, &report, workspace_snapshot, false);
    }
}
