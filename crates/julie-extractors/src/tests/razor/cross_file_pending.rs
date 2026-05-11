//! Phase 4b.razor — Recipe B closure. Razor's structured pending queue
//! is intentionally empty: cross-file C# references emerge from the
//! embedded C# pipeline (not Razor's own pending path). This test locks
//! that classification by asserting absence over an @using-bearing
//! fixture so any future regression that starts emitting Razor pending
//! produces a visible failure.

use crate::extract_canonical;
use std::path::Path;

#[test]
fn razor_pending_relationships_handled_by_csharp_embed() {
    let source = include_str!("../../../../../fixtures/extraction/razor/cross_file/source.razor");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("source.razor", source, workspace_root)
        .expect("canonical Razor extraction must succeed");

    assert!(
        result.structured_pending_relationships.is_empty(),
        "Razor must not emit structured pending; cross-file refs flow through \
         the embedded C# pipeline. Got {} entries: {:#?}",
        result.structured_pending_relationships.len(),
        result.structured_pending_relationships
    );
    assert!(
        result.pending_relationships.is_empty(),
        "Razor must not emit legacy pending either. Got {} entries: {:#?}",
        result.pending_relationships.len(),
        result.pending_relationships
    );
}
