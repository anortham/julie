use super::*;

#[test]
fn test_find_symbol_by_name() {
    let (_dir, fixture) = fixture(&[("src/engine.rs", "pub fn process() {}\n")]);
    let snapshot = fixture.snapshot();

    let found = find_symbol(snapshot.graph(), "process", None);
    assert_eq!(found.len(), 1);
    let row = snapshot.graph().symbol(found[0]);
    assert_eq!(row.name, "process");
    assert_eq!(row.path, "src/engine.rs");
}

#[test]
fn test_find_symbol_filters_imports() {
    let (_dir, fixture) = fixture(&[
        ("src/engine.rs", "pub fn process() {}\n"),
        ("src/main.rs", "use crate::engine::process;\n"),
    ]);
    let snapshot = fixture.snapshot();

    let found = find_symbol(snapshot.graph(), "process", None);
    assert_eq!(found.len(), 1, "imports should be filtered out");
    assert_eq!(snapshot.graph().symbol(found[0]).kind, SymbolKind::Function);
}

#[test]
fn test_find_symbol_disambiguates_by_file() {
    let (_dir, fixture) = fixture(&[
        ("src/engine.rs", "pub fn handle() {}\n"),
        ("src/handler.rs", "pub fn handle() {}\n"),
    ]);
    let snapshot = fixture.snapshot();

    let found = find_symbol(snapshot.graph(), "handle", Some("handler"));
    assert_eq!(found.len(), 1, "should disambiguate by file");
    assert_eq!(snapshot.graph().symbol(found[0]).path, "src/handler.rs");
}

#[test]
fn test_find_symbol_not_found() {
    let (_dir, fixture) = fixture(&[("src/engine.rs", "pub fn process() {}\n")]);
    let snapshot = fixture.snapshot();

    let found = find_symbol(snapshot.graph(), "nonexistent", None);
    assert!(found.is_empty());
}

#[test]
fn test_find_symbol_qualified_name() {
    let (_dir, fixture) = fixture(&[(
        "src/engine.rs",
        "pub struct Engine;\nimpl Engine {\n    pub fn process(&self) {}\n}\npub struct Pipeline;\nimpl Pipeline {\n    pub fn process(&self) {}\n}\n",
    )]);
    let snapshot = fixture.snapshot();
    let graph = snapshot.graph();

    let found = find_symbol(graph, "Engine::process", None);
    assert_eq!(
        found.len(),
        1,
        "qualified name should resolve to exactly one symbol"
    );
    assert_eq!(graph.symbol(found[0]).path, "src/engine.rs");
    assert_eq!(graph.symbol(found[0]).span.start_line, 3);

    let found_dot = find_symbol(graph, "Pipeline.process", None);
    assert_eq!(found_dot.len(), 1);
    assert_eq!(graph.symbol(found_dot[0]).span.start_line, 7);

    let found_all = find_symbol(graph, "process", None);
    assert_eq!(found_all.len(), 2, "unqualified should still return both");
}

#[test]
fn test_find_symbol_qualified_name_uses_impl_type_metadata() {
    let (_dir, fixture) = fixture(&[(
        "src/engine.rs",
        "pub struct Worker;\nimpl Worker {\n    fn run(&self) {}\n}\n",
    )]);
    let snapshot = fixture.snapshot();

    let found = find_symbol(snapshot.graph(), "Worker::run", None);
    assert_eq!(
        found.len(),
        1,
        "qualified lookup should use impl_type_name metadata when parent_id is missing"
    );
    assert_eq!(snapshot.graph().symbol(found[0]).name, "run");
}
