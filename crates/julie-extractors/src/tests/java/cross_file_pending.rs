//! Phase 4a.java — Java emits `StructuredPendingRelationship` for cross-
//! package class references (`import com.example.Other; new Other();`).
//!
//! Source under test: `fixtures/extraction/java/cross_file/Source.java`.

use crate::base::SymbolKind;
use crate::extract_canonical;
use std::path::Path;

#[test]
fn test_java_emits_structured_pending_for_cross_package_class() {
    let source = include_str!("../../../../../fixtures/extraction/java/cross_file/Source.java");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("Source.java", source, workspace_root)
        .expect("canonical Java extraction must succeed");

    let other = result
        .structured_pending_relationships
        .iter()
        .find(|p| p.target.terminal_name == "Other")
        .unwrap_or_else(|| {
            panic!(
                "expected structured pending for cross-package Other; got {} entries: {:#?}",
                result.structured_pending_relationships.len(),
                result.structured_pending_relationships
            )
        });

    assert!(
        other.pending.line_number > 0,
        "pending.line_number must reflect call site, not 0"
    );
    assert_eq!(
        other.pending.file_path, "Source.java",
        "pending.file_path must match the file under extraction"
    );
    assert!(
        other.caller_scope_symbol_id.is_some(),
        "caller_scope_symbol_id must point at enclosing method/class"
    );

    assert!(
        result
            .structured_pending_relationships
            .iter()
            .all(|p| p.target.terminal_name != "localHelper"),
        "intra-class localHelper call must not appear as structured pending; got: {:#?}",
        result.structured_pending_relationships
    );
}

#[test]
fn test_java_negative_local_helper_not_emitted_as_pending() {
    let source = include_str!("../../../../../fixtures/extraction/java/cross_file/Source.java");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("Source.java", source, workspace_root)
        .expect("canonical Java extraction must succeed");

    let local_helper_id = result
        .symbols
        .iter()
        .find(|s| s.name == "localHelper" && s.kind == SymbolKind::Method)
        .map(|s| s.id.clone())
        .expect("localHelper method symbol must exist");

    assert!(
        result
            .structured_pending_relationships
            .iter()
            .all(|p| p.pending.callee_name != "localHelper"
                && p.target.terminal_name != "localHelper"),
        "intra-class localHelper call leaked into structured pending"
    );
    assert!(!local_helper_id.is_empty());
}
