use julie_extractors::{RelationshipKind, SymbolKind};

use super::fixture::{file, graph_of, id_of, store_with};

fn score(graph: &crate::graph::Graph, path: &str, name: &str) -> f64 {
    graph.reference_score(id_of(graph, path, name))
}

#[test]
fn direct_score_weights_incoming_edges_by_kind() {
    let (_, store) = store_with(vec![
        file("a.rs").symbol("hub", "Hub", SymbolKind::Class).child(
            "member",
            "member",
            SymbolKind::Method,
            Some("hub"),
        ),
        file("b.rs")
            .symbol("u1", "u1", SymbolKind::Function)
            .call("u1", "Hub")
            .symbol("u2", "u2", SymbolKind::Function)
            .pending("u2", "Hub", RelationshipKind::Imports)
            .symbol("u3", "u3", SymbolKind::Function)
            .type_usage("u3", "Hub"),
    ]);
    let graph = graph_of(&store);

    assert_eq!(score(&graph, "a.rs", "Hub"), 3.0 + 2.0 + 1.0);
    assert_eq!(score(&graph, "a.rs", "member"), 0.0);
}

#[test]
fn implementations_inherit_seventy_percent_of_their_interface_score() {
    let (_, store) = store_with(vec![
        file("a.rs").symbol("shape", "Shape", SymbolKind::Interface),
        file("b.rs")
            .symbol("circle", "Circle", SymbolKind::Class)
            .pending("circle", "Shape", RelationshipKind::Implements),
        file("c.rs")
            .symbol("u", "u", SymbolKind::Function)
            .type_usage("u", "Shape")
            .type_usage("u", "Shape"),
    ]);
    let graph = graph_of(&store);

    assert_eq!(score(&graph, "a.rs", "Shape"), 1.0 + 1.0 + 2.0);
    assert!((score(&graph, "b.rs", "Circle") - 4.0 * 0.7).abs() < 1e-9);
}

#[test]
fn class_with_zero_score_inherits_seventy_percent_of_its_constructor() {
    let (_, store) = store_with(vec![
        file("a.rs")
            .symbol("svc", "Service", SymbolKind::Class)
            .child("ctor", "new", SymbolKind::Constructor, Some("svc")),
        file("b.rs")
            .symbol("u", "u", SymbolKind::Function)
            .call("u", "new"),
    ]);
    let graph = graph_of(&store);

    assert_eq!(score(&graph, "a.rs", "new"), 3.0);
    assert!((score(&graph, "a.rs", "Service") - 3.0 * 0.7).abs() < 1e-9);
}

#[test]
fn test_file_symbols_are_deweighted_to_ten_percent() {
    let (_, store) = store_with(vec![
        file("tests/fake.rs").symbol("fake", "Fake", SymbolKind::Class),
        file("src/user.rs")
            .symbol("u", "u", SymbolKind::Function)
            .type_usage("u", "Fake"),
    ]);
    let graph = graph_of(&store);

    assert!((score(&graph, "tests/fake.rs", "Fake") - 0.1).abs() < 1e-9);
}

#[test]
fn c_implementation_inherits_seventy_percent_of_its_header_declaration() {
    let (_, store) = store_with(vec![
        file("lib/parse.h")
            .symbol("decl", "parse", SymbolKind::Function)
            .symbol("inline_user", "parse_all", SymbolKind::Function)
            .call("inline_user", "parse"),
        file("lib/parse.c").symbol("def", "parse", SymbolKind::Function),
    ]);
    let graph = graph_of(&store);

    assert_eq!(score(&graph, "lib/parse.h", "parse"), 3.0);
    assert!((score(&graph, "lib/parse.c", "parse") - 3.0 * 0.7).abs() < 1e-9);
}
