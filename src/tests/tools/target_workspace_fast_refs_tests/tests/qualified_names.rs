use super::*;

const ENGINE: &str = "pub struct Engine;\nimpl Engine {\n    pub fn process(&self) {}\n}\n";
const PIPELINE: &str = "pub struct Pipeline;\nimpl Pipeline {\n    pub fn process(&self) {}\n}\n";

#[test]
fn test_fast_refs_qualified_name_filters_by_parent() {
    let files = &[("src/engine.rs", ENGINE), ("src/pipeline.rs", PIPELINE)];

    let unqualified = refs(files, "process", 10, None).definitions;
    assert_eq!(
        unqualified.len(),
        2,
        "unqualified 'process' should find both methods, got {}",
        unqualified.len()
    );

    let engine_defs = refs(files, "Engine::process", 10, None).definitions;
    assert_eq!(
        engine_defs.len(),
        1,
        "Engine::process should find exactly 1 definition, got {}",
        engine_defs.len()
    );
    assert_eq!(engine_defs[0].name, "process");
    assert_eq!(engine_defs[0].file_path, "src/engine.rs");

    let pipeline_defs = refs(files, "Pipeline::process", 10, None).definitions;
    assert_eq!(
        pipeline_defs.len(),
        1,
        "Pipeline::process should find exactly 1 definition, got {}",
        pipeline_defs.len()
    );
    assert_eq!(pipeline_defs[0].file_path, "src/pipeline.rs");

    let unknown_defs = refs(files, "Unknown::process", 10, None).definitions;
    assert_eq!(
        unknown_defs.len(),
        0,
        "Unknown::process should find nothing, got {}",
        unknown_defs.len()
    );
}

#[test]
fn test_fast_refs_qualified_dot_separator() {
    let found = refs(
        &[(
            "src/service.rs",
            "pub struct Service;\nimpl Service {\n    pub fn run(&self) {}\n}\n",
        )],
        "Service.run",
        10,
        None,
    );
    assert_eq!(
        found.definitions.len(),
        1,
        "Service.run should find 1 definition, got {}",
        found.definitions.len()
    );
    assert_eq!(found.definitions[0].name, "run");
    assert_eq!(found.definitions[0].file_path, "src/service.rs");
}
