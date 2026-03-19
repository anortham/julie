use crate::database::SymbolDatabase;
use tempfile::TempDir;

fn test_db() -> (TempDir, SymbolDatabase) {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let db = SymbolDatabase::new(&db_path).unwrap();
    (tmp, db)
}

#[test]
fn test_insert_tool_call() {
    let (_tmp, db) = test_db();
    db.insert_tool_call(
        "session-123",
        "fast_search",
        4.2,
        Some(5),
        Some(52000),
        Some(1200),
        true,
        Some(r#"{"query":"UserService"}"#),
    )
    .unwrap();

    let count: i32 = db
        .conn
        .query_row("SELECT COUNT(*) FROM tool_calls", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn test_insert_tool_call_with_nulls() {
    let (_tmp, db) = test_db();
    db.insert_tool_call("s1", "deep_dive", 8.0, None, None, None, true, None)
        .unwrap();

    let count: i32 = db
        .conn
        .query_row("SELECT COUNT(*) FROM tool_calls", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn test_query_session_summary() {
    let (_tmp, db) = test_db();
    let session = "sess-abc";

    db.insert_tool_call(
        session,
        "fast_search",
        3.0,
        Some(5),
        Some(10000),
        Some(500),
        true,
        None,
    )
    .unwrap();
    db.insert_tool_call(
        session,
        "fast_search",
        5.0,
        Some(3),
        Some(20000),
        Some(800),
        true,
        None,
    )
    .unwrap();
    db.insert_tool_call(
        session,
        "deep_dive",
        8.0,
        Some(1),
        Some(15000),
        Some(2000),
        true,
        None,
    )
    .unwrap();
    db.insert_tool_call(
        "other-session",
        "fast_search",
        2.0,
        Some(1),
        Some(5000),
        Some(100),
        true,
        None,
    )
    .unwrap();

    let summary = db.query_session_summary(session).unwrap();
    assert_eq!(summary.len(), 2);
    let search = summary
        .iter()
        .find(|s| s.tool_name == "fast_search")
        .unwrap();
    assert_eq!(search.call_count, 2);
    assert_eq!(search.total_source_bytes, 30000);
    assert_eq!(search.total_output_bytes, 1300);
}

#[test]
fn test_query_history_summary() {
    let (_tmp, db) = test_db();

    db.insert_tool_call(
        "s1",
        "fast_search",
        3.0,
        Some(5),
        Some(10000),
        Some(500),
        true,
        None,
    )
    .unwrap();
    db.insert_tool_call(
        "s2",
        "fast_search",
        5.0,
        Some(3),
        Some(20000),
        Some(800),
        true,
        None,
    )
    .unwrap();
    db.insert_tool_call(
        "s1",
        "deep_dive",
        8.0,
        Some(1),
        Some(15000),
        Some(2000),
        true,
        None,
    )
    .unwrap();

    let history = db.query_history_summary(7).unwrap();
    assert_eq!(history.session_count, 2);
    assert_eq!(history.total_calls, 3);
    assert!(history.total_source_bytes > 0);
    assert_eq!(history.per_tool.len(), 2);
    assert!(history.durations_by_tool.contains_key("fast_search"));
    assert_eq!(history.durations_by_tool["fast_search"].len(), 2);
}

#[test]
fn test_query_session_summary_empty() {
    let (_tmp, db) = test_db();
    let summary = db.query_session_summary("nonexistent").unwrap();
    assert!(summary.is_empty());
}
