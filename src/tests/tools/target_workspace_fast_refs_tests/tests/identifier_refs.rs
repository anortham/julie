use std::collections::HashSet;

use julie_extractors::RelationshipKind;

use super::*;

#[test]
fn test_target_workspace_includes_identifier_refs() {
    let found = refs(
        &[
            ("src/lib.rs", "pub fn process() {}\n"),
            ("src/main.rs", &caller("main_entry", "process")),
            ("src/handler.rs", &caller("handle", "process")),
        ],
        "process",
        100,
        None,
    );

    assert_eq!(found.definitions.len(), 1);
    assert_eq!(
        found.references.len(),
        2,
        "should find 2 identifier-based refs, got {}",
        found.references.len()
    );
    let ref_files: HashSet<&str> = found
        .references
        .iter()
        .map(|r| r.file_path.as_str())
        .collect();
    assert!(ref_files.contains("src/main.rs"));
    assert!(ref_files.contains("src/handler.rs"));
}

#[test]
fn test_target_workspace_identifier_dedup_against_relationships() {
    let found = refs(
        &[
            ("src/lib.rs", "pub fn foo() {}\n"),
            ("src/caller.rs", &caller("some_caller", "foo")),
        ],
        "foo",
        100,
        None,
    );

    assert_eq!(
        found.references.len(),
        1,
        "should deduplicate: only 1 ref for same file:line, got {}",
        found.references.len()
    );
    assert_eq!(found.references[0].file_path, "src/caller.rs");
    assert_eq!(found.references[0].line_number, 2);
}

#[test]
fn test_target_workspace_identifier_dedup_against_definitions() {
    let found = refs(
        &[
            ("src/lib.rs", "pub fn bar() {} fn same_line() { bar(); }\n"),
            ("src/other.rs", &caller("other", "bar")),
        ],
        "bar",
        100,
        None,
    );

    assert_eq!(found.definitions.len(), 1);
    assert_eq!(
        found.references.len(),
        1,
        "should not include identifier at definition site, got {} refs",
        found.references.len()
    );
    assert_eq!(found.references[0].file_path, "src/other.rs");
}

#[test]
fn test_target_workspace_identifier_kind_conversion() {
    let found = refs(
        &[
            ("src/lib.rs", "pub struct Thing(pub u8);\n"),
            ("src/a.rs", "fn build() {\n    Thing(1);\n}\n"),
            ("src/b.rs", "use crate::Thing;\n"),
            ("src/c.rs", "fn take(t: Thing) {}\n"),
        ],
        "Thing",
        100,
        None,
    );
    assert_eq!(found.references.len(), 3, "should find 3 identifier refs");

    let call_ref = found
        .references
        .iter()
        .find(|r| r.file_path == "src/a.rs")
        .unwrap();
    assert_eq!(call_ref.kind, RelationshipKind::Calls);

    let import_ref = found
        .references
        .iter()
        .find(|r| r.file_path == "src/b.rs")
        .unwrap();
    assert_eq!(import_ref.kind, RelationshipKind::Imports);

    let type_ref = found
        .references
        .iter()
        .find(|r| r.file_path == "src/c.rs")
        .unwrap();
    assert_eq!(type_ref.kind, RelationshipKind::Uses);
}

#[test]
fn test_target_workspace_combined_limit_and_kind_filter() {
    let found = refs(
        &[
            ("src/lib.rs", "pub struct Handler(pub u8);\n"),
            ("src/a.rs", "fn a() {\n    Handler(1);\n}\n"),
            ("src/b.rs", "fn b() {\n    Handler(2);\n}\n"),
            ("src/c.rs", "fn c() {\n    Handler(3);\n}\n"),
            ("src/d.rs", "fn d(h: Handler) {}\n"),
        ],
        "Handler",
        2,
        Some("call"),
    );

    assert_eq!(
        found.references.len(),
        2,
        "should get exactly 2 call refs with limit=2"
    );
    for r in &found.references {
        assert_eq!(
            r.kind,
            RelationshipKind::Calls,
            "all refs should be Calls kind"
        );
    }
    assert!(found.references[0].confidence >= found.references[1].confidence);
}
