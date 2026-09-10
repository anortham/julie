use julie_extractors::{RelationshipKind, SymbolKind};

use super::fixture::{edges_of_kind, file, graph_of, id_of, labels, store_with};
use crate::graph::EdgeKind;

#[test]
fn children_and_parent_come_from_parent_ordinal() {
    let (_, store) = store_with(vec![
        file("a.rs")
            .symbol("cls", "Widget", SymbolKind::Class)
            .child("m1", "draw", SymbolKind::Method, Some("cls"))
            .child("m2", "hide", SymbolKind::Method, Some("cls")),
    ]);
    let graph = graph_of(&store);
    let widget = id_of(&graph, "a.rs", "Widget");
    let draw = id_of(&graph, "a.rs", "draw");

    assert_eq!(
        edges_of_kind(&graph, EdgeKind::Contains),
        vec![
            ("a.rs:Widget".to_string(), "a.rs:draw".to_string()),
            ("a.rs:Widget".to_string(), "a.rs:hide".to_string()),
        ]
    );
    assert_eq!(
        labels(&graph, graph.children(widget)),
        vec!["a.rs:draw", "a.rs:hide"]
    );
    assert_eq!(graph.parent(draw), Some(widget));
    assert_eq!(graph.parent(widget), None);
}

#[test]
fn callers_and_callees_walk_calls_edges_in_both_directions() {
    let (_, store) = store_with(vec![
        file("a.rs")
            .symbol("main", "main", SymbolKind::Function)
            .call("main", "work")
            .type_usage("main", "Config"),
        file("b.rs")
            .symbol("work", "work", SymbolKind::Function)
            .symbol("config", "Config", SymbolKind::Struct)
            .call("work", "finish"),
        file("c.rs").symbol("finish", "finish", SymbolKind::Function),
    ]);
    let graph = graph_of(&store);
    let work = id_of(&graph, "b.rs", "work");

    assert_eq!(labels(&graph, graph.callers(work)), vec!["a.rs:main"]);
    assert_eq!(labels(&graph, graph.callees(work)), vec!["c.rs:finish"]);
    assert_eq!(
        labels(&graph, graph.references_from(id_of(&graph, "a.rs", "main"))),
        vec!["b.rs:Config", "b.rs:work"]
    );
    assert_eq!(
        labels(&graph, graph.references_to(id_of(&graph, "b.rs", "Config"))),
        vec!["a.rs:main"]
    );
}

#[test]
fn implementations_include_implements_and_extends_sources() {
    let (_, store) = store_with(vec![
        file("a.rs")
            .symbol("shape", "Shape", SymbolKind::Interface)
            .symbol("base", "Base", SymbolKind::Class),
        file("b.rs")
            .symbol("circle", "Circle", SymbolKind::Class)
            .pending("circle", "Shape", RelationshipKind::Implements)
            .symbol("child", "Child", SymbolKind::Class)
            .pending("child", "Base", RelationshipKind::Extends)
            .symbol("user", "user", SymbolKind::Function)
            .type_usage("user", "Shape"),
    ]);
    let graph = graph_of(&store);

    assert_eq!(
        labels(
            &graph,
            graph.implementations(id_of(&graph, "a.rs", "Shape"))
        ),
        vec!["b.rs:Circle"]
    );
    assert_eq!(
        labels(&graph, graph.implementations(id_of(&graph, "a.rs", "Base"))),
        vec!["b.rs:Child"]
    );
}

#[test]
fn duplicate_call_sites_collapse_to_one_adjacency_entry() {
    let (_, store) = store_with(vec![
        file("a.rs")
            .symbol("main", "main", SymbolKind::Function)
            .call("main", "work")
            .call("main", "work"),
        file("b.rs").symbol("work", "work", SymbolKind::Function),
    ]);
    let graph = graph_of(&store);

    assert_eq!(graph.callers(id_of(&graph, "b.rs", "work")).count(), 1);
    assert_eq!(graph.stats().edges, 1);
}
