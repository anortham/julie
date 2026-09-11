use std::sync::Arc;

use julie_extractors::{RelationshipKind, SymbolKind};
use julie_facts::rows::{RelationshipRow, Span, SymbolRow};

use crate::graph::{EdgeKind, FileRows, SymbolId, SymbolTable, resolve};

fn symbol(
    path: &str,
    ordinal: u32,
    name: &str,
    kind: SymbolKind,
    signature: Option<&str>,
) -> SymbolRow {
    let line = ordinal + 1;
    SymbolRow {
        id: format!("{path}:{ordinal}"),
        blob_hash: path.to_string(),
        ordinal,
        path: path.to_string(),
        language: "rust".to_string(),
        name: name.to_string(),
        kind,
        span: Span {
            start_line: line,
            start_col: 0,
            end_line: line,
            end_col: 1,
            start_byte: 0,
            end_byte: 1,
        },
        body_span: None,
        body_hash: None,
        signature: signature.map(str::to_string),
        doc_comment: None,
        visibility: None,
        parent_ordinal: None,
        annotations: Vec::new(),
        metadata: None,
        semantic_group: None,
        confidence: None,
        content_type: None,
    }
}

fn file(path: &str, symbols: Vec<SymbolRow>) -> FileRows {
    FileRows {
        path: path.to_string(),
        symbols: Arc::from(symbols),
        identifiers: Arc::from(Vec::new()),
        relationships: Arc::from(Vec::new()),
    }
}

fn qualified_call(path: &str, from_ordinal: u32, to_name: &str) -> RelationshipRow {
    RelationshipRow {
        blob_hash: path.to_string(),
        ordinal: 0,
        path: path.to_string(),
        from_ordinal: Some(from_ordinal),
        to_name: to_name.to_string(),
        to_blob_hash: None,
        to_ordinal: None,
        kind: RelationshipKind::Calls,
        line_number: 1,
        span: None,
        reference_site_is_exact: false,
        confidence: 1.0,
        metadata: None,
    }
}

fn reexport_table() -> SymbolTable {
    SymbolTable::new(vec![
        file(
            "src/a.rs",
            vec![symbol(
                "src/a.rs",
                0,
                "target",
                SymbolKind::Import,
                Some("pub use crate::b::target;"),
            )],
        ),
        file(
            "src/b.rs",
            vec![symbol("src/b.rs", 0, "target", SymbolKind::Function, None)],
        ),
        file(
            "src/c.rs",
            vec![symbol("src/c.rs", 0, "caller", SymbolKind::Function, None)],
        ),
    ])
}

fn definition(symbols: &SymbolTable, path: &str, name: &str) -> SymbolId {
    let file = symbols.file_index(path).unwrap();
    symbols
        .find_by_name(name)
        .iter()
        .copied()
        .find(|id| symbols.file_of(*id) == file)
        .unwrap()
}

#[test]
fn crate_call_through_reexport_resolves_to_the_definition() {
    let symbols = reexport_table();
    let call = qualified_call("src/c.rs", 0, "crate::a::target");

    let edges = resolve(&symbols, [], [&call]);

    let calls: Vec<_> = edges
        .iter()
        .filter(|edge| edge.kind == EdgeKind::Calls)
        .map(|edge| (edge.from, edge.to))
        .collect();
    assert_eq!(
        calls,
        vec![(
            definition(&symbols, "src/c.rs", "caller"),
            definition(&symbols, "src/b.rs", "target")
        )]
    );
}

#[test]
fn crate_call_through_missing_reexport_drops_the_edge() {
    let symbols = reexport_table();
    let call = qualified_call("src/c.rs", 0, "crate::missing::target");

    let edges = resolve(&symbols, [], [&call]);

    assert!(edges.iter().all(|edge| edge.kind != EdgeKind::Calls));
}
