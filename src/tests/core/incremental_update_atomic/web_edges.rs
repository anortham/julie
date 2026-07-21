use super::*;

use crate::database::{WebEdge, WebEdgeKind};
use julie_extractors::base::StructuralFact;
use julie_pipeline::indexing_core::batch::ExtractedBatch;

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
        metadata: Some(std::collections::HashMap::from([
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
        metadata: Some(std::collections::HashMap::from([
            ("verb".into(), serde_json::json!(verb)),
            (
                "normalized_route_template".into(),
                serde_json::json!(template),
            ),
        ])),
    }
}

/// Persisting client-call + route-handler facts and running the rebuild must
/// produce a matched `http_call` edge in the `web_edges` table, queryable via
/// `web_edges_from_symbol`.
#[test]
fn test_rebuild_web_edges_persists_matched_http_call_edge() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    let client_file = make_file("src/client.ts");
    let handler_file = make_file("src/Controller.php");
    let client_sym = make_symbol("fetch_user", "fetchUser", "src/client.ts");
    let handler_sym = make_symbol("show_user", "showUser", "src/Controller.php");
    let client = client_fact(
        "c1",
        "src/client.ts",
        "typescript",
        3,
        "fetch_user",
        "GET",
        "/api/users/123",
    );
    let handler = route_fact(
        "h1",
        "src/Controller.php",
        "php",
        8,
        "show_user",
        "GET",
        "/api/users/{id}",
    );

    let write_set = CanonicalWriteSet {
        files: &[client_file.clone(), handler_file.clone()],
        symbols: &[client_sym.clone(), handler_sym.clone()],
        structural_facts: &[client.clone(), handler.clone()],
        ..Default::default()
    };
    db.incremental_update_atomic_with_metadata(
        &["src/client.ts".into(), "src/Controller.php".into()],
        &write_set,
        "workspace-a",
        AtomicPersistenceMetadata::default(),
    )
    .unwrap();

    let count = julie_pipeline::indexing_core::web_edges::rebuild_web_edges(&mut db).unwrap();
    assert_eq!(count, 1);

    let edges = db.web_edges_from_symbol("fetch_user").unwrap();
    assert_eq!(edges.len(), 1);
    let e = &edges[0];
    assert_eq!(e.kind, WebEdgeKind::HttpCall);
    assert_eq!(e.from_symbol_id, "fetch_user");
    assert_eq!(e.to_symbol_id.as_deref(), Some("show_user"));
    assert_eq!(e.to_external, None);
    assert_eq!(e.method.as_deref(), Some("GET"));
    assert!((e.confidence - 0.9_f32).abs() < 1e-3);

    // Reverse lookup: impact on the handler lists the calling client.
    let reverse = db.web_edges_to_symbols(&["show_user".into()]).unwrap();
    assert_eq!(reverse.len(), 1);
    assert_eq!(reverse[0].from_symbol_id, "fetch_user");
}

/// A client call with no matching handler degrades to an external-endpoint
/// edge (`to_external` set, `to_symbol_id` None).
#[test]
fn test_rebuild_web_edges_unmatched_call_degrades_to_external() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    let client_file = make_file("src/client.ts");
    let client_sym = make_symbol("fetch_user", "fetchUser", "src/client.ts");
    let client = client_fact(
        "c1",
        "src/client.ts",
        "typescript",
        3,
        "fetch_user",
        "POST",
        "/api/unknown",
    );

    let write_set = CanonicalWriteSet {
        files: std::slice::from_ref(&client_file),
        symbols: std::slice::from_ref(&client_sym),
        structural_facts: std::slice::from_ref(&client),
        ..Default::default()
    };
    db.incremental_update_atomic_with_metadata(
        &["src/client.ts".into()],
        &write_set,
        "workspace-a",
        AtomicPersistenceMetadata::default(),
    )
    .unwrap();

    let count = julie_pipeline::indexing_core::web_edges::rebuild_web_edges(&mut db).unwrap();
    assert_eq!(count, 1);

    let edges = db.web_edges_from_symbol("fetch_user").unwrap();
    assert_eq!(edges.len(), 1);
    let e = &edges[0];
    assert_eq!(e.kind, WebEdgeKind::HttpCall);
    assert_eq!(e.to_symbol_id, None);
    assert_eq!(e.to_external.as_deref(), Some("POST /api/unknown"));
    assert_eq!(e.method.as_deref(), Some("POST"));
}

/// Re-running the rebuild is idempotent and reflects the current fact set
/// (re-derivation replaces the table rather than appending).
#[test]
fn test_rebuild_web_edges_is_idempotent_and_replaces() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    let client_file = make_file("src/client.ts");
    let handler_file = make_file("src/Controller.php");
    let client_sym = make_symbol("fetch_user", "fetchUser", "src/client.ts");
    let handler_sym = make_symbol("show_user", "showUser", "src/Controller.php");
    let client = client_fact(
        "c1",
        "src/client.ts",
        "typescript",
        3,
        "fetch_user",
        "GET",
        "/api/users/123",
    );
    let handler = route_fact(
        "h1",
        "src/Controller.php",
        "php",
        8,
        "show_user",
        "GET",
        "/api/users/{id}",
    );

    let write_set = CanonicalWriteSet {
        files: &[client_file.clone(), handler_file.clone()],
        symbols: &[client_sym.clone(), handler_sym.clone()],
        structural_facts: &[client.clone(), handler.clone()],
        ..Default::default()
    };
    db.incremental_update_atomic_with_metadata(
        &["src/client.ts".into(), "src/Controller.php".into()],
        &write_set,
        "workspace-a",
        AtomicPersistenceMetadata::default(),
    )
    .unwrap();

    // First rebuild: 1 matched edge.
    assert_eq!(
        julie_pipeline::indexing_core::web_edges::rebuild_web_edges(&mut db).unwrap(),
        1
    );
    // Second rebuild with no fact changes: still exactly 1 edge (no duplication).
    assert_eq!(
        julie_pipeline::indexing_core::web_edges::rebuild_web_edges(&mut db).unwrap(),
        1
    );
    assert_eq!(db.web_edge_count().unwrap(), 1);

    // Now delete the handler file's facts (re-persist without the handler).
    let write_set_no_handler = CanonicalWriteSet {
        files: std::slice::from_ref(&client_file),
        symbols: std::slice::from_ref(&client_sym),
        structural_facts: std::slice::from_ref(&client),
        ..Default::default()
    };
    db.incremental_update_atomic_with_metadata(
        &["src/client.ts".into(), "src/Controller.php".into()],
        &write_set_no_handler,
        "workspace-a",
        AtomicPersistenceMetadata::default(),
    )
    .unwrap();

    // Rebuild now yields an external edge (no handler to match).
    assert_eq!(
        julie_pipeline::indexing_core::web_edges::rebuild_web_edges(&mut db).unwrap(),
        1
    );
    let edges: Vec<WebEdge> = db.web_edges_from_symbol("fetch_user").unwrap();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].to_symbol_id, None);
    assert_eq!(edges[0].to_external.as_deref(), Some("GET /api/users/123"));
}

/// Regression (H1): replacing a route-handler file with a NON-web file must
/// not silently drop the cross-file `http_call` edge from another file's
/// client call. The atomic write deletes every `web_edge` touching the
/// replaced file's symbols (including the matched edge from `fetch_user`);
/// the post-replace rebuild must re-derive it as an external-endpoint edge.
/// Previously the rebuild was gated on the NEW facts (which carry no web
/// patterns after the replace), so it was skipped and the edge vanished until
/// the next full reindex — a silent correctness regression in `--mode web`.
#[test]
fn test_replace_handler_with_non_web_file_degrades_cross_file_edge_to_external() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test_h1.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Seed: client call (src/client.ts) + handler (src/Controller.php).
    let client_file = make_file("src/client.ts");
    let handler_file = make_file("src/Controller.php");
    let client_sym = make_symbol("fetch_user", "fetchUser", "src/client.ts");
    let handler_sym = make_symbol("show_user", "showUser", "src/Controller.php");
    let client = client_fact(
        "c1",
        "src/client.ts",
        "typescript",
        3,
        "fetch_user",
        "GET",
        "/api/users/123",
    );
    let handler = route_fact(
        "h1",
        "src/Controller.php",
        "php",
        8,
        "show_user",
        "GET",
        "/api/users/{id}",
    );

    let mut batch = ExtractedBatch::new();
    batch.all_file_infos = vec![client_file.clone(), handler_file.clone()];
    batch.all_symbols = vec![client_sym.clone(), handler_sym.clone()];
    batch.all_structural_facts = vec![client.clone(), handler.clone()];
    batch.files_to_clean = vec!["src/client.ts".into(), "src/Controller.php".into()];
    julie_pipeline::indexing_core::persistence::persist_incremental_scan(
        &mut db,
        "ws-h1",
        &batch,
        &[],
    )
    .unwrap();

    // Matched edge exists after the initial index.
    let edges: Vec<WebEdge> = db.web_edges_from_symbol("fetch_user").unwrap();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].to_symbol_id.as_deref(), Some("show_user"));

    // Replace the handler file with a NON-web file: same path, same symbol, but
    // NO structural facts (the route annotation was removed).
    let mut replace = ExtractedBatch::new();
    replace.all_file_infos = vec![handler_file.clone()];
    replace.all_symbols = vec![handler_sym.clone()];
    replace.all_structural_facts = vec![]; // no web facts in the new file
    replace.files_to_clean = vec!["src/Controller.php".into()];
    julie_pipeline::indexing_core::persistence::persist_single_file_replace(
        &mut db, "ws-h1", &replace,
    )
    .unwrap();

    // The cross-file client edge must still exist, degraded to external. If the
    // rebuild had been gated on the new (web-less) facts, this would be empty.
    let edges: Vec<WebEdge> = db.web_edges_from_symbol("fetch_user").unwrap();
    assert_eq!(
        edges.len(),
        1,
        "cross-file http_call edge must not be silently dropped on web->non-web replace"
    );
    assert_eq!(
        edges[0].to_symbol_id, None,
        "edge must degrade to external (handler fact gone)"
    );
    assert_eq!(edges[0].to_external.as_deref(), Some("GET /api/users/123"));
}

/// Regression (watcher delete): deleting a route-handler file via the live
/// watcher path must degrade the cross-file `http_call` to an external
/// endpoint. Persistence delete already rebuilds; `handle_file_deleted_static`
/// must too — otherwise FK `ON DELETE SET NULL` leaves a dangling edge
/// (`to_symbol_id` null, `to_external` null) until some later save rebuilds.
#[tokio::test]
async fn test_watcher_delete_handler_degrades_cross_file_edge_to_external() {
    use std::sync::{Arc, Mutex};

    use crate::watcher::handlers::handle_file_deleted_static;
    use crate::workspace::mutation_gate::acquire_gate;

    let tmp = TempDir::new().unwrap();
    let workspace_root = tmp.path().canonicalize().unwrap();
    let db_path = workspace_root.join("test_watcher_delete.db");
    let db = Arc::new(Mutex::new(SymbolDatabase::new(&db_path).unwrap()));

    let client_file = make_file("src/client.ts");
    let handler_file = make_file("src/Controller.php");
    let client_sym = make_symbol("fetch_user", "fetchUser", "src/client.ts");
    let handler_sym = make_symbol("show_user", "showUser", "src/Controller.php");
    let client = client_fact(
        "c1",
        "src/client.ts",
        "typescript",
        3,
        "fetch_user",
        "GET",
        "/api/users/123",
    );
    let handler = route_fact(
        "h1",
        "src/Controller.php",
        "php",
        8,
        "show_user",
        "GET",
        "/api/users/{id}",
    );

    {
        let mut db_lock = db.lock().unwrap();
        let write_set = CanonicalWriteSet {
            files: &[client_file, handler_file],
            symbols: &[client_sym, handler_sym],
            structural_facts: &[client, handler],
            ..Default::default()
        };
        db_lock
            .incremental_update_atomic_with_metadata(
                &["src/client.ts".into(), "src/Controller.php".into()],
                &write_set,
                "ws-watcher-delete",
                AtomicPersistenceMetadata::default(),
            )
            .unwrap();
        assert_eq!(
            julie_pipeline::indexing_core::web_edges::rebuild_web_edges(&mut *db_lock).unwrap(),
            1
        );
        let edges = db_lock.web_edges_from_symbol("fetch_user").unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].to_symbol_id.as_deref(), Some("show_user"));
    }

    let handler_abs = workspace_root.join("src/Controller.php");
    let guard = acquire_gate("test_watcher_web_edges_delete").await;
    handle_file_deleted_static(handler_abs, &db, &workspace_root, None, &guard)
        .await
        .expect("watcher delete must succeed");

    let db_lock = db.lock().unwrap();
    let edges: Vec<WebEdge> = db_lock.web_edges_from_symbol("fetch_user").unwrap();
    assert_eq!(
        edges.len(),
        1,
        "cross-file http_call must survive watcher delete of the handler file"
    );
    assert_eq!(
        edges[0].to_symbol_id, None,
        "matched target must be cleared after handler delete"
    );
    assert_eq!(
        edges[0].to_external.as_deref(),
        Some("GET /api/users/123"),
        "edge must degrade to external after rebuild; dangling (null,null) means rebuild was skipped"
    );
}

/// Two HTTP edges from the same symbol/line/method targeting the same
/// handler with different `path` values must both survive
/// `replace_all_web_edges`. The edge primary key previously omitted
/// `path`/`table`, so `INSERT OR REPLACE` silently dropped one whenever
/// `to` (symbol or external) matched.
#[test]
fn test_edge_id_distinguishes_distinct_paths() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("edge_id.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    let client_file = make_file("src/client.ts");
    let handler_file = make_file("src/Controller.php");
    let client_sym = make_symbol("fetch_user", "fetchUser", "src/client.ts");
    let handler_sym = make_symbol("show_user", "showUser", "src/Controller.php");
    let write_set = CanonicalWriteSet {
        files: &[client_file, handler_file],
        symbols: &[client_sym, handler_sym],
        ..Default::default()
    };
    db.incremental_update_atomic_with_metadata(
        &["src/client.ts".into(), "src/Controller.php".into()],
        &write_set,
        "ws-edge-id",
        AtomicPersistenceMetadata::default(),
    )
    .unwrap();

    // Same from / to_symbol / file / line / method — only `path` differs.
    let edges = vec![
        WebEdge {
            from_symbol_id: "fetch_user".into(),
            to_symbol_id: Some("show_user".into()),
            to_external: None,
            kind: WebEdgeKind::HttpCall,
            method: Some("GET".into()),
            path: Some("/api/a".into()),
            table: None,
            file_path: "src/client.ts".into(),
            line_number: 10,
            confidence: 0.9,
            metadata: None,
        },
        WebEdge {
            from_symbol_id: "fetch_user".into(),
            to_symbol_id: Some("show_user".into()),
            to_external: None,
            kind: WebEdgeKind::HttpCall,
            method: Some("GET".into()),
            path: Some("/api/b".into()),
            table: None,
            file_path: "src/client.ts".into(),
            line_number: 10,
            confidence: 0.9,
            metadata: None,
        },
    ];
    db.replace_all_web_edges(&edges).unwrap();

    let stored = db.web_edges_from_symbol("fetch_user").unwrap();
    assert_eq!(
        stored.len(),
        2,
        "distinct paths must not collide under edge_id INSERT OR REPLACE"
    );
    let mut paths: Vec<_> = stored.iter().filter_map(|e| e.path.as_deref()).collect();
    paths.sort_unstable();
    assert_eq!(paths, vec!["/api/a", "/api/b"]);
}
