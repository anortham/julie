use julie_extractors::SymbolKind;

use super::fixture::{edges_of_kind, file, graph_of, store_with};
use crate::graph::EdgeKind;

#[test]
fn client_call_links_to_the_matching_route_handler() {
    let (_, store) = store_with(vec![
        file("client.ts")
            .symbol("fetch_user", "fetchUser", SymbolKind::Function)
            .client_call("fetch_user", Some("get"), "/users/42", 0.9),
        file("server.rs")
            .symbol("get_user", "get_user", SymbolKind::Function)
            .route("get_user", Some("GET"), "/users/:id", 0.8)
            .symbol("list_users", "list_users", SymbolKind::Function)
            .route("list_users", Some("GET"), "/users", 0.8),
    ]);
    let graph = graph_of(&store);

    assert_eq!(
        edges_of_kind(&graph, EdgeKind::WebRoute),
        vec![(
            "client.ts:fetchUser".to_string(),
            "server.rs:get_user".to_string()
        )]
    );
}

#[test]
fn verb_mismatch_and_low_confidence_yield_no_edge() {
    let (_, store) = store_with(vec![
        file("client.ts")
            .symbol("del", "del", SymbolKind::Function)
            .client_call("del", Some("delete"), "/users/1", 0.9)
            .symbol("weak", "weak", SymbolKind::Function)
            .client_call("weak", Some("get"), "/users/1", 0.2),
        file("server.rs")
            .symbol("get_user", "get_user", SymbolKind::Function)
            .route("get_user", Some("GET"), "/users/{id}", 0.8),
    ]);
    let graph = graph_of(&store);

    assert!(edges_of_kind(&graph, EdgeKind::WebRoute).is_empty());
}

#[test]
fn handler_without_verb_accepts_any_client_verb() {
    let (_, store) = store_with(vec![
        file("client.ts")
            .symbol("post_it", "postIt", SymbolKind::Function)
            .client_call("post_it", Some("post"), "/items", 0.9),
        file("server.rs")
            .symbol("any_items", "any_items", SymbolKind::Function)
            .route("any_items", None, "/items", 0.9),
    ]);
    let graph = graph_of(&store);

    assert_eq!(
        edges_of_kind(&graph, EdgeKind::WebRoute),
        vec![(
            "client.ts:postIt".to_string(),
            "server.rs:any_items".to_string()
        )]
    );
}

#[test]
fn equally_confident_handlers_in_different_symbols_are_ambiguous() {
    let (_, store) = store_with(vec![
        file("client.ts")
            .symbol("go", "go", SymbolKind::Function)
            .client_call("go", Some("get"), "/dup", 0.9),
        file("server.rs")
            .symbol("h1", "h1", SymbolKind::Function)
            .route("h1", Some("GET"), "/dup", 0.8)
            .symbol("h2", "h2", SymbolKind::Function)
            .route("h2", Some("GET"), "/dup", 0.8),
    ]);
    let graph = graph_of(&store);

    assert!(edges_of_kind(&graph, EdgeKind::WebRoute).is_empty());
}
