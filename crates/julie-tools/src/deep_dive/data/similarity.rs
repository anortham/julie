use super::types::SimilarEntry;
use julie_index::snapshot::Snapshot;

/// Semantically similar symbols from the snapshot vector set. The set is
/// empty until KNN over snapshot vectors lands (Task 10), so the section is
/// empty; that task replaces this body.
pub(crate) fn build_similar(snapshot: &Snapshot) -> Vec<SimilarEntry> {
    debug_assert!(
        snapshot.vectors().is_empty(),
        "deep_dive similarity is not wired to the snapshot vector set yet"
    );
    Vec::new()
}
