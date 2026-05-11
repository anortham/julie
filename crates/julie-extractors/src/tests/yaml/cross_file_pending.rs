//! Phase 4d.yaml — Recipe B closure. YAML anchors/aliases are
//! intra-document only; there is no cross-document forward-reference
//! construct. Structured pending is intentionally empty.

use crate::extract_canonical;
use std::path::Path;

#[test]
fn yaml_pending_relationships_intra_document_only() {
    let source = include_str!("../../../../../fixtures/extraction/yaml/cross_file/source.yaml");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("source.yaml", source, workspace_root)
        .expect("canonical YAML extraction must succeed");

    assert!(
        result.structured_pending_relationships.is_empty(),
        "YAML must not emit structured pending — anchors/aliases are intra-document. \
         Got {} entries: {:#?}",
        result.structured_pending_relationships.len(),
        result.structured_pending_relationships
    );
    assert!(
        result.pending_relationships.is_empty(),
        "YAML must not emit legacy pending either. Got {} entries: {:#?}",
        result.pending_relationships.len(),
        result.pending_relationships
    );
}
