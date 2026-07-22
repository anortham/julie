//! Shared state for the dashboard HTTP server and SSE broadcast channel.

use std::sync::Arc;
use std::sync::RwLock;
use std::time::{Duration, Instant};

use julie_pipeline::indexing_core::web_edges::WEB_EDGES_PROJECTION_NAME;
use serde::Serialize;
use tokio::sync::broadcast;

use crate::dashboard::error_buffer::{ErrorBuffer, LogEntry};
use crate::embeddings::EmbeddingBackend;
use crate::embeddings::EmbeddingRuntimeStatus;
use crate::health::{
    EmbeddingRuntimeHealth, HealthLevel, ProjectionFreshness, ProjectionHealth, ProjectionState,
    SystemStatus, overall_from_planes, project_embedding_runtime,
};
use crate::registry::database::DaemonDatabase;
use crate::registry::embedding_service::EmbeddingService;
use crate::registry::lifecycle::{LifecyclePhase, LifecyclePhaseKind, ShutdownCause};
use crate::registry::session::{SessionPhaseCounts, SessionTracker};
use crate::search::projection::TANTIVY_PROJECTION_NAME;

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

pub use crate::registry::lifecycle::LifecyclePhaseKind as DashboardDaemonPhase;

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
    pub projections: Vec<ProjectionHealth>,
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

/// Shared state injected into every dashboard route handler.
///
/// Cheap to clone — all fields are either `Arc`-wrapped or `Copy`.
#[derive(Clone)]
pub struct DashboardState {
    sessions: Arc<SessionTracker>,
    daemon_db: Option<Arc<DaemonDatabase>>,
    action_csrf_token: Arc<String>,
    daemon_phase: Arc<RwLock<LifecyclePhase>>,
    start_time: Instant,
    error_buffer: ErrorBuffer,
    /// Recovery markers from a previous unclean daemon shutdown (A1.7).
    /// Surfaced through the `/api/status` endpoint until the operator
    /// clears them. Default empty.
    recovery_markers: Arc<Vec<crate::registry::shutdown::RecoveryMarker>>,
    /// Live reference to the daemon's shared embedding service. Stored as a
    /// reference (not a snapshot bool) so the dashboard reflects state
    /// transitions as the background init task progresses from
    /// `Initializing` -> `Ready` (or `Unavailable`) without needing a restart.
    /// `None` in test contexts that don't wire up a service; the
    /// `embedding_available` accessor returns `false` in that case.
    embedding_service: Option<Arc<EmbeddingService>>,
    tx: broadcast::Sender<DashboardEvent>,
}

impl DashboardState {
    /// Create a new `DashboardState`.
    ///
    /// Internally creates an `ErrorBuffer` with the given capacity and a
    /// broadcast channel with capacity 256. Pool fields were removed in
    /// Phase 3d.2b-ii (dashboard dead-but-compiling until 3d.3).
    pub fn new(
        sessions: Arc<SessionTracker>,
        daemon_db: Option<Arc<DaemonDatabase>>,
        daemon_phase: Arc<RwLock<LifecyclePhase>>,
        start_time: Instant,
        embedding_service: Option<Arc<EmbeddingService>>,
        error_buffer_capacity: usize,
    ) -> Self {
        let error_buffer = ErrorBuffer::new(error_buffer_capacity);
        let (tx, _rx) = broadcast::channel(256);
        Self {
            sessions,
            daemon_db,
            action_csrf_token: Arc::new(uuid::Uuid::new_v4().to_string()),
            daemon_phase,
            start_time,
            error_buffer,
            recovery_markers: Arc::new(Vec::new()),
            embedding_service,
            tx,
        }
    }

    /// Attach the `RecoveryMarker` list (from a previous unclean shutdown)
    /// that the dashboard `/api/status` route should surface.
    ///
    /// Empty `markers` is equivalent to "no markers": the `/status` endpoint
    /// will still report the field but with an empty array.
    pub fn with_recovery_markers(
        mut self,
        markers: Arc<Vec<crate::registry::shutdown::RecoveryMarker>>,
    ) -> Self {
        self.recovery_markers = markers;
        self
    }

    /// Snapshot of recovery markers visible to dashboard handlers.
    pub fn recovery_markers(&self) -> &[crate::registry::shutdown::RecoveryMarker] {
        &self.recovery_markers
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

    /// Current daemon phase as observed by the dashboard.
    pub fn daemon_phase_kind(&self) -> DashboardDaemonPhase {
        self.daemon_phase
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .kind()
    }

    /// Whether dashboard routes may start workspace mutations.
    pub fn accepts_workspace_actions(&self) -> bool {
        self.daemon_phase_kind() == DashboardDaemonPhase::Ready
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
        let daemon_phase_snapshot = *self.daemon_phase.read().unwrap_or_else(|p| p.into_inner());
        let daemon_phase = daemon_phase_snapshot.kind();
        let shutdown_cause = daemon_phase_snapshot.shutdown_cause();
        let session_phases = self.sessions.phase_counts();
        let workspace_pool_connected = false;

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
            } else if daemon_phase != LifecyclePhaseKind::Ready {
                HealthLevel::Degraded
            } else {
                HealthLevel::Ready
            },
            daemon_phase,
            shutdown_cause,
            active_sessions,
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
                    "; workspace pool detached"
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
        let projections = Self::projection_health();
        let indexing = self.indexing_health().await;

        let data_plane = DashboardDataPlaneHealth {
            level: overall_from_planes(data_plane_level, indexing.level, HealthLevel::Ready, false),
            readiness,
            workspace_count,
            active_workspace_count,
            session_count,
            ready_workspace_count,
            pending_workspace_count,
            other_workspace_count,
            symbol_count,
            file_count,
            projections,
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
        DashboardIndexingHealth {
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
        }
    }

    fn projection_health() -> Vec<ProjectionHealth> {
        [TANTIVY_PROJECTION_NAME, WEB_EDGES_PROJECTION_NAME]
            .into_iter()
            .map(|name| ProjectionHealth {
                name: name.to_string(),
                level: HealthLevel::Unavailable,
                state: ProjectionState::Missing,
                freshness: ProjectionFreshness::Unavailable,
                workspace_id: None,
                canonical_revision: None,
                projected_revision: None,
                revision_lag: None,
                repair_needed: false,
                detail: format!(
                    "{name} projection visibility unavailable because workspace pool is detached"
                ),
            })
            .collect()
    }
}

fn backend_label(backend: &EmbeddingBackend) -> String {
    match backend {
        EmbeddingBackend::Auto => "auto".to_string(),
        EmbeddingBackend::Sidecar => "sidecar".to_string(),
        EmbeddingBackend::Unresolved => "unresolved".to_string(),
        EmbeddingBackend::Invalid(value) => format!("invalid({value})"),
    }
}
