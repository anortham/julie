//! Primitives, configuration, errors, and I/O helpers for source edit operations.

pub use super::edit_journal::{EditDisposition, RecoveryAction};
use crate::request_engine::RequestFailure;
use julie_core::workspace::mutation_gate::{MutationGuard, acquire_gate};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

pub const DEFAULT_MAX_EDIT_SOURCE_BYTES: usize = 16 * 1024 * 1024;
pub const MIN_MAX_EDIT_SOURCE_BYTES: usize = 1;
pub const MAX_MAX_EDIT_SOURCE_BYTES: usize = 256 * 1024 * 1024;
pub const ENV_MAX_EDIT_SOURCE_BYTES: &str = "JULIE_MAX_EDIT_SOURCE_BYTES";

#[derive(Debug, Clone)]
pub struct SourceEditConfig {
    pub max_source_bytes: usize,
}

impl Default for SourceEditConfig {
    fn default() -> Self {
        let max_source_bytes = std::env::var(ENV_MAX_EDIT_SOURCE_BYTES)
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(DEFAULT_MAX_EDIT_SOURCE_BYTES)
            .clamp(MIN_MAX_EDIT_SOURCE_BYTES, MAX_MAX_EDIT_SOURCE_BYTES);

        Self { max_source_bytes }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreparedSourceChange {
    pub path: PathBuf,
    pub before_hash: String,
    pub after_bytes: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_bytes: Option<Vec<u8>>,
    #[serde(default)]
    pub is_ast_aware: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditPreview {
    pub file_path: PathBuf,
    pub old_text: String,
    pub new_text: String,
    pub diff: String,
    pub before_hash: String,
    pub after_bytes: Vec<u8>,
}

#[derive(Debug, Error)]
pub enum SourceEditError {
    #[error("EDIT_CONFLICT: File '{path}' disk hash '{actual_hash}' != expected '{expected_hash}'")]
    Conflict {
        path: PathBuf,
        expected_hash: String,
        actual_hash: String,
    },
    #[error("EDIT_BUSY: Timed out waiting for source-edit lock at '{path}' ({elapsed:?})")]
    Busy { path: PathBuf, elapsed: Duration },
    #[error(
        "SOURCE_TOO_LARGE: File '{path}' size ({bytes} bytes) exceeds limit ({max_bytes} bytes)"
    )]
    SourceTooLarge {
        path: PathBuf,
        bytes: usize,
        max_bytes: usize,
    },
    #[error("SOURCE_EDIT_UNAVAILABLE: Cannot access lock/journal directory: {reason}")]
    Unavailable { reason: String },
    #[error("EDIT_RECOVERY_CONFLICT: Conflict during recovery on edit '{edit_id}'")]
    RecoveryConflict {
        edit_id: String,
        disposition: Box<EditDisposition>,
    },
    #[error("SYNTAX_REGRESSION: AST edit introduced syntax errors in '{path}': {details}")]
    SyntaxRegression { path: PathBuf, details: String },
    #[error("RECOVERY_ACTION_CONFLICT: Active recovery '{active}' != requested '{requested}'")]
    RecoveryActionConflict {
        edit_id: String,
        active: String,
        requested: String,
    },
    #[error("CANCELLED: Source edit was cancelled")]
    Cancelled,
    #[error("DEADLINE_EXCEEDED: Request deadline exceeded during source edit")]
    DeadlineExceeded,
    #[error("INVALID_ARGUMENTS: {0}")]
    InvalidArguments(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl SourceEditError {
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::Conflict { .. } => "EDIT_CONFLICT",
            Self::Busy { .. } => "EDIT_BUSY",
            Self::SourceTooLarge { .. } => "SOURCE_TOO_LARGE",
            Self::Unavailable { .. } => "SOURCE_EDIT_UNAVAILABLE",
            Self::RecoveryConflict { .. } => "EDIT_RECOVERY_CONFLICT",
            Self::SyntaxRegression { .. } => "SYNTAX_REGRESSION",
            Self::RecoveryActionConflict { .. } => "RECOVERY_ACTION_CONFLICT",
            Self::Cancelled => "CANCELLED",
            Self::DeadlineExceeded => "DEADLINE_EXCEEDED",
            Self::InvalidArguments(_) => "INVALID_ARGUMENTS",
            Self::Io(_) => "IO_ERROR",
        }
    }

    pub fn to_request_failure(&self) -> RequestFailure {
        let (code, details) = match self {
            Self::Conflict {
                path,
                expected_hash,
                actual_hash,
            } => (
                "EDIT_CONFLICT",
                serde_json::json!({
                    "path": path.to_string_lossy(),
                    "expected_hash": expected_hash,
                    "actual_hash": actual_hash
                }),
            ),
            Self::Busy { path, elapsed } => (
                "EDIT_BUSY",
                serde_json::json!({
                    "path": path.to_string_lossy(),
                    "elapsed_ms": elapsed.as_millis()
                }),
            ),
            Self::SourceTooLarge {
                path,
                bytes,
                max_bytes,
            } => (
                "SOURCE_TOO_LARGE",
                serde_json::json!({
                    "path": path.to_string_lossy(),
                    "bytes": bytes,
                    "max_bytes": max_bytes
                }),
            ),
            Self::Unavailable { reason } => (
                "SOURCE_EDIT_UNAVAILABLE",
                serde_json::json!({ "reason": reason }),
            ),
            Self::RecoveryConflict {
                edit_id,
                disposition,
            } => (
                "EDIT_RECOVERY_CONFLICT",
                serde_json::json!({ "edit_id": edit_id, "details": disposition }),
            ),
            Self::SyntaxRegression { path, details } => (
                "SYNTAX_REGRESSION",
                serde_json::json!({ "path": path.to_string_lossy(), "details": details }),
            ),
            Self::RecoveryActionConflict {
                edit_id,
                active,
                requested,
            } => (
                "RECOVERY_ACTION_CONFLICT",
                serde_json::json!({
                    "edit_id": edit_id,
                    "active": active,
                    "requested": requested
                }),
            ),
            Self::Cancelled => ("CANCELLED", serde_json::json!({})),
            Self::DeadlineExceeded => ("DEADLINE_EXCEEDED", serde_json::json!({})),
            Self::InvalidArguments(msg) => {
                ("INVALID_ARGUMENTS", serde_json::json!({ "message": msg }))
            }
            Self::Io(err) => ("IO_ERROR", serde_json::json!({ "error": err.to_string() })),
        };
        RequestFailure::new(code, self.to_string(), false, details)
    }
}

pub fn read_bounded_source(
    path: &Path,
    max_bytes: usize,
    deadline: Option<Instant>,
    cancellation: Option<&CancellationToken>,
) -> Result<Vec<u8>, SourceEditError> {
    if let Some(c) = cancellation {
        if c.is_cancelled() {
            return Err(SourceEditError::Cancelled);
        }
    }
    if let Some(d) = deadline {
        if Instant::now() >= d {
            return Err(SourceEditError::DeadlineExceeded);
        }
    }

    let file = File::open(path)?;
    let metadata = file.metadata()?;
    let file_len = metadata.len() as usize;

    if file_len > max_bytes {
        return Err(SourceEditError::SourceTooLarge {
            path: path.to_path_buf(),
            bytes: file_len,
            max_bytes,
        });
    }

    let mut buf = Vec::with_capacity(file_len);
    let mut take = file.take((max_bytes + 1) as u64);
    let mut chunk = [0u8; 64 * 1024];

    loop {
        if let Some(c) = cancellation {
            if c.is_cancelled() {
                return Err(SourceEditError::Cancelled);
            }
        }
        if let Some(d) = deadline {
            if Instant::now() >= d {
                return Err(SourceEditError::DeadlineExceeded);
            }
        }

        let n = take.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.len() > max_bytes {
            return Err(SourceEditError::SourceTooLarge {
                path: path.to_path_buf(),
                bytes: buf.len(),
                max_bytes,
            });
        }
    }

    Ok(buf)
}

pub async fn acquire_source_edit_lock(
    lock_path: &Path,
    deadline: Instant,
    cancellation: &CancellationToken,
) -> Result<MutationGuard<'static>, SourceEditError> {
    if cancellation.is_cancelled() {
        return Err(SourceEditError::Cancelled);
    }
    let start = Instant::now();
    let key = lock_path.to_string_lossy().into_owned();
    tokio::select! {
        acquired = tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), acquire_gate(&key)) => {
            acquired.map_err(|_| SourceEditError::Busy {
                path: lock_path.to_path_buf(),
                elapsed: start.elapsed(),
            })
        }
        _ = cancellation.cancelled() => Err(SourceEditError::Cancelled),
    }
}

pub fn validate_ast_changes<F>(
    changes: &[PreparedSourceChange],
    resolve_path: F,
    deadline: Instant,
) -> Result<(), SourceEditError>
where
    F: Fn(&Path) -> Result<PathBuf, SourceEditError>,
{
    for change in changes {
        if !change.is_ast_aware {
            continue;
        }
        let full_path = resolve_path(&change.path)?;
        let proposed_str = match std::str::from_utf8(&change.after_bytes) {
            Ok(s) => s,
            Err(_) => {
                return Err(SourceEditError::InvalidArguments(
                    "Non-UTF-8 bytes in AST edit".into(),
                ));
            }
        };
        let adapter = julie_tools::editing::syntax::SyntaxAdapter::new(
            julie_tools::editing::syntax::SyntaxConfig::default(),
        )
        .map_err(|e| SourceEditError::SyntaxRegression {
            path: change.path.clone(),
            details: e.to_string(),
        })?;
        let parsed = match adapter.parse_source(&full_path, proposed_str, Some(deadline), None) {
            Ok(p) => p,
            Err(julie_tools::editing::syntax::SyntaxAdapterError::UnsupportedLanguage {
                ..
            }) => {
                continue;
            }
            Err(e) => {
                return Err(SourceEditError::SyntaxRegression {
                    path: change.path.clone(),
                    details: e.to_string(),
                });
            }
        };

        let old_errors = if let Some(ref b) = change.before_bytes {
            if let Ok(old_s) = std::str::from_utf8(b) {
                adapter
                    .parse_source(&full_path, old_s, Some(deadline), None)
                    .map(|p| {
                        p.diagnostics
                            .iter()
                            .filter(|d| {
                                matches!(
                                    d.kind,
                                    julie_extractors::ParseDiagnosticKind::Error
                                        | julie_extractors::ParseDiagnosticKind::Missing
                                )
                            })
                            .count()
                    })
                    .unwrap_or(0)
            } else {
                0
            }
        } else {
            0
        };

        let new_errors = parsed
            .diagnostics
            .iter()
            .filter(|d| {
                matches!(
                    d.kind,
                    julie_extractors::ParseDiagnosticKind::Error
                        | julie_extractors::ParseDiagnosticKind::Missing
                )
            })
            .count();

        if new_errors > old_errors {
            let msg = parsed
                .diagnostics
                .iter()
                .find(|d| {
                    matches!(
                        d.kind,
                        julie_extractors::ParseDiagnosticKind::Error
                            | julie_extractors::ParseDiagnosticKind::Missing
                    )
                })
                .and_then(|d| d.message.clone())
                .unwrap_or_else(|| "Syntax regression introduced in AST edit".to_string());
            return Err(SourceEditError::SyntaxRegression {
                path: change.path.clone(),
                details: msg,
            });
        }
    }
    Ok(())
}
