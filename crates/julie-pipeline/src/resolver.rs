//! Cross-file relationship resolution now happens in the in-memory graph.

use julie_extractors::{PendingRelationship, Relationship, StructuredPendingRelationship};

#[derive(Debug, Default, Clone)]
pub struct ResolutionStats {
    pub total: usize,
    pub resolved: usize,
    pub no_candidates: usize,
    pub no_valid_candidates: usize,
    pub lookup_errors: usize,
}

impl ResolutionStats {
    pub fn log_summary(&self) {}
}

pub fn resolve_batch(_pendings: &[PendingRelationship]) -> (Vec<Relationship>, ResolutionStats) {
    (Vec::new(), ResolutionStats::default())
}

pub fn resolve_structured_batch(
    _pendings: &[StructuredPendingRelationship],
) -> (Vec<Relationship>, ResolutionStats) {
    (Vec::new(), ResolutionStats::default())
}
