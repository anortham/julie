use std::collections::HashMap;

use anyhow::Result;
use julie_core::database::bulk::atomic::{AtomicPersistenceMetadata, CanonicalWriteSet};
use julie_core::database::SymbolDatabase;
use julie_extractors::base::StructuralFact;
use julie_extractors::SymbolKind;
use julie_test_support::db::{file_info_builder, symbol_builder};
use julie_test_support::FakeToolContext;
use tempfile::TempDir;

use crate::tests::helpers::mcp::call_tool_result_text;
use crate::tools::{BlastRadiusTool, CallPathTool};

fn client_fact(
    id: &str,
    file_path: &str,
    language: &str,
    line: u32,
    symbol_id: &str,
    verb: &str,
    target_path: &str,
) -> StructuralFact {
    StructuralFact {
        id: id.into(),
        file_path: file_path.into(),
        language: language.into(),
        pattern_id: "http.client_request.v1".into(),
        capture_name: "request".into(),
        node_kind: "call_expression".into(),
        containing_symbol_id: Some(symbol_id.into()),
        start_line: line,
        start_column: 0,
        end_line: line,
        end_column: 40,
        start_byte: line * 10,
        end_byte: line * 10 + 40,
        confidence: 0.95,
        metadata: Some(HashMap::from([
            ("verb".into(), serde_json::json!(verb)),
            ("target_path".into(), serde_json::json!(target_path)),
            ("client".into(), serde_json::json!("fetch")),
        ])),
    }
}

fn route_fact(
    id: &str,
    file_path: &str,
    language: &str,
    line: u32,
    symbol_id: &str,
    verb: &str,
    template: &str,
) -> StructuralFact {
    StructuralFact {
        id: id.into(),
        file_path: file_path.into(),
        language: language.into(),
        pattern_id: "symfony.route.v1".into(),
        capture_name: "route".into(),
        node_kind: "route".into(),
        containing_symbol_id: Some(symbol_id.into()),
        start_line: line,
        start_column: 0,
        end_line: line,
        end_column: 50,
        start_byte: line * 10,
        end_byte: line * 10 + 50,
        confidence: 0.9,
        metadata: Some(HashMap::from([
            ("verb".into(), serde_json::json!(verb)),
            (
                "normalized_route_template".into(),
                serde_json::json!(template),
            ),
        ])),
    }
}

/// Build a temp workspace with:
///  - `fetchUser` (src/client.ts) issuing `GET /api/users/123`
///  - `showUser`  (src/Controller.php) handling `GET /api/users/{id}`
///  - `fetchUnknown` (src/client.ts) issuing `POST /api/unknown` (no handler)
/// Web edges are derived via `rebuild_web_edges`.
fn seeded_context() -> Result<(TempDir, FakeToolContext)> {
    let temp = TempDir::new()?;
    let db_path = temp.path().join("webnav.db");
    let mut db = SymbolDatabase::new(&db_path)?;

    let files = vec![
        file_info_builder("src/client.ts")
            .language("typescript")
            .build(),
        file_info_builder("src/Controller.php")
            .language("php")
            .build(),
    ];
    let symbols = vec![
        symbol_builder("fetch_user", "fetchUser", "src/client.ts").build(),
        symbol_builder("fetch_unknown", "fetchUnknown", "src/client.ts").build(),
        symbol_builder("show_user", "showUser", "src/Controller.php").build(),
    ];
    let facts = vec![
        client_fact(
            "c1",
            "src/client.ts",
            "typescript",
            3,
            "fetch_user",
            "GET",
            "/api/users/123",
        ),
        client_fact(
            "c2",
            "src/client.ts",
            "typescript",
            7,
            "fetch_unknown",
            "POST",
            "/api/unknown",
        ),
        route_fact(
            "h1",
            "src/Controller.php",
            "php",
            8,
            "show_user",
            "GET",
            "/api/users/{id}",
        ),
    ];

    let write_set = CanonicalWriteSet {
        files: &files,
        symbols: &symbols,
        structural_facts: &facts,
        ..Default::default()
    };
    db.incremental_update_atomic_with_metadata(
        &[
            "src/client.ts".to_string(),
            "src/Controller.php".to_string(),
        ],
        &write_set,
        "webnav-test",
        AtomicPersistenceMetadata::default(),
    )?;
    julie_pipeline::indexing_core::web_edges::rebuild_web_edges(&mut db)?;
    drop(db);

    let context = FakeToolContext::new()
        .with_workspace_id("webnav-test")
        .with_primary_root(temp.path())
        .with_primary_db_path(&db_path);
    Ok((temp, context))
}

#[tokio::test]
async fn trace_web_mode_follows_http_call_edge_to_handler() -> Result<()> {
    let (_temp, context) = seeded_context()?;

    let result = CallPathTool {
        from: "fetchUser".into(),
        to: "showUser".into(),
        mode: Some("web".into()),
        ..Default::default()
    }
    .call_tool(&context)
    .await?;
    let text = call_tool_result_text(&result);

    assert!(
        text.contains("found=true"),
        "expected found=true, got: {text}"
    );
    assert!(
        text.contains("--http_call-->"),
        "expected an http_call hop, got: {text}"
    );
    assert!(
        text.contains("showUser"),
        "expected target showUser, got: {text}"
    );
    Ok(())
}

#[tokio::test]
async fn trace_web_mode_reports_external_endpoint_for_unmatched_call() -> Result<()> {
    let (_temp, context) = seeded_context()?;

    let result = CallPathTool {
        from: "fetchUnknown".into(),
        to: "showUser".into(),
        mode: Some("web".into()),
        ..Default::default()
    }
    .call_tool(&context)
    .await?;
    let text = call_tool_result_text(&result);

    assert!(
        text.contains("found=false"),
        "expected found=false, got: {text}"
    );
    assert!(
        text.contains("external_endpoint: POST /api/unknown"),
        "expected external endpoint label, got: {text}"
    );
    Ok(())
}

#[tokio::test]
async fn trace_default_mode_is_byte_identical_no_web_markers() -> Result<()> {
    let (_temp, context) = seeded_context()?;

    let result = CallPathTool {
        from: "fetchUser".into(),
        to: "showUser".into(),
        ..Default::default()
    }
    .call_tool(&context)
    .await?;
    let text = call_tool_result_text(&result);

    // No stored Calls/Instantiates/Overrides relationship exists between
    // these symbols, so the default BFS reports no path — and must NOT leak
    // any web-mode markers (parity guarantee).
    assert!(
        text.contains("found=false"),
        "expected found=false, got: {text}"
    );
    assert!(
        !text.contains("http_call"),
        "default mode must not emit http_call: {text}"
    );
    assert!(
        !text.contains("external_endpoint"),
        "default mode must not emit external_endpoint: {text}"
    );
    Ok(())
}

#[tokio::test]
async fn impact_web_mode_lists_calling_frontend_symbols() -> Result<()> {
    let (_temp, context) = seeded_context()?;

    let result = BlastRadiusTool {
        symbol_ids: vec!["show_user".into()],
        mode: Some("web".into()),
        ..Default::default()
    }
    .call_tool(&context)
    .await?;
    let text = call_tool_result_text(&result);

    assert!(
        text.contains("Web callers"),
        "expected Web callers section: {text}"
    );
    assert!(
        text.contains("fetchUser"),
        "expected caller fetchUser: {text}"
    );
    assert!(
        text.contains("http_call"),
        "expected http_call label: {text}"
    );
    Ok(())
}

#[tokio::test]
async fn impact_default_mode_omits_web_callers_section() -> Result<()> {
    let (_temp, context) = seeded_context()?;

    let result = BlastRadiusTool {
        symbol_ids: vec!["show_user".into()],
        ..Default::default()
    }
    .call_tool(&context)
    .await?;
    let text = call_tool_result_text(&result);

    assert!(
        !text.contains("Web callers"),
        "default mode must not emit Web callers: {text}"
    );
    Ok(())
}

fn table_fact(
    id: &str,
    file_path: &str,
    line: u32,
    symbol_id: &str,
    table_name: &str,
) -> StructuralFact {
    StructuralFact {
        id: id.into(),
        file_path: file_path.into(),
        language: "sql".into(),
        pattern_id: "sql.table_definition.v1".into(),
        capture_name: "create_table".into(),
        node_kind: "create_table".into(),
        containing_symbol_id: Some(symbol_id.into()),
        start_line: line,
        start_column: 0,
        end_line: line,
        end_column: 50,
        start_byte: line * 10,
        end_byte: line * 10 + 50,
        confidence: 1.0,
        metadata: Some(HashMap::from([
            ("table_name".into(), serde_json::json!(table_name)),
            ("column_count".into(), serde_json::json!(2)),
            ("constraint_count".into(), serde_json::json!(0)),
        ])),
    }
}

fn update_fact(
    id: &str,
    file_path: &str,
    line: u32,
    symbol_id: &str,
    table_name: &str,
) -> StructuralFact {
    StructuralFact {
        id: id.into(),
        file_path: file_path.into(),
        language: "sql".into(),
        pattern_id: "sql.update_statement.v1".into(),
        capture_name: "update".into(),
        node_kind: "update".into(),
        containing_symbol_id: Some(symbol_id.into()),
        start_line: line,
        start_column: 0,
        end_line: line,
        end_column: 40,
        start_byte: line * 10,
        end_byte: line * 10 + 40,
        confidence: 1.0,
        metadata: Some(HashMap::from([
            ("table_name".into(), serde_json::json!(table_name)),
            ("has_where".into(), serde_json::json!(true)),
        ])),
    }
}

/// Build a temp workspace with:
///  - `users` table symbol (schema/tables.sql) defined by a
///    `sql.table_definition.v1` fact.
///  - `touch_users` routine symbol (schema/routines.sql) issuing an
///    `UPDATE users` via a `sql.update_statement.v1` fact attached to the
///    routine.
/// A `sql_query` edge (routine -> table) is derived via `rebuild_web_edges`.
fn seeded_sql_context() -> Result<(TempDir, FakeToolContext)> {
    let temp = TempDir::new()?;
    let db_path = temp.path().join("webnavsql.db");
    let mut db = SymbolDatabase::new(&db_path)?;

    let files = vec![
        file_info_builder("schema/tables.sql")
            .language("sql")
            .build(),
        file_info_builder("schema/routines.sql")
            .language("sql")
            .build(),
    ];
    let symbols = vec![
        symbol_builder("users_table_symbol", "users", "schema/tables.sql")
            .kind(SymbolKind::Class)
            .language("sql")
            .build(),
        symbol_builder("touch_users_proc", "touch_users", "schema/routines.sql")
            .kind(SymbolKind::Method)
            .language("sql")
            .build(),
    ];
    let facts = vec![
        table_fact("t1", "schema/tables.sql", 2, "users_table_symbol", "users"),
        update_fact("u1", "schema/routines.sql", 6, "touch_users_proc", "users"),
    ];

    let write_set = CanonicalWriteSet {
        files: &files,
        symbols: &symbols,
        structural_facts: &facts,
        ..Default::default()
    };
    db.incremental_update_atomic_with_metadata(
        &[
            "schema/tables.sql".to_string(),
            "schema/routines.sql".to_string(),
        ],
        &write_set,
        "webnav-sql-test",
        AtomicPersistenceMetadata::default(),
    )?;
    julie_pipeline::indexing_core::web_edges::rebuild_web_edges(&mut db)?;
    drop(db);

    let context = FakeToolContext::new()
        .with_workspace_id("webnav-sql-test")
        .with_primary_root(temp.path())
        .with_primary_db_path(&db_path);
    Ok((temp, context))
}

#[tokio::test]
async fn trace_web_mode_follows_sql_query_edge_to_table() -> Result<()> {
    let (_temp, context) = seeded_sql_context()?;

    let result = CallPathTool {
        from: "touch_users".into(),
        to: "users".into(),
        mode: Some("web".into()),
        ..Default::default()
    }
    .call_tool(&context)
    .await?;
    let text = call_tool_result_text(&result);

    assert!(
        text.contains("found=true"),
        "expected found=true, got: {text}"
    );
    assert!(
        text.contains("--sql_query-->"),
        "expected an sql_query hop, got: {text}"
    );
    assert!(text.contains("users"), "expected target users, got: {text}");
    Ok(())
}

#[tokio::test]
async fn impact_web_mode_lists_routines_querying_table() -> Result<()> {
    let (_temp, context) = seeded_sql_context()?;

    let result = BlastRadiusTool {
        symbol_ids: vec!["users_table_symbol".into()],
        mode: Some("web".into()),
        ..Default::default()
    }
    .call_tool(&context)
    .await?;
    let text = call_tool_result_text(&result);

    assert!(
        text.contains("Web callers"),
        "expected Web callers section: {text}"
    );
    assert!(
        text.contains("touch_users"),
        "expected caller touch_users: {text}"
    );
    assert!(
        text.contains("sql_query"),
        "expected sql_query label: {text}"
    );
    assert!(
        text.contains("table:users"),
        "expected table:users endpoint: {text}"
    );
    Ok(())
}

#[tokio::test]
async fn trace_default_mode_ignores_sql_query_edge() -> Result<()> {
    let (_temp, context) = seeded_sql_context()?;

    let result = CallPathTool {
        from: "touch_users".into(),
        to: "users".into(),
        ..Default::default()
    }
    .call_tool(&context)
    .await?;
    let text = call_tool_result_text(&result);

    // No stored Calls relationship exists between the routine and the table,
    // so the default BFS reports no path and must not leak sql_query markers.
    assert!(
        text.contains("found=false"),
        "expected found=false, got: {text}"
    );
    assert!(
        !text.contains("sql_query"),
        "default mode must not emit sql_query: {text}"
    );
    Ok(())
}

#[tokio::test]
async fn impact_default_mode_ignores_sql_query_edge() -> Result<()> {
    let (_temp, context) = seeded_sql_context()?;

    let result = BlastRadiusTool {
        symbol_ids: vec!["users_table_symbol".into()],
        ..Default::default()
    }
    .call_tool(&context)
    .await?;
    let text = call_tool_result_text(&result);

    // Default mode must not surface the sql_query reverse edge: no Web callers
    // section, no sql_query label, no table: endpoint marker (byte-identical
    // parity with pre-web-edges behavior).
    assert!(
        !text.contains("Web callers"),
        "default mode must not emit Web callers: {text}"
    );
    assert!(
        !text.contains("sql_query"),
        "default mode must not emit sql_query: {text}"
    );
    assert!(
        !text.contains("table:users"),
        "default mode must not emit table: endpoint marker: {text}"
    );
    Ok(())
}
