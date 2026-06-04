//! Workspace resolution error types.
//!
//! Pure error-value types used by workspace resolution paths. Lives in
//! julie-core so both `utils::paths` and `tools::navigation::resolution`
//! can import from here without creating an upward edge.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceResolutionFailureKind {
    UnknownWorkspace,
    WorkspaceNotReady,
    PrimarySwapInProgress,
    AutoActivationFailed,
    /// The caller supplied a file path that resolves outside the workspace root.
    FileOutsideWorkspace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceResolutionFailure {
    kind: WorkspaceResolutionFailureKind,
    message: String,
}

impl WorkspaceResolutionFailure {
    pub fn new(kind: WorkspaceResolutionFailureKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn kind(&self) -> WorkspaceResolutionFailureKind {
        self.kind
    }
}

impl fmt::Display for WorkspaceResolutionFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for WorkspaceResolutionFailure {}

/// Downcast an `anyhow::Error` to `WorkspaceResolutionFailureKind`, if applicable.
pub fn workspace_resolution_failure_kind(
    error: &anyhow::Error,
) -> Option<WorkspaceResolutionFailureKind> {
    error
        .downcast_ref::<WorkspaceResolutionFailure>()
        .map(WorkspaceResolutionFailure::kind)
}
