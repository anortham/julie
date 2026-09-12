use std::collections::HashMap;

use julie_extractors::SymbolKind;
use julie_facts::rows::StructuralFactQuery;
use serde_json::json;

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

#[test]
fn web_route_resolution_retains_bound_client_without_target_path() {
    let (_, store) = store_with(vec![file("client.ts")
        .symbol("dynamic", "dynamic", SymbolKind::Function)
        .fact(
            "http.client_request.v1",
            Some("dynamic"),
            0.9,
            HashMap::from([("verb".to_string(), json!("GET"))]),
        )]);
    let graph = graph_of(&store);
    let facts = store
        .reader()
        .structural_facts(&StructuralFactQuery {
            pattern_ids: vec!["http.client_request.v1".to_string()],
            path_pattern: None,
            language: None,
            limit: usize::MAX,
        })
        .unwrap();
    let routes = graph.resolved_web_routes(&facts);

    assert_eq!(routes.len(), 1);
    assert_eq!(graph.symbol(routes[0].from).name, "dynamic");
    assert_eq!(routes[0].client_index, 0);
    assert_eq!(routes[0].to, None);
    assert_eq!(routes[0].confidence, None);
}

fn table_fact(name: &str) -> HashMap<String, serde_json::Value> {
    HashMap::from([("table_name".to_string(), json!(name))])
}

#[test]
fn sql_mutation_links_the_routine_to_the_unique_table_definition() {
    let (_, store) = store_with(vec![
        file("schema/tables.sql")
            .symbol("users", "users", SymbolKind::Class)
            .fact(
                "sql.table_definition.v1",
                Some("users"),
                1.0,
                table_fact("users"),
            )
            .symbol("dup", "dup", SymbolKind::Class)
            .fact(
                "sql.table_definition.v1",
                Some("dup"),
                1.0,
                table_fact("dup"),
            ),
        file("schema/more.sql")
            .symbol("dup2", "dup", SymbolKind::Class)
            .fact(
                "sql.table_definition.v1",
                Some("dup2"),
                1.0,
                table_fact("dup"),
            ),
        file("schema/routines.sql")
            .symbol("touch", "touch_users", SymbolKind::Function)
            .fact(
                "sql.update_statement.v1",
                Some("touch"),
                1.0,
                table_fact("users"),
            )
            .symbol("view", "user_view", SymbolKind::Function)
            .fact(
                "sql.view_definition.v1",
                Some("view"),
                1.0,
                HashMap::from([(
                    "source_tables".to_string(),
                    json!(["users", "orders", "dup"]),
                )]),
            )
            .symbol("merge", "merge_users", SymbolKind::Function)
            .fact(
                "sql.merge_statement.v1",
                Some("merge"),
                1.0,
                HashMap::from([("target_table".to_string(), json!("users"))]),
            ),
    ]);
    let graph = graph_of(&store);

    assert_eq!(
        edges_of_kind(&graph, EdgeKind::SqlQuery),
        vec![
            (
                "schema/routines.sql:merge_users".to_string(),
                "schema/tables.sql:users".to_string()
            ),
            (
                "schema/routines.sql:touch_users".to_string(),
                "schema/tables.sql:users".to_string()
            ),
            (
                "schema/routines.sql:user_view".to_string(),
                "schema/tables.sql:users".to_string()
            ),
        ]
    );
}
