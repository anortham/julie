//! src/workspace_runtime/continuation.rs
//! Continuation types, binding validation, and failure representations.

use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

pub const DEFAULT_CONTINUATION_TTL_SECS: u64 = 15 * 60; // 15 minutes
pub const MAX_SNAPSHOT_SIZE_BYTES: usize = 8 * 1024 * 1024; // 8 MiB
pub const MAX_WORKSPACE_CONTINUATION_BUDGET_BYTES: usize = 64 * 1024 * 1024; // 64 MiB

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContinuationBinding {
    pub workspace_id: String,
    pub tool: String,
    pub arguments_hash: String,
    pub generation: u64,
    pub source_hashes: String,
}

impl ContinuationBinding {
    pub fn new(
        workspace_id: impl Into<String>,
        tool: impl Into<String>,
        arguments_hash: impl Into<String>,
        generation: u64,
        source_hashes: impl Into<String>,
    ) -> Self {
        Self {
            workspace_id: workspace_id.into(),
            tool: tool.into(),
            arguments_hash: arguments_hash.into(),
            generation,
            source_hashes: source_hashes.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContinuationFailure {
    pub code: String,
    pub message: String,
    pub restart_command: Option<String>,
}

impl ContinuationFailure {
    pub fn invalid(message: impl Into<String>) -> Self {
        Self {
            code: "CONTINUATION_INVALID".to_string(),
            message: message.into(),
            restart_command: None,
        }
    }

    pub fn stale(message: impl Into<String>, restart_command: Option<String>) -> Self {
        Self {
            code: "CONTINUATION_STALE".to_string(),
            message: message.into(),
            restart_command,
        }
    }

    pub fn expired(message: impl Into<String>) -> Self {
        Self {
            code: "CONTINUATION_EXPIRED".to_string(),
            message: message.into(),
            restart_command: None,
        }
    }

    pub fn oversized(size: usize, max: usize) -> Self {
        Self {
            code: "PAYLOAD_OVERSIZED".to_string(),
            message: format!(
                "Snapshot payload exceeds maximum budget ({size} bytes > {max} bytes)"
            ),
            restart_command: None,
        }
    }

    pub fn code(&self) -> &str {
        &self.code
    }
}

impl fmt::Display for ContinuationFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ContinuationFailure {}

#[derive(Debug, Error)]
pub enum ContinuationError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Path containment violation: path {0} is outside index root {1}")]
    PathEscape(String, String),
    #[error("Snapshot payload exceeds maximum budget ({size} bytes > {max} bytes)")]
    OversizedPayload { size: usize, max: usize },
    #[error("Continuation failure: {0}")]
    Failure(#[from] ContinuationFailure),
}

/// Validates that a requested continuation binding matches stored snapshot state.
///
/// Callers retrieving paged rows via `spillover_get` are permitted to consume
/// snapshots created by any query tool (e.g. `fast_search`, `get_context`, `blast_radius`).
pub fn validate_handle_binding(
    stored: &ContinuationBinding,
    requested: &ContinuationBinding,
) -> Result<(), ContinuationFailure> {
    let tool_args_match = if requested.tool == "spillover_get" {
        true
    } else {
        stored.tool == requested.tool && stored.arguments_hash == requested.arguments_hash
    };

    if stored.workspace_id != requested.workspace_id || !tool_args_match {
        return Err(ContinuationFailure::invalid(format!(
            "Continuation binding mismatch: stored (ws={}, tool={}, args={}), requested (ws={}, tool={}, args={})",
            stored.workspace_id,
            stored.tool,
            stored.arguments_hash,
            requested.workspace_id,
            requested.tool,
            requested.arguments_hash
        )));
    }
    if stored.generation != requested.generation || stored.source_hashes != requested.source_hashes
    {
        let restart = format!("{}(...)", stored.tool);
        return Err(ContinuationFailure::stale(
            format!(
                "Continuation stale: generation or source state changed (stored gen={}, current gen={})",
                stored.generation, requested.generation
            ),
            Some(restart),
        ));
    }
    Ok(())
}

/// Strictly checks that a continuation token is a 64-character lowercase hex string.
pub fn is_valid_continuation_token(token: &str) -> bool {
    token.len() == 64
        && token
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

/// Generates a random 256-bit cryptographically secure hex token.
pub fn generate_continuation_token() -> String {
    let u1 = uuid::Uuid::new_v4();
    let u2 = uuid::Uuid::new_v4();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let combined = format!("{u1}:{u2}:{now}");
    blake3::hash(combined.as_bytes()).to_hex().to_string()
}

/// Computes a deterministic blake3 hash of serialized tool arguments.
pub fn compute_arguments_hash(arguments: &serde_json::Value) -> String {
    let canonical_str = serde_json::to_string(arguments).unwrap_or_default();
    blake3::hash(canonical_str.as_bytes()).to_hex().to_string()
}
