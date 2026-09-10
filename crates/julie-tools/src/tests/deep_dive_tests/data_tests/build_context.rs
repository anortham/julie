use super::*;

const ENGINE: &str = "pub fn process() {}\n";
const MAIN_CALLING_PROCESS_AT_LINE_8: &str =
    "fn main() {\n    let ready = true;\n    let _ = ready;\n    process();\n}\n";

#[test]
fn test_build_context_with_incoming_relationships() {
    let (_dir, fixture) = fixture(&[
        ("src/engine.rs", ENGINE),
        ("src/main.rs", &at_line(5, MAIN_CALLING_PROCESS_AT_LINE_8)),
    ]);
    let snapshot = fixture.snapshot();
    let target = only(&snapshot, "process");

    let ctx = build_symbol_context(&snapshot, target, "overview", 10, 10).unwrap();

    assert_eq!(ctx.incoming.len(), 1);
    assert_eq!(ctx.incoming_total, 1);
    assert_eq!(ctx.incoming[0].file_path, "src/main.rs");
    assert_eq!(ctx.incoming[0].line_number, 8);
    assert!(
        ctx.incoming[0].symbol.is_some(),
        "overview should still enrich refs for symbol names"
    );
    assert_eq!(ctx.incoming[0].symbol.as_ref().unwrap().name, "main");
}

#[test]
fn test_build_context_enriches_at_context_depth() {
    let (_dir, fixture) = fixture(&[
        ("src/engine.rs", ENGINE),
        ("src/main.rs", &at_line(5, MAIN_CALLING_PROCESS_AT_LINE_8)),
    ]);
    let snapshot = fixture.snapshot();
    let target = only(&snapshot, "process");

    let ctx = build_symbol_context(&snapshot, target, "context", 15, 15).unwrap();

    assert_eq!(ctx.incoming.len(), 1);
    assert!(
        ctx.incoming[0].symbol.is_some(),
        "context depth should enrich refs"
    );
    let enriched = ctx.incoming[0].symbol.as_ref().unwrap();
    assert_eq!(enriched.name, "main");
    assert_eq!(enriched.signature.as_deref(), Some("fn main()"));
}

#[test]
fn test_build_context_with_outgoing_relationships() {
    let (_dir, fixture) = fixture(&[(
        "src/engine.rs",
        "pub fn process() {\n    validate();\n}\n\nfn validate() {}\n",
    )]);
    let snapshot = fixture.snapshot();
    let source = only(&snapshot, "process");

    let ctx = build_symbol_context(&snapshot, source, "overview", 10, 10).unwrap();

    assert_eq!(ctx.outgoing.len(), 1);
    assert_eq!(ctx.outgoing_total, 1);
    assert_eq!(ctx.outgoing[0].file_path, "src/engine.rs");
}

#[test]
fn test_build_context_with_children() {
    let (_dir, fixture) = fixture(&[(
        "src/engine.rs",
        "pub trait UserService {\n    fn users(&self);\n    fn get_user(&self);\n}\n",
    )]);
    let snapshot = fixture.snapshot();
    let parent = only(&snapshot, "UserService");

    let ctx = build_symbol_context(&snapshot, parent, "overview", 10, 10).unwrap();

    assert_eq!(ctx.children.len(), 2, "should have 2 children");
    assert_eq!(ctx.children[0].name, "users");
    assert_eq!(ctx.children[1].name, "get_user");
}

#[test]
fn test_build_context_non_container_has_no_children() {
    let (_dir, fixture) = fixture(&[("src/engine.rs", ENGINE)]);
    let snapshot = fixture.snapshot();
    let function = only(&snapshot, "process");

    let ctx = build_symbol_context(&snapshot, function, "overview", 10, 10).unwrap();

    assert!(
        ctx.children.is_empty(),
        "functions should not query for children"
    );
}
