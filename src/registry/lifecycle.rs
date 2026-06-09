//! Daemon lifecycle management: coarse lifecycle phases for the dashboard.

use serde::Serialize;

/// Coarse daemon runtime phase used by the control plane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecyclePhase {
    Starting,
    Ready,
    Draining { cause: ShutdownCause },
    Stopping { cause: ShutdownCause },
}

impl LifecyclePhase {
    pub fn kind(self) -> LifecyclePhaseKind {
        match self {
            Self::Starting => LifecyclePhaseKind::Starting,
            Self::Ready => LifecyclePhaseKind::Ready,
            Self::Draining { .. } => LifecyclePhaseKind::Draining,
            Self::Stopping { .. } => LifecyclePhaseKind::Stopping,
        }
    }

    pub fn shutdown_cause(self) -> Option<ShutdownCause> {
        match self {
            Self::Starting | Self::Ready => None,
            Self::Draining { cause } | Self::Stopping { cause } => Some(cause),
        }
    }

    pub fn state_file_value(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Ready => "ready",
            Self::Draining { .. } => "draining",
            Self::Stopping { .. } => "stopping",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecyclePhaseKind {
    Starting,
    Ready,
    Draining,
    Stopping,
}

impl LifecyclePhaseKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Starting => "STARTING",
            Self::Ready => "READY",
            Self::Draining => "DRAINING",
            Self::Stopping => "STOPPING",
        }
    }
}

/// High-level shutdown cause. Specific restart reasons stay on accept-loop decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShutdownCause {
    Signal,
    StopCommand,
    RestartRequired,
}

impl ShutdownCause {
    pub fn label(self) -> &'static str {
        match self {
            Self::Signal => "SIGNAL",
            Self::StopCommand => "STOP COMMAND",
            Self::RestartRequired => "RESTART REQUIRED",
        }
    }
}
