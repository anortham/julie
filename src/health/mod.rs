// Centralized Health Check System
//
// Shared health vocabulary for tools, dashboard routes, and control-plane work.

mod checker;
mod data_plane;
mod embedding;
mod evaluation;
mod indexing;
mod projection;
mod report;
mod types;

pub use checker::HealthChecker;
pub(crate) use checker::PrimaryWorkspaceHealth;
pub(crate) use data_plane::build_data_plane;
pub(crate) use embedding::project_embedding_runtime;
pub(crate) use evaluation::overall_from_levels;
pub(crate) use evaluation::overall_from_planes;
pub(crate) use indexing::indexing_health;
pub(crate) use projection::search_projection_health_for_workspace;
pub use types::{
    CanonicalStoreHealth, ControlPlaneHealth, DaemonLifecycleState, DataPlaneHealth,
    EmbeddingRuntimeHealth, EmbeddingState, HealthLevel, IndexingHealth, ProjectionFreshness,
    ProjectionState, RuntimePlaneHealth, SearchProjectionHealth, SystemHealthSnapshot,
    SystemStatus, WatcherState,
};
