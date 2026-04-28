use crate::database::{FileInfo, SymbolDatabase};
use crate::extractors::base::{Relationship, RelationshipKind, Symbol, SymbolKind, Visibility};
use crate::tools::deep_dive::data::{RefEntry, SymbolContext, build_symbol_context, find_symbol};
use crate::tools::deep_dive::formatting::format_symbol_context;
use crate::tools::deep_dive::{DeepDiveTool, deep_dive_query};
use tempfile::TempDir;

fn setup_db() -> (TempDir, SymbolDatabase) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = SymbolDatabase::new(&db_path).unwrap();

    for file in &[
        "src/engine.rs",
        "src/main.rs",
        "src/handler.rs",
        "src/tests/search_tests.rs",
    ] {
        store_file(&db, file);
    }

    (temp_dir, db)
}

fn store_file(db: &SymbolDatabase, path: &str) {
    db.store_file_info(&FileInfo {
        path: path.to_string(),
        language: "rust".to_string(),
        hash: format!("hash_{}", path),
        size: 500,
        last_modified: 1000000,
        last_indexed: 0,
        symbol_count: 2,
        line_count: 0,
        content: None,
    })
    .unwrap();
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
        code_context: code_context.map(|s| s.to_string()),
        content_type: None,
        annotations: Vec::new(),
    }
}

fn make_rel(
    id: &str,
    from: &str,
    to: &str,
    kind: RelationshipKind,
    file: &str,
    line: u32,
) -> Relationship {
    Relationship {
        id: id.to_string(),
        from_symbol_id: from.to_string(),
        to_symbol_id: to.to_string(),
        kind,
        file_path: file.to_string(),
        line_number: line,
        confidence: 0.9,
        metadata: None,
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

fn insert_identifier(
    db: &SymbolDatabase,
    name: &str,
    kind: &str,
    file: &str,
    line: u32,
    containing_symbol_id: Option<&str>,
) {
    db.conn.execute(
        "INSERT INTO identifiers (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, containing_symbol_id, confidence)
         VALUES (?1, ?2, ?3, 'rust', ?4, ?5, 0, ?5, 10, 0, 100, ?6, 0.9)",
        rusqlite::params![
            format!("ident_{}_{}", name, line),
            name,
            kind,
            file,
            line,
            containing_symbol_id,
        ],
    ).unwrap();
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
    let (_tmp, mut db) = setup_db();

    let symbols = vec![
        make_symbol(
            "sym-def",
            "process",
            SymbolKind::Function,
            "src/engine.rs",
            10,
            None,
            None,
            None,
            None,
        ),
        make_symbol(
            "sym-export",
            "process",
            SymbolKind::Export,
            "src/main.rs",
            1,
            None,
            None,
            None,
            None,
        ),
    ];
    db.store_symbols(&symbols).unwrap();

    let found = find_symbol(&db, "process", None).unwrap();
    assert_eq!(found.len(), 1, "exports should be filtered out");
    assert_eq!(found[0].kind, SymbolKind::Function);
}

#[test]
fn test_deep_dive_regression_context_file_uses_path_suffix_matching() {
    let (_tmp, mut db) = setup_db();
    store_file(&db, "src/test.rs");
    store_file(&db, "src/contest.rs");

    let symbols = vec![
        make_symbol(
            "sym-test",
            "handle",
            SymbolKind::Function,
            "src/test.rs",
            10,
            None,
            None,
            None,
            None,
        ),
        make_symbol(
            "sym-contest",
            "handle",
            SymbolKind::Function,
            "src/contest.rs",
            20,
            None,
            None,
            None,
            None,
        ),
    ];
    db.store_symbols(&symbols).unwrap();

    let suffix_match = find_symbol(&db, "handle", Some("test.rs")).unwrap();
    assert_eq!(suffix_match.len(), 1);
    assert_eq!(suffix_match[0].file_path, "src/test.rs");

    let absolute_match = find_symbol(&db, "handle", Some("/tmp/workspace/src/test.rs")).unwrap();
    assert_eq!(absolute_match.len(), 1);
    assert_eq!(absolute_match[0].file_path, "src/test.rs");

    let typo_match = find_symbol(&db, "handle", Some("missing.rs")).unwrap();
    assert!(
        typo_match.is_empty(),
        "context_file typos should not fall back to all candidates"
    );
}

#[test]
fn test_deep_dive_regression_same_line_outgoing_refs_keep_distinct_symbols() {
    let (_tmp, mut db) = setup_db();

    let symbols = vec![
        make_symbol(
            "sym-source",
            "process",
            SymbolKind::Function,
            "src/engine.rs",
            10,
            None,
            None,
            None,
            None,
        ),
        make_symbol(
            "sym-validate",
            "validate",
            SymbolKind::Function,
            "src/engine.rs",
            50,
            None,
            Some("fn validate()"),
            None,
            None,
        ),
        make_symbol(
            "sym-transform",
            "transform",
            SymbolKind::Function,
            "src/engine.rs",
            60,
            None,
            Some("fn transform()"),
            None,
            None,
        ),
    ];
    db.store_symbols(&symbols).unwrap();

    let rels = vec![
        make_rel(
            "rel-validate",
            "sym-source",
            "sym-validate",
            RelationshipKind::Calls,
            "src/engine.rs",
            15,
        ),
        make_rel(
            "rel-transform",
            "sym-source",
            "sym-transform",
            RelationshipKind::Calls,
            "src/engine.rs",
            15,
        ),
    ];
    db.store_relationships(&rels).unwrap();

    let ctx = build_symbol_context(&db, &symbols[0], "overview", 10, 10).unwrap();
    let names: Vec<&str> = ctx
        .outgoing
        .iter()
        .map(|r| r.symbol.as_ref().map(|s| s.name.as_str()).unwrap_or(""))
        .collect();

    assert_eq!(names, vec!["validate", "transform"]);
}

#[test]
fn test_deep_dive_regression_qualified_method_identifier_fallback_avoids_bare_name_noise() {
    let (_tmp, mut db) = setup_db();
    store_file(&db, "src/tokenizer.rs");

    let symbols = vec![
        make_symbol(
            "sym-tokenizer",
            "CodeTokenizer",
            SymbolKind::Struct,
            "src/tokenizer.rs",
            20,
            None,
            Some("pub struct CodeTokenizer"),
            Some(Visibility::Public),
            None,
        ),
        make_symbol(
            "sym-new",
            "new",
            SymbolKind::Method,
            "src/tokenizer.rs",
            42,
            Some("sym-tokenizer"),
            Some("pub fn new() -> Self"),
            Some(Visibility::Public),
            None,
        ),
        make_symbol(
            "sym-qualified-caller",
            "uses_tokenizer",
            SymbolKind::Function,
            "src/main.rs",
            10,
            None,
            Some("fn uses_tokenizer()"),
            None,
            None,
        ),
        make_symbol(
            "sym-noisy-caller",
            "uses_unrelated_new",
            SymbolKind::Function,
            "src/handler.rs",
            30,
            None,
            Some("fn uses_unrelated_new()"),
            None,
            None,
        ),
    ];
    db.store_symbols(&symbols).unwrap();

    insert_identifier(
        &db,
        "CodeTokenizer::new",
        "call",
        "src/main.rs",
        12,
        Some("sym-qualified-caller"),
    );
    insert_identifier(
        &db,
        "new",
        "call",
        "src/handler.rs",
        35,
        Some("sym-noisy-caller"),
    );

    let ctx = build_symbol_context(&db, &symbols[1], "overview", 10, 10).unwrap();
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
    let (_tmp, mut db) = setup_db();

    for file in &[
        "include/foo.hpp",
        "src/foo_adapter.rs",
        "src/foo_test.rs",
        "src/foo_generated.rs",
    ] {
        store_file(&db, file);
    }

    let mut symbols = vec![make_symbol(
        "sym-foo-class",
        "Foo",
        SymbolKind::Class,
        "include/foo.hpp",
        77,
        None,
        Some("class Foo"),
        Some(Visibility::Public),
        None,
    )];
    for i in 0..3 {
        symbols.push(make_symbol(
            &format!("sym-foo-ctor-{}", i),
            "Foo",
            SymbolKind::Function,
            "include/foo.hpp",
            100 + i * 20,
            Some("sym-foo-class"),
            Some(&format!("Foo(arg{})", i)),
            Some(Visibility::Public),
            None,
        ));
    }
    for (i, file) in [
        "src/foo_adapter.rs",
        "src/foo_test.rs",
        "src/foo_generated.rs",
    ]
    .iter()
    .enumerate()
    {
        symbols.push(make_symbol(
            &format!("sym-foo-other-{}", i),
            "Foo",
            SymbolKind::Function,
            file,
            10,
            None,
            Some("fn Foo()"),
            Some(Visibility::Public),
            None,
        ));
    }
    db.store_symbols(&symbols).unwrap();

    let result = deep_dive_query(&db, "Foo", None, "overview", 10, 10).unwrap();

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
