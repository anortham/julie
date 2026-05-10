//! Phase 4a.ruby — Ruby emits `StructuredPendingRelationship` for cross-
//! file calls (`require 'other'; OtherModule.do_thing`).

use crate::base::SymbolKind;
use crate::extract_canonical;
use std::path::Path;

#[test]
fn test_ruby_emits_structured_pending_for_cross_file_call() {
    let source = include_str!("../../../../../fixtures/extraction/ruby/cross_file/source.rb");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("source.rb", source, workspace_root)
        .expect("canonical Ruby extraction must succeed");

    let do_thing = result
        .structured_pending_relationships
        .iter()
        .find(|p| p.target.terminal_name == "do_thing")
        .unwrap_or_else(|| {
            panic!(
                "expected structured pending for cross-file do_thing; got {} entries: {:#?}",
                result.structured_pending_relationships.len(),
                result.structured_pending_relationships
            )
        });

    assert!(do_thing.pending.line_number > 0);
    assert_eq!(do_thing.pending.file_path, "source.rb");
    assert!(do_thing.caller_scope_symbol_id.is_some());

    assert!(
        result
            .structured_pending_relationships
            .iter()
            .all(|p| p.target.terminal_name != "local_helper"),
        "intra-class local_helper must not appear as structured pending"
    );
}

#[test]
fn test_ruby_negative_local_helper_not_emitted_as_pending() {
    let source = include_str!("../../../../../fixtures/extraction/ruby/cross_file/source.rb");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("source.rb", source, workspace_root)
        .expect("canonical Ruby extraction must succeed");

    let id = result
        .symbols
        .iter()
        .find(|s| s.name == "local_helper" && s.kind == SymbolKind::Method)
        .map(|s| s.id.clone())
        .expect("local_helper method symbol must exist");

    assert!(
        result
            .structured_pending_relationships
            .iter()
            .all(|p| p.pending.callee_name != "local_helper"
                && p.target.terminal_name != "local_helper"),
        "intra-class local_helper leaked into pending"
    );
    assert!(!id.is_empty());
}
