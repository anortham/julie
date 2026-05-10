//! Phase 4c.regex — Recipe B closure. Regex backreferences are
//! within-pattern only; structured pending is intentionally empty.
//! This test locks the no-pending classification so any future
//! regression that starts emitting regex pending produces a visible
//! failure.

use crate::extract_canonical;
use std::path::Path;

#[test]
fn regex_pending_relationships_within_pattern_only() {
    let source = include_str!("../../../../../fixtures/extraction/regex/cross_file/source.regex");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("source.regex", source, workspace_root)
        .expect("canonical regex extraction must succeed");

    assert!(
        result.structured_pending_relationships.is_empty(),
        "Regex must not emit structured pending — backrefs are intra-pattern. \
         Got {} entries: {:#?}",
        result.structured_pending_relationships.len(),
        result.structured_pending_relationships
    );
    assert!(
        result.pending_relationships.is_empty(),
        "Regex must not emit legacy pending either. Got {} entries: {:#?}",
        result.pending_relationships.len(),
        result.pending_relationships
    );
}
