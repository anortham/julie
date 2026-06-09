//! Health state enums and `SystemStatus` — pure value types with no upward deps.
//!
//! The compound snapshot types (`SystemHealthSnapshot`, `ControlPlaneHealth`,
//! etc.) live in the top-crate `src/health/types.rs` where they can carry
//! the `render_report` impl and other top-crate-specific logic.

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
}

impl DaemonLifecycleState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Direct => "DIRECT",
            Self::Serving => "SERVING",
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
