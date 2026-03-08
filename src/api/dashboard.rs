//! Dashboard stats endpoint.
//!
//! `GET /api/dashboard/stats` — aggregated statistics from DaemonState,
//! memory filesystem, DispatchManager, and detected backends.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;

use crate::agent::backend::BackendInfo;
use crate::daemon_state::WorkspaceLoadStatus;
use crate::memory;
use crate::server::AppState;

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

/// Top-level dashboard stats response.
#[derive(Debug, Serialize)]
pub struct DashboardStats {
    pub projects: ProjectStats,
    pub memories: MemoryStats,
    pub agents: AgentStats,
    pub backends: Vec<BackendStat>,
    /// Number of active file watchers (one per watched project).
    pub active_watchers: usize,
}

/// Breakdown of project counts by status.
#[derive(Debug, Serialize)]
pub struct ProjectStats {
    pub total: usize,
    pub ready: usize,
    pub indexing: usize,
    pub error: usize,
    pub registered: usize,
    pub stale: usize,
}

/// Memory system summary.
#[derive(Debug, Serialize)]
pub struct MemoryStats {
    pub total_checkpoints: usize,
    pub active_plan: Option<String>,
    pub last_checkpoint: Option<String>,
}

/// Agent dispatch summary.
#[derive(Debug, Serialize)]
pub struct AgentStats {
    pub total_dispatches: usize,
    pub last_dispatch: Option<String>,
}

/// Single backend status entry.
#[derive(Debug, Serialize)]
pub struct BackendStat {
    pub name: String,
    pub available: bool,
    pub version: Option<String>,
}

impl From<&BackendInfo> for BackendStat {
    fn from(b: &BackendInfo) -> Self {
        Self {
            name: b.name.clone(),
            available: b.available,
            version: b.version.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// `GET /api/dashboard/stats`
///
/// Aggregates stats from all subsystems into a single response.
pub async fn stats(
    State(state): State<Arc<AppState>>,
) -> Result<Json<DashboardStats>, (StatusCode, String)> {
    // -- Projects --
    let project_stats = {
        let ds = state.daemon_state.read().await;
        let mut ready = 0usize;
        let mut indexing = 0usize;
        let mut error = 0usize;
        let mut registered = 0usize;
        let mut stale = 0usize;

        for ws in ds.workspaces.values() {
            match &ws.status {
                WorkspaceLoadStatus::Ready => ready += 1,
                WorkspaceLoadStatus::Indexing => indexing += 1,
                WorkspaceLoadStatus::Error(_) => error += 1,
                WorkspaceLoadStatus::Registered => registered += 1,
                WorkspaceLoadStatus::Stale => stale += 1,
            }
        }

        ProjectStats {
            total: ds.workspaces.len(),
            ready,
            indexing,
            error,
            registered,
            stale,
        }
    };

    // -- Memories --
    // Find first Ready workspace and gather memory stats from its filesystem.
    // Clone the path and drop the read lock before awaiting spawn_blocking.
    let memory_stats = {
        let ds = state.daemon_state.read().await;
        let workspace_path = ds
            .workspaces
            .values()
            .find(|ws| ws.status == WorkspaceLoadStatus::Ready)
            .map(|ws| ws.path.clone());
        drop(ds);

        match workspace_path {
            Some(path) => {
                tokio::task::spawn_blocking(move || gather_memory_stats(&path))
                    .await
                    .unwrap_or(MemoryStats {
                        total_checkpoints: 0,
                        active_plan: None,
                        last_checkpoint: None,
                    })
            }
            None => MemoryStats {
                total_checkpoints: 0,
                active_plan: None,
                last_checkpoint: None,
            },
        }
    };

    // -- Agents --
    let agent_stats = {
        let dm = state.dispatch_manager.read().await;
        let dispatches = dm.list_dispatches();
        let total = dispatches.len();
        // Find the most recent dispatch by started_at timestamp.
        let last_dispatch = dispatches
            .iter()
            .map(|d| d.started_at.as_str())
            .max()
            .map(|s| s.to_string());

        AgentStats {
            total_dispatches: total,
            last_dispatch,
        }
    };

    // -- Backends --
    let backends: Vec<BackendStat> = state.backends.iter().map(BackendStat::from).collect();

    // -- Active watchers --
    let active_watchers = {
        let ds = state.daemon_state.read().await;
        ds.watcher_manager.active_watchers().await.len()
    };

    Ok(Json(DashboardStats {
        projects: project_stats,
        memories: memory_stats,
        agents: agent_stats,
        backends,
        active_watchers,
    }))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Walk `.memories/` date directories and count checkpoints.
///
/// Returns total count, active plan title, and the timestamp of the most
/// recent checkpoint (by filename sort, newest last in chronological order).
fn gather_memory_stats(workspace_root: &std::path::Path) -> MemoryStats {
    let memories_dir = workspace_root.join(".memories");

    // Active plan
    let active_plan = memory::plan::get_active_plan(workspace_root)
        .ok()
        .flatten()
        .map(|p| p.title);

    if !memories_dir.exists() {
        return MemoryStats {
            total_checkpoints: 0,
            active_plan,
            last_checkpoint: None,
        };
    }

    // Walk date directories and count checkpoint files.
    let mut total = 0usize;
    let mut newest_timestamp: Option<String> = None;

    let entries = match std::fs::read_dir(&memories_dir) {
        Ok(e) => e,
        Err(_) => {
            return MemoryStats {
                total_checkpoints: 0,
                active_plan,
                last_checkpoint: None,
            }
        }
    };

    // Collect date dirs, sorted reverse chronologically so the first one
    // with checkpoint files gives us the newest checkpoint.
    let mut date_dirs: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if is_date_dir(&name) && e.path().is_dir() {
                Some(name)
            } else {
                None
            }
        })
        .collect();

    date_dirs.sort_unstable_by(|a, b| b.cmp(a)); // newest first

    for date_str in &date_dirs {
        let dir = memories_dir.join(date_str);
        let files: Vec<String> = std::fs::read_dir(&dir)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                if name.ends_with(".md") {
                    Some(name)
                } else {
                    None
                }
            })
            .collect();

        total += files.len();

        // If we haven't found the newest yet and there are files, the newest
        // checkpoint is in the first date dir (newest first) with the
        // lexicographically last filename (HHMMSS_hash.md).
        if newest_timestamp.is_none() && !files.is_empty() {
            // Derive timestamp from date + filename: "2026-03-08" + "023301_abcd.md"
            // → "2026-03-08T02:33:01Z"
            if let Some(newest_file) = files.iter().max() {
                newest_timestamp = parse_timestamp_from_filename(date_str, newest_file);
            }
        }
    }

    MemoryStats {
        total_checkpoints: total,
        active_plan,
        last_checkpoint: newest_timestamp,
    }
}

/// Check if a directory name matches `YYYY-MM-DD` format.
fn is_date_dir(name: &str) -> bool {
    if name.len() != 10 {
        return false;
    }
    let bytes = name.as_bytes();
    // Pattern: DDDD-DD-DD where D is a digit
    bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[..4].iter().all(|b| b.is_ascii_digit())
        && bytes[5..7].iter().all(|b| b.is_ascii_digit())
        && bytes[8..10].iter().all(|b| b.is_ascii_digit())
}

/// Parse a timestamp from a checkpoint filename.
///
/// Filename format: `HHMMSS_hash.md` (e.g. `023301_abcd.md`)
/// Combined with date string: `2026-03-08` → `2026-03-08T02:33:01Z`
fn parse_timestamp_from_filename(date: &str, filename: &str) -> Option<String> {
    // Strip .md, take the HHMMSS part before the underscore
    let stem = filename.strip_suffix(".md")?;
    let time_part = stem.split('_').next()?;
    if time_part.len() != 6 {
        return None;
    }

    let hh = &time_part[0..2];
    let mm = &time_part[2..4];
    let ss = &time_part[4..6];

    Some(format!("{date}T{hh}:{mm}:{ss}Z"))
}
