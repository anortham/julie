use julie_extractors::RelationshipKind;

use super::*;

#[test]
fn test_target_workspace_reference_kind_filters_relationships() {
    let files = &[
        ("src/lib.rs", "pub struct Widget(pub u8);\n"),
        ("src/caller.rs", "fn make() {\n    Widget(1);\n}\n"),
        ("src/importer.rs", "use crate::Widget;\n"),
    ];

    let all = refs(files, "Widget", 100, None);
    assert!(
        all.references
            .iter()
            .any(|r| r.file_path == "src/importer.rs" && r.kind == RelationshipKind::Imports),
        "unfiltered lookup should list the import: {:?}",
        all.references
    );

    let calls = refs(files, "Widget", 100, Some("call"));
    assert!(
        calls
            .references
            .iter()
            .all(|r| r.file_path != "src/importer.rs"),
        "call filter should not return unfiltered import relationships"
    );
    assert!(
        calls
            .references
            .iter()
            .any(|r| r.file_path == "src/caller.rs"),
        "call filter should include src/caller.rs call ref"
    );
}

#[test]
fn test_target_workspace_reference_kind_filters_identifiers() {
    let files = &[
        ("src/lib.rs", "pub struct Config(pub u8);\n"),
        ("src/user.rs", "fn user() {\n    Config(1);\n}\n"),
        ("src/types.rs", "fn typed(c: Config) {}\n"),
    ];

    let calls = refs(files, "Config", 100, Some("call"));
    assert_eq!(
        calls.references.len(),
        1,
        "should find exactly 1 call identifier, got {}",
        calls.references.len()
    );
    assert_eq!(calls.references[0].file_path, "src/user.rs");

    let types = refs(files, "Config", 100, Some("type_usage"));
    assert_eq!(
        types.references.len(),
        1,
        "should find exactly 1 type_usage identifier"
    );
    assert_eq!(types.references[0].file_path, "src/types.rs");

    let all = refs(files, "Config", 100, None);
    assert_eq!(
        all.references.len(),
        2,
        "without filter, should find 2 identifier refs"
    );
}

#[test]
fn test_relationship_kind_filter_scopes_to_identifier_occurrence() {
    let found = refs(
        &[
            ("src/lib.rs", "pub struct Widget(pub u8);\n"),
            ("src/call_site.rs", "fn call_site() {\n    Widget(1);\n}\n"),
            ("src/type_site.rs", "fn type_site(w: Widget) {}\n"),
        ],
        "Widget",
        100,
        Some("call"),
    );

    assert_eq!(
        found.references.len(),
        1,
        "kind filter should only return the call occurrence, got {:?}",
        found.references
    );
    assert_eq!(found.references[0].kind, RelationshipKind::Calls);
    assert_eq!(found.references[0].file_path, "src/call_site.rs");
    assert_eq!(found.references[0].line_number, 2);
}
