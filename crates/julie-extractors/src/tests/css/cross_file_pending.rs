//! Phase 4c.css — Recipe B closure. CSS @import directives resolve at
//! extraction time to direct relationship edges and CSS has no
//! forward-reference construct; structured pending is intentionally
//! empty. This test locks the no-pending classification so any future
//! regression that starts emitting CSS pending produces a visible
//! failure.

use crate::extract_canonical;
use std::path::Path;

#[test]
fn css_pending_relationships_intra_document_only() {
    let source = include_str!("../../../../../fixtures/extraction/css/cross_file/source.css");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("source.css", source, workspace_root)
        .expect("canonical CSS extraction must succeed");

    assert!(
        result.structured_pending_relationships.is_empty(),
        "CSS must not emit structured pending — @import resolves at extraction time. \
         Got {} entries: {:#?}",
        result.structured_pending_relationships.len(),
        result.structured_pending_relationships
    );
    assert!(
        result.pending_relationships.is_empty(),
        "CSS must not emit legacy pending either. Got {} entries: {:#?}",
        result.pending_relationships.len(),
        result.pending_relationships
    );
}
