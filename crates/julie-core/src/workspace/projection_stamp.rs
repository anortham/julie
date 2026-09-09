//! Revision stamps shared between the SQLite canonical store and the Tantivy projection.

use serde::{Deserialize, Serialize};

/// Point-in-time stamp identifying an atomic revision publication across stores.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicationStamp {
    pub epoch: u64,
    pub canonical_revision: u64,
    pub projected_revision: u64,
    pub generation: u64,
}

/// Metadata payload embedded into Tantivy commit via `PreparedCommit::set_payload`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TantivyCommitPayload {
    pub generation: u64,
    pub revision: u64,
    pub epoch: u64,
}

/// Verification state of disk files prior to canonical commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceCheckState {
    Verified { files_checked: usize },
    RecheckRequeued { requeued_count: usize },
    HashMismatch,
    Unchecked,
}

/// True when the Tantivy projection is behind the SQLite canonical revision.
pub fn needs_projection_recovery(canonical: u64, projected: Option<u64>) -> bool {
    match projected {
        Some(revision) => revision != canonical,
        None => true,
    }
}
