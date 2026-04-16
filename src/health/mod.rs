// Centralized Health Check System
//
// Shared health vocabulary for tools, dashboard routes, and control-plane work.

mod checker;
mod embedding;
mod evaluation;
mod report;
mod types;

pub use checker::HealthChecker;
pub(crate) use checker::PrimaryWorkspaceHealth;
pub(crate) use embedding::project_embedding_runtime;
pub use types::{
    CanonicalStoreHealth, ControlPlaneHealth, DaemonLifecycleState, DataPlaneHealth,
    EmbeddingRuntimeHealth, EmbeddingState, HealthLevel, IndexingHealth, ProjectionFreshness,
    ProjectionState, RuntimePlaneHealth, SearchProjectionHealth, SystemHealthSnapshot,
    SystemStatus, WatcherState,
};
