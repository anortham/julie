use std::fs;

use crate::deep_dive::data::{RefEntry, SymbolContext, build_symbol_context, find_symbol};
use crate::deep_dive::formatting::format_symbol_context;
use crate::deep_dive::{DeepDiveTool, deep_dive_query};
use julie_core::Symbol;
use julie_extractors::{RelationshipKind, SymbolKind, Visibility};
use julie_index::graph::SymbolId;
use julie_index::snapshot::Snapshot;
use julie_test_support::SnapshotFixture;
use tempfile::TempDir;

fn fixture(files: &[(&str, &str)]) -> (TempDir, SnapshotFixture) {
    let dir = TempDir::new().unwrap();
    for (path, content) in files {
        let full = dir.path().join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, content).unwrap();
    }
    let fixture = SnapshotFixture::from_tree(dir.path()).unwrap();
    (dir, fixture)
}

fn only(snapshot: &Snapshot, name: &str) -> SymbolId {
    let found = find_symbol(snapshot.graph(), name, None);
    assert_eq!(found.len(), 1, "expected one symbol named {name}");
    found[0]
}

fn make_symbol(
    id: &str,
    name: &str,
    kind: SymbolKind,
    file: &str,
    line: u32,
    parent_id: Option<&str>,
    signature: Option<&str>,
    visibility: Option<Visibility>,
    code_context: Option<&str>,
) -> Symbol {
    Symbol {
        extracted: julie_extractors::Symbol {
            id: id.to_string(),
            name: name.to_string(),
            kind,
            language: "rust".to_string(),
            file_path: file.to_string(),
            start_line: line,
            end_line: line + 10,
            start_column: 0,
            end_column: 0,
            start_byte: 0,
            end_byte: 100,
            parent_id: parent_id.map(|s| s.to_string()),
            signature: signature.map(|s| s.to_string()),
            doc_comment: None,
            visibility,
            metadata: None,
            semantic_group: None,
            confidence: Some(0.9),
            content_type: None,
            body_span: None,
            body_hash: None,
            annotations: Vec::new(),
        },
        code_context: code_context.map(|s| s.to_string()),
    }
}

fn make_ref(kind: RelationshipKind, file: &str, line: u32, sym: Option<Symbol>) -> RefEntry {
    RefEntry {
        kind,
        file_path: file.to_string(),
        line_number: line,
        symbol: sym,
    }
}

fn empty_context(symbol: Symbol) -> SymbolContext {
    SymbolContext {
        symbol,
        complexity: None,
        incoming: vec![],
        incoming_total: 0,
        incoming_calls_total: 0,
        outgoing: vec![],
        outgoing_total: 0,
        outgoing_calls_total: 0,
        children: vec![],
        implementations: vec![],
        test_refs: vec![],
        similar: vec![],
    }
}

#[test]
fn test_deep_dive_regression_rejects_invalid_depth() {
    let json = r#"{"symbol": "MyFunction", "depth": "verbose"}"#;
    let err = serde_json::from_str::<DeepDiveTool>(json)
        .expect_err("invalid depth should fail parameter deserialization");
    assert!(
        err.to_string().contains("verbose") || err.to_string().contains("depth"),
        "error should explain invalid depth, got: {err}"
    );
}

#[test]
fn test_deep_dive_regression_callable_counts_only_displayed_ref_kind() {
    let sym = make_symbol(
        "process",
        "process",
        SymbolKind::Function,
        "src/engine.rs",
        42,
        None,
        None,
        None,
        None,
    );
    let call_sym = make_symbol(
        "validate",
        "validate",
        SymbolKind::Function,
        "src/validate.rs",
        5,
        None,
        None,
        None,
        None,
    );
    let type_sym = make_symbol(
        "Order",
        "Order",
        SymbolKind::Struct,
        "src/order.rs",
        9,
        None,
        None,
        None,
        None,
    );
    let mut ctx = empty_context(sym);
    ctx.outgoing = vec![
        make_ref(
            RelationshipKind::Calls,
            "src/validate.rs",
            5,
            Some(call_sym),
        ),
        make_ref(
            RelationshipKind::Parameter,
            "src/order.rs",
            9,
            Some(type_sym.clone()),
        ),
        make_ref(RelationshipKind::Returns, "src/order.rs", 9, Some(type_sym)),
    ];
    ctx.outgoing_total = 3;

    let output = format_symbol_context(&ctx, "overview");
    assert!(
        output.contains("Callees (1):"),
        "callee count should not include type refs, got:\n{}",
        output
    );
    assert!(
        !output.contains("Callees (1 of 3):"),
        "callee count should not use mixed outgoing total, got:\n{}",
        output
    );
}

#[test]
fn test_deep_dive_regression_find_symbol_filters_exports() {
    let (_dir, fixture) = fixture(&[
        ("src/engine.rs", "pub fn process() {}\n"),
        ("src/main.rs", "pub use crate::engine::process;\n"),
    ]);
    let snapshot = fixture.snapshot();

    let found = find_symbol(snapshot.graph(), "process", None);
    assert_eq!(found.len(), 1, "exports should be filtered out");
    assert_eq!(snapshot.graph().symbol(found[0]).kind, SymbolKind::Function);
}

#[test]
fn test_deep_dive_regression_context_file_uses_path_suffix_matching() {
    let (_dir, fixture) = fixture(&[
        ("src/test.rs", "pub fn handle() {}\n"),
        ("src/contest.rs", "pub fn handle() {}\n"),
    ]);
    let snapshot = fixture.snapshot();
    let graph = snapshot.graph();

    let suffix_match = find_symbol(graph, "handle", Some("test.rs"));
    assert_eq!(suffix_match.len(), 1);
    assert_eq!(graph.symbol(suffix_match[0]).path, "src/test.rs");

    let absolute_match = find_symbol(graph, "handle", Some("/tmp/workspace/src/test.rs"));
    assert_eq!(absolute_match.len(), 1);
    assert_eq!(graph.symbol(absolute_match[0]).path, "src/test.rs");

    let typo_match = find_symbol(graph, "handle", Some("missing.rs"));
    assert!(
        typo_match.is_empty(),
        "context_file typos should not fall back to all candidates"
    );
}

#[test]
fn test_deep_dive_regression_same_line_outgoing_refs_keep_distinct_symbols() {
    let (_dir, fixture) = fixture(&[(
        "src/engine.rs",
        "pub fn process() {\n    validate(); transform();\n}\nfn validate() {}\nfn transform() {}\n",
    )]);
    let snapshot = fixture.snapshot();
    let source = only(&snapshot, "process");

    let ctx = build_symbol_context(&snapshot, source, "overview", 10, 10).unwrap();
    let names: Vec<&str> = ctx
        .outgoing
        .iter()
        .map(|r| r.symbol.as_ref().map(|s| s.name.as_str()).unwrap_or(""))
        .collect();

    assert_eq!(names, vec!["validate", "transform"]);
}

#[test]
fn test_deep_dive_regression_qualified_method_identifier_fallback_avoids_bare_name_noise() {
    let (_dir, fixture) = fixture(&[
        (
            "src/tokenizer.rs",
            "pub struct CodeTokenizer;\nimpl CodeTokenizer {\n    pub fn new() -> Self {\n        CodeTokenizer\n    }\n}\n",
        ),
        (
            "src/main.rs",
            "fn uses_tokenizer() {\n    let _tokenizer = CodeTokenizer::new();\n}\n",
        ),
        (
            "src/handler.rs",
            "pub struct Unrelated;\nimpl Unrelated {\n    pub fn new() -> Self {\n        Unrelated\n    }\n}\nfn uses_unrelated_new() {\n    let _unrelated = Unrelated::new();\n}\n",
        ),
    ]);
    let snapshot = fixture.snapshot();
    let constructor = only(&snapshot, "CodeTokenizer::new");

    let ctx = build_symbol_context(&snapshot, constructor, "overview", 10, 10).unwrap();
    let caller_names: Vec<&str> = ctx
        .incoming
        .iter()
        .map(|r| r.symbol.as_ref().map(|s| s.name.as_str()).unwrap_or(""))
        .collect();

    assert_eq!(caller_names, vec!["uses_tokenizer"]);
    assert_eq!(ctx.incoming_total, 1);
}

#[test]
fn test_deep_dive_regression_auto_select_requires_all_matches_in_one_file() {
    let (_dir, fixture) = fixture(&[
        (
            "include/foo.hpp",
            "class Foo {\npublic:\n    Foo() {}\n    Foo(int a) {}\n    Foo(double d) {}\n};\n",
        ),
        ("src/foo_adapter.rs", "pub fn Foo() {}\n"),
        ("src/foo_test.rs", "pub fn Foo() {}\n"),
        ("src/foo_generated.rs", "pub fn Foo() {}\n"),
    ]);
    let snapshot = fixture.snapshot();
    assert!(
        find_symbol(snapshot.graph(), "Foo", None).len() > 5,
        "the fixture must define more than five symbols named Foo"
    );

    let result = deep_dive_query(&snapshot, "Foo", None, "overview", 10, 10).unwrap();

    assert!(
        result.contains("Use context_file to disambiguate"),
        "cross-file matches should ask for disambiguation, got:\n{}",
        result
    );
    assert!(
        !result.contains("Auto-selected"),
        "cross-file matches should not auto-select, got:\n{}",
        result
    );
}
