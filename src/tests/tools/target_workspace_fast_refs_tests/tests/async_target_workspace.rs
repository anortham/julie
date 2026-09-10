use julie_extractors::{RelationshipKind, SymbolKind};

use super::*;

#[test]
fn test_find_references_in_target_workspace_accepts_limit_and_kind() {
    let found = refs(
        &[(
            "src/lib.rs",
            "pub fn compute(x: i32) -> i32 {\n    x * 2\n}\n\npub fn caller_one() {\n    let result = compute(5);\n}\n\npub fn caller_two() {\n    compute(10);\n}\n",
        )],
        "compute",
        10,
        None,
    );

    assert!(
        !found.definitions.is_empty(),
        "should find at least one definition for 'compute'"
    );
    assert_eq!(found.references.len(), 2);
    assert_eq!(found.source_names.len(), 2);
}

#[test]
fn test_find_references_in_target_workspace_parity() {
    let files = &[
        (
            "src/engine.rs",
            "pub struct Engine;\nimpl Engine {\n    pub fn process(&self) {}\n    pub fn run(&self) {\n        self.process();\n    }\n}\n",
        ),
        (
            "src/pipeline.rs",
            "pub struct Pipeline;\nimpl Pipeline {\n    pub fn process(&self) {}\n    pub fn run(&self) {\n        self.process();\n    }\n}\n",
        ),
        ("src/inner.rs", "pub struct Thing;\n"),
        (
            "src/imports.rs",
            "use crate::inner::Thing;\nfn take(t: Thing) {}\n",
        ),
    ];

    let engine = refs(files, "Engine::process", 10, None);
    assert_eq!(
        engine.definitions.len(),
        1,
        "qualified lookup should return one child definition"
    );
    assert_eq!(engine.definitions[0].file_path, "src/engine.rs");
    assert_eq!(
        engine.references.len(),
        1,
        "qualified lookup should keep only Engine::process refs: {:?}",
        engine.references
    );
    assert_eq!(
        engine.references[0].file_path, "src/engine.rs",
        "qualified lookup should not return the other parent's call"
    );

    let thing = refs(files, "Thing", 10, None);
    assert!(
        thing
            .definitions
            .iter()
            .all(|d| d.kind != SymbolKind::Import),
        "import symbols should not remain in definitions"
    );
    assert!(
        thing
            .references
            .iter()
            .any(|r| r.kind == RelationshipKind::Imports),
        "import symbols should become import references"
    );
    assert!(
        thing
            .references
            .iter()
            .any(|r| r.kind == RelationshipKind::Uses),
        "type_usage identifiers should map to RelationshipKind::Uses"
    );
}
