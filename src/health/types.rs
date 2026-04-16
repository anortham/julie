use serde::Serialize;

/// System readiness levels for graceful degradation on the query path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemStatus {
    /// No workspace or database available.
    NotReady,
    /// SQLite is available but the Tantivy projection is missing.
    SqliteOnly { symbol_count: i64 },
    /// SQLite and Tantivy are both available.
    FullyReady { symbol_count: i64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthLevel {
    Ready,
    Degraded,
    Unavailable,
}

impl HealthLevel {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ready => "READY",
            Self::Degraded => "DEGRADED",
            Self::Unavailable => "UNAVAILABLE",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DaemonLifecycleState {
    Direct,
    Serving,
    RestartPending,
}

impl DaemonLifecycleState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Direct => "DIRECT",
            Self::Serving => "SERVING",
            Self::RestartPending => "RESTART PENDING",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WatcherState {
    Local,
    SharedActive,
    SharedGrace,
    SharedIdle,
    Unavailable,
}

impl WatcherState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Local => "LOCAL",
            Self::SharedActive => "SHARED ACTIVE",
            Self::SharedGrace => "SHARED GRACE",
            Self::SharedIdle => "SHARED IDLE",
            Self::Unavailable => "UNAVAILABLE",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionState {
    Ready,
    Missing,
}

impl ProjectionState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ready => "READY",
            Self::Missing => "MISSING",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionFreshness {
    Current,
    Lagging,
    RebuildRequired,
    Unavailable,
}

impl ProjectionFreshness {
    pub fn label(self) -> &'static str {
        match self {
            Self::Current => "CURRENT",
            Self::Lagging => "LAGGING",
            Self::RebuildRequired => "REBUILD REQUIRED",
            Self::Unavailable => "UNAVAILABLE",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddingState {
    Initializing,
    Initialized,
    Degraded,
    Unavailable,
    NotInitialized,
}

impl EmbeddingState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Initializing => "INITIALIZING",
            Self::Initialized => "INITIALIZED",
            Self::Degraded => "DEGRADED",
            Self::Unavailable => "UNAVAILABLE",
            Self::NotInitialized => "NOT INITIALIZED",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ControlPlaneHealth {
    pub level: HealthLevel,
    pub daemon_state: DaemonLifecycleState,
    pub primary_workspace_id: Option<String>,
    pub watcher_state: WatcherState,
    pub watcher_ref_count: Option<usize>,
    pub watcher_grace_active: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CanonicalStoreHealth {
    pub level: HealthLevel,
    pub symbol_count: i64,
    pub file_count: i64,
    pub relationship_count: i64,
    pub embedding_count: i64,
    pub db_size_mb: f64,
    pub languages: Vec<String>,
    pub detail: String,
}

impl CanonicalStoreHealth {
    pub fn sqlite_status_label(&self) -> &'static str {
        match self.level {
            HealthLevel::Ready => "HEALTHY",
            HealthLevel::Degraded => "BUSY",
            HealthLevel::Unavailable => "NOT CONNECTED",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchProjectionHealth {
    pub level: HealthLevel,
    pub state: ProjectionState,
    pub freshness: ProjectionFreshness,
    pub workspace_id: Option<String>,
    pub canonical_revision: Option<i64>,
    pub projected_revision: Option<i64>,
    pub revision_lag: Option<i64>,
    pub repair_needed: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct IndexingHealth {
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
pub struct DataPlaneHealth {
    pub level: HealthLevel,
    pub canonical_store: CanonicalStoreHealth,
    pub search_projection: SearchProjectionHealth,
    pub indexing: IndexingHealth,
}

#[derive(Debug, Clone, Serialize)]
pub struct EmbeddingRuntimeHealth {
    pub level: HealthLevel,
    pub state: EmbeddingState,
    pub runtime: String,
    pub requested_backend: String,
    pub backend: String,
    pub device: String,
    pub accelerated: bool,
    pub detail: String,
    pub query_fallback: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimePlaneHealth {
    pub level: HealthLevel,
    pub embeddings: EmbeddingRuntimeHealth,
}

#[derive(Debug, Clone, Serialize)]
pub struct SystemHealthSnapshot {
    pub overall: HealthLevel,
    pub readiness: SystemStatus,
    pub control_plane: ControlPlaneHealth,
    pub data_plane: DataPlaneHealth,
    pub runtime_plane: RuntimePlaneHealth,
}
