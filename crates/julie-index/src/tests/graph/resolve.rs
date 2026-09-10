use julie_extractors::{IdentifierKind, RelationshipKind, SymbolKind};

use super::fixture::{edge_labels, edges_of_kind, file, graph_of, store_with};
use crate::graph::EdgeKind;

fn pair(from: &str, to: &str) -> (String, String) {
    (from.to_string(), to.to_string())
}

#[test]
fn exact_name_match_resolves_a_call_identifier() {
    let (_, store) = store_with(vec![
        file("a.rs")
            .symbol("caller", "caller", SymbolKind::Function)
            .call("caller", "target"),
        file("b.rs").symbol("target", "target", SymbolKind::Function),
    ]);
    let graph = graph_of(&store);

    assert_eq!(
        edges_of_kind(&graph, EdgeKind::Calls),
        vec![pair("a.rs:caller", "b.rs:target")]
    );
}

#[test]
fn qualified_suffix_match_resolves_to_the_child_of_the_named_parent() {
    let (_, store) = store_with(vec![
        file("a.rs")
            .symbol("caller", "caller", SymbolKind::Function)
            .call("caller", "Foo::run")
            .call("caller", "Bar.run"),
        file("b.rs").symbol("foo", "Foo", SymbolKind::Class).child(
            "foo_run",
            "run",
            SymbolKind::Method,
            Some("foo"),
        ),
        file("c.rs").symbol("bar", "Bar", SymbolKind::Class).child(
            "bar_run",
            "run",
            SymbolKind::Method,
            Some("bar"),
        ),
    ]);
    let graph = graph_of(&store);

    assert_eq!(
        edges_of_kind(&graph, EdgeKind::Calls),
        vec![
            pair("a.rs:caller", "b.rs:run"),
            pair("a.rs:caller", "c.rs:run")
        ]
    );
}

#[test]
fn ambiguous_name_drops_the_edge() {
    let (_, store) = store_with(vec![
        file("a.rs")
            .symbol("caller", "caller", SymbolKind::Function)
            .call("caller", "dup"),
        file("b.rs").symbol("dup_b", "dup", SymbolKind::Function),
        file("c.rs").symbol("dup_c", "dup", SymbolKind::Function),
    ]);
    let graph = graph_of(&store);

    assert!(edges_of_kind(&graph, EdgeKind::Calls).is_empty());
}

#[test]
fn definition_priority_picks_the_class_over_the_variable() {
    let (_, store) = store_with(vec![
        file("a.rs")
            .symbol("user", "user", SymbolKind::Function)
            .type_usage("user", "Thing"),
        file("b.rs").symbol("thing_var", "Thing", SymbolKind::Variable),
        file("c.rs").symbol("thing_class", "Thing", SymbolKind::Class),
    ]);
    let graph = graph_of(&store);

    assert_eq!(
        edges_of_kind(&graph, EdgeKind::References),
        vec![pair("a.rs:user", "c.rs:Thing")]
    );
}

#[test]
fn same_file_definition_is_preferred_over_other_files() {
    let (_, store) = store_with(vec![
        file("a.rs")
            .symbol("caller", "caller", SymbolKind::Function)
            .symbol("helper_a", "helper", SymbolKind::Function)
            .call("caller", "helper"),
        file("b.rs").symbol("helper_b", "helper", SymbolKind::Function),
    ]);
    let graph = graph_of(&store);

    assert_eq!(
        edges_of_kind(&graph, EdgeKind::Calls),
        vec![pair("a.rs:caller", "a.rs:helper")]
    );
}

#[test]
fn impact_identifier_kinds_map_call_and_type_usage_and_skip_the_rest() {
    let (_, store) = store_with(vec![
        file("a.rs")
            .symbol("user", "user", SymbolKind::Function)
            .identifier("user", "target", IdentifierKind::Call)
            .identifier("user", "Shape", IdentifierKind::TypeUsage)
            .identifier("user", "field", IdentifierKind::MemberAccess)
            .identifier("user", "local", IdentifierKind::VariableRef),
        file("b.rs")
            .symbol("target", "target", SymbolKind::Function)
            .symbol("shape", "Shape", SymbolKind::Struct)
            .symbol("field", "field", SymbolKind::Field)
            .symbol("local", "local", SymbolKind::Variable),
    ]);
    let graph = graph_of(&store);

    let edges: Vec<_> = edge_labels(&graph)
        .into_iter()
        .filter(|(_, _, kind)| *kind != EdgeKind::Contains)
        .collect();
    assert_eq!(
        edges,
        vec![
            (
                "a.rs:user".into(),
                "b.rs:Shape".into(),
                EdgeKind::References
            ),
            ("a.rs:user".into(), "b.rs:target".into(), EdgeKind::Calls),
        ]
    );
}

#[test]
fn unresolved_relationship_target_resolves_by_name() {
    let (_, store) = store_with(vec![
        file("a.rs")
            .symbol("impl", "Impl", SymbolKind::Class)
            .pending("impl", "Shape", RelationshipKind::Implements)
            .pending("impl", "Base", RelationshipKind::Extends)
            .pending("impl", "make", RelationshipKind::Instantiates)
            .pending("impl", "cfg", RelationshipKind::Imports),
        file("b.rs")
            .symbol("shape", "Shape", SymbolKind::Interface)
            .symbol("base", "Base", SymbolKind::Class)
            .symbol("make", "make", SymbolKind::Function)
            .symbol("cfg", "cfg", SymbolKind::Module),
    ]);
    let graph = graph_of(&store);

    assert_eq!(
        edge_labels(&graph),
        vec![
            ("a.rs:Impl".into(), "b.rs:Base".into(), EdgeKind::Extends),
            (
                "a.rs:Impl".into(),
                "b.rs:Shape".into(),
                EdgeKind::Implements
            ),
            ("a.rs:Impl".into(), "b.rs:cfg".into(), EdgeKind::Imports),
            ("a.rs:Impl".into(), "b.rs:make".into(), EdgeKind::Calls),
        ]
    );
}

#[test]
fn in_blob_relationship_maps_ordinals_directly() {
    let (_, store) = store_with(vec![
        file("a.rs")
            .symbol("base", "Base", SymbolKind::Class)
            .symbol("base2", "Base", SymbolKind::Class)
            .symbol("child", "Child", SymbolKind::Class)
            .relation("child", "base2", RelationshipKind::Extends),
    ]);
    let graph = graph_of(&store);
    let base2 = graph.symbols_in_path("a.rs")[1];

    let parents: Vec<_> = graph
        .references_from(graph.symbols_in_path("a.rs")[2])
        .collect();
    assert_eq!(parents, vec![base2]);
}

#[test]
fn import_and_export_symbols_are_not_resolution_targets() {
    let (_, store) = store_with(vec![
        file("a.rs")
            .symbol("caller", "caller", SymbolKind::Function)
            .symbol("use_target", "target", SymbolKind::Import)
            .call("caller", "target"),
        file("b.rs")
            .symbol("target", "target", SymbolKind::Function)
            .symbol("export_target", "target", SymbolKind::Export),
    ]);
    let graph = graph_of(&store);

    assert_eq!(
        edges_of_kind(&graph, EdgeKind::Calls),
        vec![pair("a.rs:caller", "b.rs:target")]
    );
}

#[test]
fn self_edges_are_dropped() {
    let (_, store) = store_with(vec![
        file("a.rs")
            .symbol("rec", "rec", SymbolKind::Function)
            .call("rec", "rec"),
    ]);
    let graph = graph_of(&store);

    assert!(edge_labels(&graph).is_empty());
}
