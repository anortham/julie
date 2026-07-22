use serde::Serialize;

// State enums relocated to julie-core; re-exported here so all
// `crate::health::types::*` import sites continue to compile.
pub use julie_core::health_types::{
    DaemonLifecycleState, EmbeddingState, HealthLevel, ProjectionFreshness, ProjectionState,
    SystemStatus, WatcherState,
};

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
            HealthLevel::Unavailable => {
                if self.symbol_count > 0 && self.db_size_mb == 0.0 {
                    "MISSING ON DISK"
                } else {
                    "NOT CONNECTED"
                }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectionHealth {
    pub name: String,
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
    pub projections: Vec<ProjectionHealth>,
    pub indexing: IndexingHealth,
}

impl DataPlaneHealth {
    pub fn projection(&self, name: &str) -> Option<&ProjectionHealth> {
        self.projections
            .iter()
            .find(|projection| projection.name == name)
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn store_with(level: HealthLevel, symbol_count: i64, db_size_mb: f64) -> CanonicalStoreHealth {
        CanonicalStoreHealth {
            level,
            symbol_count,
            file_count: 0,
            relationship_count: 0,
            embedding_count: 0,
            db_size_mb,
            languages: Vec::new(),
            detail: String::new(),
        }
    }

    #[test]
    fn sqlite_status_label_returns_healthy_when_ready() {
        let store = store_with(HealthLevel::Ready, 1000, 50.0);
        assert_eq!(store.sqlite_status_label(), "HEALTHY");
    }

    #[test]
    fn sqlite_status_label_returns_busy_when_degraded() {
        let store = store_with(HealthLevel::Degraded, 1000, 50.0);
        assert_eq!(store.sqlite_status_label(), "BUSY");
    }

    #[test]
    fn sqlite_status_label_returns_not_connected_when_no_data() {
        let store = store_with(HealthLevel::Unavailable, 0, 0.0);
        assert_eq!(store.sqlite_status_label(), "NOT CONNECTED");
    }

    #[test]
    fn sqlite_status_label_flags_phantom_fd_state() {
        let store = store_with(HealthLevel::Unavailable, 37989, 0.0);
        assert_eq!(store.sqlite_status_label(), "MISSING ON DISK");
    }

    #[test]
    fn sqlite_status_label_does_not_flag_empty_workspace_as_missing() {
        let store = store_with(HealthLevel::Unavailable, 0, 0.0);
        assert_eq!(store.sqlite_status_label(), "NOT CONNECTED");
    }
}
