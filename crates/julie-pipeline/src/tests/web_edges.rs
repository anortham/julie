use std::collections::HashMap;

use julie_core::database::WebEdgeKind;
use julie_extractors::base::StructuralFact;
use serde_json::json;

use crate::indexing_core::web_edges::{derive_http_call_edges, derive_sql_query_edges};

#[allow(clippy::too_many_arguments)]
fn fact(
    id: &str,
    pattern_id: &str,
    file_path: &str,
    language: &str,
    line: u32,
    containing_symbol_id: Option<&str>,
    confidence: f32,
    metadata: serde_json::Value,
) -> StructuralFact {
    let metadata = serde_json::from_value::<HashMap<String, serde_json::Value>>(metadata).ok();
    StructuralFact {
        id: id.into(),
        file_path: file_path.into(),
        language: language.into(),
        pattern_id: pattern_id.into(),
        capture_name: "node".into(),
        node_kind: "call_expression".into(),
        containing_symbol_id: containing_symbol_id.map(str::to_string),
        start_line: line,
        start_column: 0,
        end_line: line,
        end_column: 12,
        start_byte: line * 10,
        end_byte: line * 10 + 12,
        confidence,
        metadata,
    }
}

#[test]
fn derives_matched_http_call_edge() {
    let client = fact(
        "c1",
        "http.client_request.v1",
        "src/client.ts",
        "typescript",
        3,
        Some("fetch_user"),
        0.95,
        json!({"verb": "GET", "target_path": "/api/users/123", "client": "fetch"}),
    );
    let handler = fact(
        "h1",
        "symfony.route.v1",
        "src/Controller.php",
        "php",
        8,
        Some("show_user"),
        0.9,
        json!({"verb": "GET", "normalized_route_template": "/api/users/{id}", "route_template": "/api/users/{id}"}),
    );
    let edges = derive_http_call_edges(&[client], &[handler]);
    assert_eq!(edges.len(), 1);
    let e = &edges[0];
    assert_eq!(e.kind, WebEdgeKind::HttpCall);
    assert_eq!(e.from_symbol_id, "fetch_user");
    assert_eq!(e.to_symbol_id.as_deref(), Some("show_user"));
    assert_eq!(e.to_external, None);
    assert_eq!(e.method.as_deref(), Some("GET"));
    assert!((e.confidence - 0.9_f32).abs() < 1e-3);
}

#[test]
fn equal_confidence_http_handlers_remain_external() {
    let client = fact(
        "c1",
        "http.client_request.v1",
        "src/client.ts",
        "typescript",
        3,
        Some("fetch_user"),
        0.95,
        json!({"verb": "GET", "target_path": "/api/users/123", "client": "fetch"}),
    );
    let handlers = [
        fact(
            "h1",
            "symfony.route.v1",
            "src/FirstController.php",
            "php",
            8,
            Some("show_user_first"),
            0.9,
            json!({"verb": "GET", "normalized_route_template": "/api/users/{id}"}),
        ),
        fact(
            "h2",
            "symfony.route.v1",
            "src/SecondController.php",
            "php",
            9,
            Some("show_user_second"),
            0.9,
            json!({"verb": "GET", "normalized_route_template": "/api/users/{id}"}),
        ),
    ];

    let edges = derive_http_call_edges(&[client], &handlers);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].to_symbol_id, None);
    assert_eq!(edges[0].to_external.as_deref(), Some("GET /api/users/123"));

    let client = fact(
        "c2",
        "http.client_request.v1",
        "src/client.ts",
        "typescript",
        4,
        Some("fetch_user"),
        0.95,
        json!({"verb": "GET", "target_path": "/api/users/123"}),
    );
    let duplicate_handler_facts = [
        fact(
            "h3",
            "symfony.route.v1",
            "src/Controller.php",
            "php",
            10,
            Some("show_user"),
            0.9,
            json!({"verb": "GET", "normalized_route_template": "/api/users/{id}"}),
        ),
        fact(
            "h4",
            "symfony.route.v1",
            "src/Controller.php",
            "php",
            11,
            Some("show_user"),
            0.9,
            json!({"verb": "GET", "normalized_route_template": "/api/users/{id}"}),
        ),
    ];

    let edges = derive_http_call_edges(&[client], &duplicate_handler_facts);

    assert_eq!(edges[0].to_symbol_id.as_deref(), Some("show_user"));
    assert_eq!(edges[0].to_external, None);
}

#[test]
fn derives_external_edge_when_no_handler_matches() {
    let client = fact(
        "c1",
        "http.client_request.v1",
        "src/client.ts",
        "typescript",
        3,
        Some("fetch_user"),
        0.95,
        json!({"verb": "GET", "target_path": "/api/unknown", "client": "fetch"}),
    );
    let edges = derive_http_call_edges(&[client], &[]);
    assert_eq!(edges.len(), 1);
    let e = &edges[0];
    assert_eq!(e.kind, WebEdgeKind::HttpCall);
    assert_eq!(e.from_symbol_id, "fetch_user");
    assert_eq!(e.to_symbol_id, None);
    assert_eq!(e.to_external.as_deref(), Some("GET /api/unknown"));
}

#[test]
fn degrades_to_external_on_method_mismatch() {
    let client = fact(
        "c1",
        "http.client_request.v1",
        "src/client.ts",
        "typescript",
        3,
        Some("fetch_user"),
        0.95,
        json!({"verb": "GET", "target_path": "/api/users/123", "client": "fetch"}),
    );
    let handler = fact(
        "h1",
        "symfony.route.v1",
        "src/Controller.php",
        "php",
        8,
        Some("create_user"),
        0.9,
        json!({"verb": "POST", "normalized_route_template": "/api/users/{id}"}),
    );
    let edges = derive_http_call_edges(&[client], &[handler]);
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].to_symbol_id, None);
    assert_eq!(edges[0].to_external.as_deref(), Some("GET /api/users/123"));
}

#[test]
fn matches_parametric_templates_across_flavors() {
    let client = |target: &str| {
        fact(
            "c1",
            "http.client_request.v1",
            "src/client.ts",
            "typescript",
            3,
            Some("fetch_user"),
            0.95,
            json!({"verb": "GET", "target_path": target, "client": "fetch"}),
        )
    };
    let handler = |template: &str, sym: &str| {
        fact(
            "h1",
            "symfony.route.v1",
            "src/Controller.php",
            "php",
            8,
            Some(sym),
            0.9,
            json!({"verb": "GET", "normalized_route_template": template}),
        )
    };

    let edges = derive_http_call_edges(
        &[client("/api/users/123")],
        &[handler("/api/users/{id}", "show_braces")],
    );
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].to_symbol_id.as_deref(), Some("show_braces"));

    let edges = derive_http_call_edges(
        &[client("/api/users/123")],
        &[handler("/api/users/:id", "show_colon")],
    );
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].to_symbol_id.as_deref(), Some("show_colon"));

    let edges = derive_http_call_edges(
        &[client("/api/users/123")],
        &[handler("/api/users/<id>", "show_angle")],
    );
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].to_symbol_id.as_deref(), Some("show_angle"));
}

#[test]
fn literal_segment_mismatch_degrades_to_external() {
    let client = fact(
        "c1",
        "http.client_request.v1",
        "src/client.ts",
        "typescript",
        3,
        Some("fetch_user"),
        0.95,
        json!({"verb": "GET", "target_path": "/api/posts/1", "client": "fetch"}),
    );
    let handler = fact(
        "h1",
        "symfony.route.v1",
        "src/Controller.php",
        "php",
        8,
        Some("show_user"),
        0.9,
        json!({"verb": "GET", "normalized_route_template": "/api/users/{id}"}),
    );
    let edges = derive_http_call_edges(&[client], &[handler]);
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].to_symbol_id, None);
    assert_eq!(edges[0].to_external.as_deref(), Some("GET /api/posts/1"));
}

#[test]
fn derives_sql_query_edge_from_view_to_table() {
    let view = fact(
        "v1",
        "sql.view_definition.v1",
        "schema/views.sql",
        "sql",
        4,
        Some("active_users_view"),
        1.0,
        json!({"view_name": "active_users", "source_table_count": 1, "source_tables": ["users"]}),
    );
    let table = fact(
        "t1",
        "sql.table_definition.v1",
        "schema/tables.sql",
        "sql",
        2,
        Some("users_table_symbol"),
        1.0,
        json!({"table_name": "users", "column_count": 2, "constraint_count": 0}),
    );
    let edges = derive_sql_query_edges(&[view], &[table]);
    assert_eq!(edges.len(), 1);
    let e = &edges[0];
    assert_eq!(e.kind, WebEdgeKind::SqlQuery);
    assert_eq!(e.from_symbol_id, "active_users_view");
    assert_eq!(e.to_symbol_id.as_deref(), Some("users_table_symbol"));
    assert_eq!(e.to_external, None);
    assert_eq!(e.table.as_deref(), Some("users"));
    assert!((e.confidence - 1.0_f32).abs() < 1e-3);
}

#[test]
fn duplicate_sql_table_definitions_remain_external() {
    let query = fact(
        "q1",
        "sql.update_statement.v1",
        "schema/routines.sql",
        "sql",
        4,
        Some("touch_users"),
        1.0,
        json!({"table_name": "users"}),
    );
    let tables = [
        fact(
            "t1",
            "sql.table_definition.v1",
            "schema/first.sql",
            "sql",
            2,
            Some("users_table_first"),
            1.0,
            json!({"table_name": "users"}),
        ),
        fact(
            "t2",
            "sql.table_definition.v1",
            "schema/second.sql",
            "sql",
            2,
            Some("users_table_second"),
            1.0,
            json!({"table_name": "users"}),
        ),
    ];

    let edges = derive_sql_query_edges(&[query], &tables);

    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].to_symbol_id, None);
    assert_eq!(edges[0].to_external.as_deref(), Some("table:users"));
}

#[test]
fn derives_sql_query_edge_from_routine_to_table() {
    let update = fact(
        "u1",
        "sql.update_statement.v1",
        "schema/routines.sql",
        "sql",
        6,
        Some("touch_user_proc"),
        1.0,
        json!({"table_name": "users", "has_where": true}),
    );
    let table = fact(
        "t1",
        "sql.table_definition.v1",
        "schema/tables.sql",
        "sql",
        2,
        Some("users_table_symbol"),
        1.0,
        json!({"table_name": "users", "column_count": 2, "constraint_count": 0}),
    );
    let edges = derive_sql_query_edges(&[update], &[table]);
    assert_eq!(edges.len(), 1);
    let e = &edges[0];
    assert_eq!(e.kind, WebEdgeKind::SqlQuery);
    assert_eq!(e.from_symbol_id, "touch_user_proc");
    assert_eq!(e.to_symbol_id.as_deref(), Some("users_table_symbol"));
    assert_eq!(e.table.as_deref(), Some("users"));
}

#[test]
fn sql_query_degrades_to_external_table_when_no_definition() {
    let update = fact(
        "u1",
        "sql.delete_statement.v1",
        "schema/routines.sql",
        "sql",
        6,
        Some("purge_proc"),
        1.0,
        json!({"table_name": "orders", "has_where": false}),
    );
    let edges = derive_sql_query_edges(&[update], &[]);
    assert_eq!(edges.len(), 1);
    let e = &edges[0];
    assert_eq!(e.kind, WebEdgeKind::SqlQuery);
    assert_eq!(e.from_symbol_id, "purge_proc");
    assert_eq!(e.to_symbol_id, None);
    assert_eq!(e.to_external.as_deref(), Some("table:orders"));
    assert_eq!(e.table.as_deref(), Some("orders"));
}

#[test]
fn sql_query_skips_facts_with_no_containing_symbol() {
    let update = fact(
        "u1",
        "sql.update_statement.v1",
        "migrations/0001.sql",
        "sql",
        1,
        None,
        1.0,
        json!({"table_name": "users", "has_where": false}),
    );
    let table = fact(
        "t1",
        "sql.table_definition.v1",
        "schema/tables.sql",
        "sql",
        2,
        Some("users_table_symbol"),
        1.0,
        json!({"table_name": "users", "column_count": 1, "constraint_count": 0}),
    );
    let edges = derive_sql_query_edges(&[update], &[table]);
    assert!(
        edges.is_empty(),
        "top-level statement with no containing symbol emits no edge"
    );
}

#[test]
fn sql_query_merge_uses_target_table() {
    let merge = fact(
        "m1",
        "sql.merge_statement.v1",
        "schema/routines.sql",
        "sql",
        9,
        Some("upsert_proc"),
        1.0,
        json!({"target_table": "users", "source_kind": "values", "has_when_matched": true, "has_when_not_matched": true}),
    );
    let table = fact(
        "t1",
        "sql.table_definition.v1",
        "schema/tables.sql",
        "sql",
        2,
        Some("users_table_symbol"),
        1.0,
        json!({"table_name": "users", "column_count": 1, "constraint_count": 0}),
    );
    let edges = derive_sql_query_edges(&[merge], &[table]);
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].from_symbol_id, "upsert_proc");
    assert_eq!(edges[0].to_symbol_id.as_deref(), Some("users_table_symbol"));
    assert_eq!(edges[0].table.as_deref(), Some("users"));
}

#[test]
fn sql_query_view_emits_one_edge_per_source_table() {
    let view = fact(
        "v1",
        "sql.view_definition.v1",
        "schema/views.sql",
        "sql",
        4,
        Some("join_view"),
        1.0,
        json!({"view_name": "join_view", "source_table_count": 2, "source_tables": ["users", "orders"]}),
    );
    let users = fact(
        "t1",
        "sql.table_definition.v1",
        "schema/tables.sql",
        "sql",
        2,
        Some("users_table_symbol"),
        1.0,
        json!({"table_name": "users", "column_count": 1, "constraint_count": 0}),
    );
    let orders = fact(
        "t2",
        "sql.table_definition.v1",
        "schema/tables.sql",
        "sql",
        8,
        Some("orders_table_symbol"),
        1.0,
        json!({"table_name": "orders", "column_count": 1, "constraint_count": 0}),
    );
    let edges = derive_sql_query_edges(&[view], &[users, orders]);
    assert_eq!(edges.len(), 2);
    let tables: Vec<Option<&str>> = edges.iter().map(|e| e.to_symbol_id.as_deref()).collect();
    assert!(tables.contains(&Some("users_table_symbol")));
    assert!(tables.contains(&Some("orders_table_symbol")));
}
