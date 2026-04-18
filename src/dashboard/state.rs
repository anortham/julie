//! Shared state for the dashboard HTTP server and SSE broadcast channel.

use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;
use tokio::sync::broadcast;

use crate::daemon::database::DaemonDatabase;
use crate::daemon::embedding_service::EmbeddingService;
use crate::daemon::lifecycle::{LifecyclePhase, LifecyclePhaseKind, ShutdownCause};
use crate::daemon::session::{SessionPhaseCounts, SessionTracker};
use crate::daemon::watcher_pool::WatcherPool;
use crate::daemon::workspace_pool::WorkspacePool;
use crate::dashboard::error_buffer::{ErrorBuffer, LogEntry};
use crate::embeddings::EmbeddingBackend;
use crate::embeddings::EmbeddingRuntimeStatus;
use crate::health::{
    EmbeddingRuntimeHealth, HealthLevel, ProjectionFreshness, ProjectionState,
    SearchProjectionHealth, SystemStatus, overall_from_planes, project_embedding_runtime,
    search_projection_health_for_workspace,
};

// ---------------------------------------------------------------------------
// DashboardEvent
// ---------------------------------------------------------------------------

/// Events broadcast over the SSE channel to connected dashboard clients.
#[derive(Debug, Clone)]
pub enum DashboardEvent {
    ToolCall {
        tool_name: String,
        workspace: String,
        duration_ms: f64,
    },
    SessionChange {
        active_count: usize,
    },
}

// ---------------------------------------------------------------------------
// DashboardHealth
// ---------------------------------------------------------------------------

pub use crate::daemon::lifecycle::LifecyclePhaseKind as DashboardDaemonPhase;

#[derive(Debug, Clone, Serialize)]
pub struct DashboardHealthSnapshot {
    pub overall: HealthLevel,
    pub control_plane: DashboardControlPlaneHealth,
    pub data_plane: DashboardDataPlaneHealth,
    pub runtime_plane: DashboardRuntimePlaneHealth,
}

#[derive(Debug, Clone, Serialize)]
pub struct DashboardControlPlaneHealth {
    pub level: HealthLevel,
    pub daemon_phase: DashboardDaemonPhase,
    pub shutdown_cause: Option<ShutdownCause>,
    pub active_sessions: usize,
    pub restart_pending: bool,
    pub session_phases: SessionPhaseCounts,
    pub daemon_db_connected: bool,
    pub workspace_pool_connected: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DashboardDataPlaneHealth {
    pub level: HealthLevel,
    pub readiness: SystemStatus,
    pub workspace_count: usize,
    pub active_workspace_count: usize,
    pub session_count: usize,
    pub ready_workspace_count: usize,
    pub pending_workspace_count: usize,
    pub other_workspace_count: usize,
    pub symbol_count: i64,
    pub file_count: i64,
    pub search_projection: SearchProjectionHealth,
    pub indexing: DashboardIndexingHealth,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DashboardIndexingHealth {
    pub level: HealthLevel,
    pub active_operation: Option<String>,
    pub stage: Option<String>,
    pub catchup_active: bool,
    pub watcher_paused: bool,
    pub watcher_rescan_pending: bool,
    pub dirty_projection_count: usize,
    pub repair_needed: bool,
    pub repair_issue_count: usize,
    pub repair_reasons: Vec<String>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DashboardRuntimePlaneHealth {
    pub level: HealthLevel,
    pub configured: bool,
    pub embedding_available: bool,
    pub embedding_initializing: bool,
    pub detail: String,
    pub embeddings: EmbeddingRuntimeHealth,
    pub runtime_status: Option<DashboardEmbeddingRuntimeStatus>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DashboardEmbeddingRuntimeStatus {
    pub requested_backend: String,
    pub resolved_backend: String,
    pub accelerated: bool,
    pub degraded_reason: Option<String>,
}

// ---------------------------------------------------------------------------
// DashboardState
// ---------------------------------------------------------------------------

/// Shared state injected into every dashboard route handler.
///
/// Cheap to clone — all fields are either `Arc`-wrapped or `Copy`.
#[derive(Clone)]
pub struct DashboardState {
    sessions: Arc<SessionTracker>,
    daemon_db: Option<Arc<DaemonDatabase>>,
    action_csrf_token: Arc<String>,
    restart_pending: Arc<AtomicBool>,
    daemon_phase: Arc<RwLock<LifecyclePhase>>,
    start_time: Instant,
    error_buffer: ErrorBuffer,
    /// Live reference to the daemon's shared embedding service. Stored as a
    /// reference (not a snapshot bool) so the dashboard reflects state
    /// transitions as the background init task progresses from
    /// `Initializing` -> `Ready` (or `Unavailable`) without needing a restart.
    /// `None` in test contexts that don't wire up a service; the
    /// `embedding_available` accessor returns `false` in that case.
    embedding_service: Option<Arc<EmbeddingService>>,
    watcher_pool: Option<Arc<WatcherPool>>,
    workspace_pool: Option<Arc<WorkspacePool>>,
    tx: broadcast::Sender<DashboardEvent>,
}

impl DashboardState {
    /// Create a new `DashboardState`.
    ///
    /// Internally creates an `ErrorBuffer` with the given capacity and a
    /// broadcast channel with capacity 256.
    pub fn new(
        sessions: Arc<SessionTracker>,
        daemon_db: Option<Arc<DaemonDatabase>>,
        restart_pending: Arc<AtomicBool>,
        daemon_phase: Arc<RwLock<LifecyclePhase>>,
        start_time: Instant,
        embedding_service: Option<Arc<EmbeddingService>>,
        workspace_pool: Option<Arc<WorkspacePool>>,
        error_buffer_capacity: usize,
    ) -> Self {
        Self::new_with_watcher_pool(
            sessions,
            daemon_db,
            restart_pending,
            daemon_phase,
            start_time,
            embedding_service,
            None,
            workspace_pool,
            error_buffer_capacity,
        )
    }

    pub fn new_with_watcher_pool(
        sessions: Arc<SessionTracker>,
        daemon_db: Option<Arc<DaemonDatabase>>,
        restart_pending: Arc<AtomicBool>,
        daemon_phase: Arc<RwLock<LifecyclePhase>>,
        start_time: Instant,
        embedding_service: Option<Arc<EmbeddingService>>,
        watcher_pool: Option<Arc<WatcherPool>>,
        workspace_pool: Option<Arc<WorkspacePool>>,
        error_buffer_capacity: usize,
    ) -> Self {
        let error_buffer = ErrorBuffer::new(error_buffer_capacity);
        let (tx, _rx) = broadcast::channel(256);
        Self {
            sessions,
            daemon_db,
            action_csrf_token: Arc::new(uuid::Uuid::new_v4().to_string()),
            restart_pending,
            daemon_phase,
            start_time,
            error_buffer,
            embedding_service,
            watcher_pool,
            workspace_pool,
            tx,
        }
    }

    /// Reference to the session tracker.
    pub fn sessions(&self) -> &SessionTracker {
        &self.sessions
    }

    /// Reference to the daemon database, if available.
    pub fn daemon_db(&self) -> Option<&Arc<DaemonDatabase>> {
        self.daemon_db.as_ref()
    }

    pub fn action_csrf_token(&self) -> &str {
        self.action_csrf_token.as_str()
    }

    /// Whether a daemon restart is pending.
    pub fn is_restart_pending(&self) -> bool {
        self.restart_pending.load(Ordering::Relaxed)
    }

    /// Time elapsed since the daemon started.
    pub fn uptime(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Reference to the error ring buffer.
    pub fn error_buffer(&self) -> &ErrorBuffer {
        &self.error_buffer
    }

    /// Snapshot of recent error/warn log entries, oldest first.
    pub fn error_entries(&self) -> Vec<LogEntry> {
        self.error_buffer.recent_entries()
    }

    /// Build a dashboard-friendly health snapshot from the state already
    /// available to the dashboard server.
    pub async fn health_snapshot(&self) -> DashboardHealthSnapshot {
        let active_sessions = self.sessions.active_count();
        let restart_pending = self.is_restart_pending();
        let daemon_phase_snapshot = *self.daemon_phase.read().unwrap_or_else(|p| p.into_inner());
        let daemon_phase = daemon_phase_snapshot.kind();
        let shutdown_cause = daemon_phase_snapshot.shutdown_cause();
        let session_phases = self.sessions.phase_counts();
        let workspace_pool_connected = self.workspace_pool.is_some();

        let (workspaces, daemon_db_connected) = match self.daemon_db.as_ref() {
            Some(db) => match db.list_workspaces() {
                Ok(rows) => (rows, true),
                Err(_) => (Vec::new(), false),
            },
            None => (Vec::new(), false),
        };

        let workspace_count = workspaces.len();
        let active_workspace_count = workspaces
            .iter()
            .filter(|row| row.session_count > 0)
            .count();
        let session_count = workspaces
            .iter()
            .map(|row| row.session_count.max(0) as usize)
            .sum();
        let ready_workspace_count = workspaces
            .iter()
            .filter(|row| row.status.eq_ignore_ascii_case("ready"))
            .count();
        let pending_workspace_count = workspaces
            .iter()
            .filter(|row| row.status.eq_ignore_ascii_case("pending"))
            .count();
        let other_workspace_count =
            workspace_count.saturating_sub(ready_workspace_count + pending_workspace_count);
        let symbol_count = workspaces
            .iter()
            .map(|row| row.symbol_count.unwrap_or(0))
            .sum();
        let file_count = workspaces
            .iter()
            .map(|row| row.file_count.unwrap_or(0))
            .sum();

        let control_plane = DashboardControlPlaneHealth {
            level: if !daemon_db_connected {
                HealthLevel::Unavailable
            } else if restart_pending || daemon_phase != LifecyclePhaseKind::Ready {
                HealthLevel::Degraded
            } else {
                HealthLevel::Ready
            },
            daemon_phase,
            shutdown_cause,
            active_sessions,
            restart_pending,
            session_phases,
            daemon_db_connected,
            workspace_pool_connected,
            detail: if !daemon_db_connected {
                "daemon registry unavailable".to_string()
            } else {
                format!(
                    "daemon {}{}; {} live session(s); phases {} / {} / {} / {}{}",
                    daemon_phase.label(),
                    shutdown_cause
                        .map_or_else(String::new, |cause| format!(" ({})", cause.label())),
                    active_sessions,
                    session_phases.connecting,
                    session_phases.bound,
                    session_phases.serving,
                    session_phases.closing,
                    if workspace_pool_connected {
                        ""
                    } else {
                        "; workspace pool detached"
                    }
                )
            },
        };

        let readiness = if !daemon_db_connected || workspace_count == 0 {
            SystemStatus::NotReady
        } else if pending_workspace_count > 0 || other_workspace_count > 0 {
            SystemStatus::SqliteOnly { symbol_count }
        } else {
            SystemStatus::FullyReady { symbol_count }
        };

        let data_plane_level = if !daemon_db_connected {
            HealthLevel::Unavailable
        } else if workspace_count == 0 || pending_workspace_count > 0 || other_workspace_count > 0 {
            HealthLevel::Degraded
        } else {
            HealthLevel::Ready
        };
        let search_projection = self.search_projection_health().await;
        let indexing = self.indexing_health().await;

        let data_plane = DashboardDataPlaneHealth {
            level: overall_from_planes(
                data_plane_level,
                indexing.level,
                if self.workspace_pool.is_some() {
                    search_projection.level
                } else {
                    HealthLevel::Ready
                },
                false,
            ),
            readiness,
            workspace_count,
            active_workspace_count,
            session_count,
            ready_workspace_count,
            pending_workspace_count,
            other_workspace_count,
            symbol_count,
            file_count,
            search_projection,
            indexing,
            detail: if !daemon_db_connected {
                "workspace registry unavailable".to_string()
            } else {
                format!(
                    "{} workspace(s): {} ready, {} pending, {} other, {} active session-bearing",
                    workspace_count,
                    ready_workspace_count,
                    pending_workspace_count,
                    other_workspace_count,
                    active_workspace_count
                )
            },
        };

        let embedding_service = self.embedding_service.as_ref();
        let embedding_available = embedding_service.is_some_and(|svc| svc.is_available());
        let embedding_initializing = embedding_service.is_some_and(|svc| !svc.is_settled());
        let runtime_status = embedding_service.and_then(|svc| svc.runtime_status());
        let embedding_provider = embedding_service.and_then(|svc| svc.provider());
        let runtime_status_snapshot =
            runtime_status
                .as_ref()
                .map(|status| DashboardEmbeddingRuntimeStatus {
                    requested_backend: backend_label(&status.requested_backend),
                    resolved_backend: backend_label(&status.resolved_backend),
                    accelerated: status.accelerated,
                    degraded_reason: status.degraded_reason.clone(),
                });
        let embeddings = project_embedding_runtime(
            runtime_status.clone(),
            embedding_provider.as_deref(),
            embedding_service.is_some(),
            embedding_initializing,
        );

        let runtime_plane = DashboardRuntimePlaneHealth {
            level: embeddings.level,
            configured: embedding_service.is_some(),
            embedding_available,
            embedding_initializing,
            detail: if embedding_service.is_none() {
                "embedding service not configured".to_string()
            } else if embedding_initializing {
                "embedding runtime initializing".to_string()
            } else if embedding_available {
                "embedding runtime available".to_string()
            } else if let Some(status) = runtime_status.as_ref() {
                status
                    .degraded_reason
                    .clone()
                    .unwrap_or_else(|| "embedding runtime unavailable".to_string())
            } else {
                "embedding runtime unavailable".to_string()
            },
            embeddings,
            runtime_status: runtime_status_snapshot,
        };

        let overall = overall_from_planes(
            control_plane.level,
            data_plane.level,
            runtime_plane.level,
            runtime_plane.configured,
        );

        DashboardHealthSnapshot {
            overall,
            control_plane,
            data_plane,
            runtime_plane,
        }
    }

    /// Whether an embedding provider is currently available. Reads the
    /// `EmbeddingService` state live on each call, so the dashboard reflects
    /// the background init task's progress (Initializing -> Ready) without
    /// needing a restart. Returns `false` when no service is configured.
    pub fn embedding_available(&self) -> bool {
        self.embedding_service
            .as_ref()
            .is_some_and(|svc| svc.is_available())
    }

    /// `true` when the embedding service is configured but still starting up.
    /// Reads `EmbeddingService` state live. Returns `false` when no service is
    /// configured (that's "Not configured", not "Initializing").
    pub fn embedding_initializing(&self) -> bool {
        self.embedding_service
            .as_ref()
            .is_some_and(|svc| !svc.is_settled())
    }

    /// Current embedding runtime status, if available. Reads the
    /// `EmbeddingService` state live on each call. Returns `None` when no
    /// service is configured or when the service has no runtime status
    /// (e.g. still in `Initializing`).
    pub fn embedding_runtime_status(&self) -> Option<EmbeddingRuntimeStatus> {
        self.embedding_service
            .as_ref()
            .and_then(|svc| svc.runtime_status())
    }

    /// Reference to the workspace pool, if available.
    pub fn workspace_pool(&self) -> Option<&Arc<WorkspacePool>> {
        self.workspace_pool.as_ref()
    }

    /// Reference to the watcher pool, if available.
    pub fn watcher_pool(&self) -> Option<&Arc<WatcherPool>> {
        self.watcher_pool.as_ref()
    }

    /// Subscribe to the broadcast channel. Each call returns an independent receiver.
    pub fn subscribe(&self) -> broadcast::Receiver<DashboardEvent> {
        self.tx.subscribe()
    }

    /// Send an event to all current subscribers. Ignores send errors (no subscribers is fine).
    pub fn send_event(&self, event: DashboardEvent) {
        let _ = self.tx.send(event);
    }

    /// Clone the broadcast sender (for use in middleware or background tasks).
    pub fn sender(&self) -> broadcast::Sender<DashboardEvent> {
        self.tx.clone()
    }
}

impl DashboardState {
    async fn indexing_health(&self) -> DashboardIndexingHealth {
        let Some(workspace_pool) = self.workspace_pool.as_ref() else {
            return DashboardIndexingHealth {
                level: HealthLevel::Ready,
                active_operation: None,
                stage: None,
                catchup_active: false,
                watcher_paused: false,
                watcher_rescan_pending: false,
                dirty_projection_count: 0,
                repair_needed: false,
                repair_issue_count: 0,
                repair_reasons: Vec::new(),
                detail: "indexing idle".to_string(),
            };
        };

        let snapshots = workspace_pool.indexing_snapshots().await;
        let mut active_workspace = None;
        let mut active_operation = None;
        let mut stage = None;
        let mut catchup_active = false;
        let mut watcher_paused = false;
        let mut watcher_rescan_pending = false;
        let mut dirty_projection_count = 0usize;
        let mut repair_needed = false;
        let mut repair_issue_count = 0usize;
        let mut repair_reasons = std::collections::BTreeSet::new();

        for (workspace_id, snapshot) in snapshots {
            if active_workspace.is_none()
                && (snapshot.active_operation.is_some()
                    || snapshot.catchup_active
                    || snapshot.watcher_paused
                    || snapshot.watcher_rescan_pending
                    || snapshot.dirty_projection_count > 0
                    || snapshot.repair_needed())
            {
                active_workspace = Some(workspace_id);
                active_operation = snapshot
                    .active_operation
                    .map(|operation| operation.as_str().to_string());
                stage = snapshot.stage.map(|stage| stage.as_str().to_string());
            }

            catchup_active |= snapshot.catchup_active;
            watcher_paused |= snapshot.watcher_paused;
            watcher_rescan_pending |= snapshot.watcher_rescan_pending;
            dirty_projection_count += snapshot.dirty_projection_count;
            repair_needed |= snapshot.repair_needed();
            repair_issue_count += snapshot.repair_issue_count();
            for reason in snapshot.repair_reasons {
                repair_reasons.insert(reason.as_str().to_string());
            }
        }

        let level = if catchup_active
            || watcher_paused
            || watcher_rescan_pending
            || dirty_projection_count > 0
            || repair_needed
            || active_operation.is_some()
        {
            HealthLevel::Degraded
        } else {
            HealthLevel::Ready
        };

        let detail = if level == HealthLevel::Ready {
            "indexing idle".to_string()
        } else {
            let mut parts = Vec::new();
            if let Some(workspace_id) = active_workspace.as_deref() {
                parts.push(format!("workspace {workspace_id}"));
            }
            if let Some(operation) = active_operation.as_deref() {
                parts.push(format!("operation {operation}"));
            }
            if let Some(stage_value) = stage.as_deref() {
                parts.push(format!("stage {stage_value}"));
            }
            if catchup_active {
                parts.push("catch-up active".to_string());
            }
            if watcher_paused {
                parts.push("watcher paused".to_string());
            }
            if watcher_rescan_pending {
                parts.push("watcher rescan pending".to_string());
            }
            if dirty_projection_count > 0 {
                parts.push(format!("{dirty_projection_count} dirty projection entries"));
            }
            if repair_needed {
                parts.push(format!("{repair_issue_count} repair issue(s)"));
            }
            parts.join(", ")
        };

        DashboardIndexingHealth {
            level,
            active_operation,
            stage,
            catchup_active,
            watcher_paused,
            watcher_rescan_pending,
            dirty_projection_count,
            repair_needed,
            repair_issue_count,
            repair_reasons: repair_reasons.into_iter().collect(),
            detail,
        }
    }

    async fn search_projection_health(&self) -> SearchProjectionHealth {
        let Some(workspace_pool) = self.workspace_pool.as_ref() else {
            return SearchProjectionHealth {
                level: HealthLevel::Unavailable,
                state: ProjectionState::Missing,
                freshness: ProjectionFreshness::Unavailable,
                workspace_id: None,
                canonical_revision: None,
                projected_revision: None,
                revision_lag: None,
                repair_needed: false,
                detail: "projection visibility unavailable because workspace pool is detached"
                    .to_string(),
            };
        };

        let snapshots = workspace_pool.projection_inputs().await;
        let mut selected: Option<SearchProjectionHealth> = None;

        for (workspace_id, db, search_index_ready) in snapshots {
            let projection = match db.lock() {
                Ok(db_lock) => {
                    let symbol_count = db_lock.get_symbol_count_for_workspace().unwrap_or(0);
                    search_projection_health_for_workspace(
                        &workspace_id,
                        &db_lock,
                        symbol_count,
                        search_index_ready,
                    )
                    .unwrap_or_else(|err| SearchProjectionHealth {
                        level: HealthLevel::Unavailable,
                        state: ProjectionState::Missing,
                        freshness: ProjectionFreshness::Unavailable,
                        workspace_id: Some(workspace_id.clone()),
                        canonical_revision: None,
                        projected_revision: None,
                        revision_lag: None,
                        repair_needed: false,
                        detail: format!("Failed to read projection state: {}", err),
                    })
                }
                Err(_busy) => SearchProjectionHealth {
                    level: HealthLevel::Degraded,
                    state: if search_index_ready {
                        ProjectionState::Ready
                    } else {
                        ProjectionState::Missing
                    },
                    freshness: if search_index_ready {
                        ProjectionFreshness::Lagging
                    } else {
                        ProjectionFreshness::RebuildRequired
                    },
                    workspace_id: Some(workspace_id.clone()),
                    canonical_revision: None,
                    projected_revision: None,
                    revision_lag: None,
                    repair_needed: true,
                    detail: "projection visibility temporarily unavailable because SQLite is busy"
                        .to_string(),
                },
            };

            if selected
                .as_ref()
                .is_none_or(|current| projection_rank(&projection) > projection_rank(current))
            {
                selected = Some(projection);
            }
        }

        selected.unwrap_or(SearchProjectionHealth {
            level: HealthLevel::Unavailable,
            state: ProjectionState::Missing,
            freshness: ProjectionFreshness::Unavailable,
            workspace_id: None,
            canonical_revision: None,
            projected_revision: None,
            revision_lag: None,
            repair_needed: false,
            detail: "no active workspace projections are visible to the dashboard".to_string(),
        })
    }
}

fn projection_rank(projection: &SearchProjectionHealth) -> (u8, i64, String) {
    let severity = match projection.freshness {
        ProjectionFreshness::RebuildRequired => 3,
        ProjectionFreshness::Lagging => 2,
        ProjectionFreshness::Current => 1,
        ProjectionFreshness::Unavailable => 0,
    };
    let lag = projection.revision_lag.unwrap_or(0);
    let workspace = projection.workspace_id.clone().unwrap_or_default();
    (severity, lag, workspace)
}

fn backend_label(backend: &EmbeddingBackend) -> String {
    match backend {
        EmbeddingBackend::Auto => "auto".to_string(),
        EmbeddingBackend::Sidecar => "sidecar".to_string(),
        EmbeddingBackend::Unresolved => "unresolved".to_string(),
        EmbeddingBackend::Invalid(value) => format!("invalid({value})"),
    }
}
