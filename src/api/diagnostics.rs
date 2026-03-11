//! Diagnostic report endpoint.
//!
//! `GET /api/diagnostics/report` — collects system info, daemon health, project
//! status, embedding state, and recent log lines into a single JSON response.
//! Designed for the web dashboard's "Export Diagnostics" button and the tray
//! app's diagnostic bundle feature.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;
use utoipa::ToSchema;

use crate::server::AppState;

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

/// Top-level diagnostic report.
#[derive(Debug, Serialize, ToSchema)]
pub struct DiagnosticReport {
    /// ISO 8601 timestamp of when this report was generated.
    pub generated_at: String,
    pub system: SystemInfo,
    pub daemon: DaemonHealth,
    pub projects: Vec<ProjectDiagnostic>,
    pub embeddings: Vec<EmbeddingDiagnostic>,
    /// Last N lines from today's log file.
    pub recent_logs: Vec<String>,
    /// Last N lines from daemon stdout/stderr logs.
    pub daemon_logs: DaemonLogs,
}

/// Basic system and server info.
#[derive(Debug, Serialize, ToSchema)]
pub struct SystemInfo {
    pub version: String,
    pub os: String,
    pub arch: String,
    pub uptime_seconds: u64,
}

/// Daemon health snapshot.
#[derive(Debug, Serialize, ToSchema)]
pub struct DaemonHealth {
    pub status: String,
    pub pid_file_exists: bool,
    pub active_watchers: usize,
}

/// Per-project diagnostic info.
#[derive(Debug, Serialize, ToSchema)]
pub struct ProjectDiagnostic {
    pub name: String,
    pub workspace_id: String,
    pub status: String,
    pub symbol_count: Option<u64>,
    pub file_count: Option<u64>,
}

/// Per-project embedding diagnostic.
#[derive(Debug, Serialize, ToSchema)]
pub struct EmbeddingDiagnostic {
    pub project: String,
    pub backend: Option<String>,
    pub accelerated: Option<bool>,
    pub degraded_reason: Option<String>,
    pub embedding_count: i64,
    pub initialized: bool,
}

/// Daemon process stdout/stderr log tails.
#[derive(Debug, Serialize, ToSchema)]
pub struct DaemonLogs {
    pub stdout_tail: Vec<String>,
    pub stderr_tail: Vec<String>,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// Maximum number of log lines to include per log source.
const MAX_LOG_LINES: usize = 200;

/// `GET /api/diagnostics/report`
///
/// Collects a comprehensive diagnostic snapshot for debugging and issue reports.
#[utoipa::path(
    get,
    path = "/api/diagnostics/report",
    tag = "diagnostics",
    responses(
        (status = 200, description = "Diagnostic report", body = DiagnosticReport),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn report(
    State(state): State<Arc<AppState>>,
) -> Result<Json<DiagnosticReport>, (StatusCode, String)> {
    let now = chrono::Local::now();

    // -- System info --
    let system = SystemInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        uptime_seconds: state.start_time.elapsed().as_secs(),
    };

    // -- Daemon health --
    let pid_file_exists = state.julie_home.join("daemon.pid").exists();
    let active_watchers = {
        let ds = state.daemon_state.read().await;
        ds.watcher_manager.active_watchers().await.len()
    };
    let daemon = DaemonHealth {
        status: "running".to_string(),
        pid_file_exists,
        active_watchers,
    };

    // -- Projects --
    let projects = {
        let ds = state.daemon_state.read().await;
        let registry = state.registry.read().await;
        registry
            .list_projects()
            .iter()
            .map(|entry| {
                let ws = ds.workspaces.get(&entry.workspace_id);
                let (symbol_count, file_count) = ws
                    .and_then(|loaded| loaded.workspace.db.as_ref())
                    .and_then(|db| {
                        let db = db.lock().ok()?;
                        let symbols = db.get_symbol_count_for_workspace().ok().map(|n| n as u64);
                        let files = db.get_file_count_for_workspace().ok().map(|n| n as u64);
                        Some((symbols, files))
                    })
                    .unwrap_or((None, None));

                ProjectDiagnostic {
                    name: entry.name.clone(),
                    workspace_id: entry.workspace_id.clone(),
                    status: format!("{:?}", ws.map(|l| &l.status)),
                    symbol_count,
                    file_count,
                }
            })
            .collect()
    };

    // -- Embeddings (reuse pattern from dashboard.rs) --
    let embeddings = {
        let ds = state.daemon_state.read().await;
        let registry = state.registry.read().await;
        registry
            .list_projects()
            .iter()
            .map(|entry| {
                let ws = ds.workspaces.get(&entry.workspace_id);
                let (backend, accelerated, degraded_reason, initialized) =
                    if let Some(loaded) = ws {
                        if let Some(ers) = &loaded.workspace.embedding_runtime_status {
                            (
                                Some(ers.resolved_backend.as_str().to_string()),
                                Some(ers.accelerated),
                                ers.degraded_reason.clone(),
                                true,
                            )
                        } else {
                            (None, None, None, false)
                        }
                    } else {
                        (None, None, None, false)
                    };

                let embedding_count = ws
                    .and_then(|loaded| loaded.workspace.db.as_ref())
                    .and_then(|db| {
                        let db = db.lock().ok()?;
                        db.embedding_count().ok()
                    })
                    .unwrap_or(0);

                EmbeddingDiagnostic {
                    project: entry.name.clone(),
                    backend,
                    accelerated,
                    degraded_reason,
                    embedding_count,
                    initialized,
                }
            })
            .collect()
    };

    // -- Recent logs --
    let recent_logs = read_log_tail(&state.julie_home, &now, MAX_LOG_LINES);

    // -- Daemon stdout/stderr logs --
    let daemon_logs = DaemonLogs {
        stdout_tail: read_file_tail(
            &state.julie_home.join("logs").join("daemon-stdout.log"),
            MAX_LOG_LINES,
        ),
        stderr_tail: read_file_tail(
            &state.julie_home.join("logs").join("daemon-stderr.log"),
            MAX_LOG_LINES,
        ),
    };

    Ok(Json(DiagnosticReport {
        generated_at: now.to_rfc3339(),
        system,
        daemon,
        projects,
        embeddings,
        recent_logs,
        daemon_logs,
    }))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Read the last `max_lines` from today's log file.
fn read_log_tail(
    julie_home: &std::path::Path,
    now: &chrono::DateTime<chrono::Local>,
    max_lines: usize,
) -> Vec<String> {
    let date_str = now.format("%Y-%m-%d").to_string();
    let log_path = julie_home.join("logs").join(format!("julie.log.{}", date_str));
    read_file_tail(&log_path, max_lines)
}

/// Read the last `max_lines` from a file, returning an empty vec on any error.
fn read_file_tail(path: &std::path::Path, max_lines: usize) -> Vec<String> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let lines: Vec<&str> = content.lines().collect();
    let start = lines.len().saturating_sub(max_lines);
    lines[start..].iter().map(|s| sanitize_home(s)).collect()
}

/// Replace the home directory with `~` in output to avoid leaking paths.
/// On Windows, also handles forward-slash variants of the home path.
fn sanitize_home(s: &str) -> String {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    if home.is_empty() {
        return s.to_string();
    }
    let result = s.replace(&home, "~");
    // On Windows, paths may use forward slashes too (e.g. C:/Users/Name)
    let home_fwd = home.replace('\\', "/");
    if home_fwd != home {
        result.replace(&home_fwd, "~")
    } else {
        result
    }
}
