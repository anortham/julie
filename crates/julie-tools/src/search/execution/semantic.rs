use anyhow::Result;

use julie_index::snapshot::Snapshot;

use crate::search::backend::SearchBackend;
use crate::search::trace::SearchExecutionResult;

/// True when the snapshot carries symbol vectors for KNN search.
pub(crate) fn snapshot_has_embeddings(snapshot: &Snapshot) -> bool {
    !snapshot.vectors().is_empty()
}

/// Symbol search over the snapshot's vector set. Only reached when
/// [`snapshot_has_embeddings`] is true; the KNN and hybrid passes land with the
/// vector set itself.
pub(crate) fn run_symbol_backend_pass(
    backend: SearchBackend,
    _snapshot: &Snapshot,
) -> Result<SearchExecutionResult> {
    anyhow::bail!(
        "SEMANTICS_NOT_READY: backend={} needs symbol vectors, which this snapshot does not serve yet",
        backend.as_str()
    )
}
