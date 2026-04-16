use anyhow::Result;
use tempfile::TempDir;

use crate::database::SymbolDatabase;
use crate::database::types::FileInfo;
use crate::extractors::{Symbol, SymbolKind};
use crate::search::{SearchIndex, SearchProjection};

fn make_file(path: &str, content: &str) -> FileInfo {
    FileInfo {
        path: path.to_string(),
        language: "rust".to_string(),
        hash: format!("hash_{path}"),
        size: content.len() as i64,
        last_modified: 1000,
        last_indexed: 0,
        symbol_count: 1,
        line_count: content.lines().count() as i32,
        content: Some(content.to_string()),
    }
}

fn make_symbol(id: &str, name: &str, file_path: &str) -> Symbol {
    Symbol {
        id: id.to_string(),
        name: name.to_string(),
        kind: SymbolKind::Function,
        language: "rust".to_string(),
        file_path: file_path.to_string(),
        start_line: 1,
        start_column: 0,
        end_line: 3,
        end_column: 0,
        start_byte: 0,
        end_byte: 32,
        signature: Some(format!("fn {}()", name)),
        doc_comment: None,
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: Some(format!("fn {}() {{}}", name)),
        content_type: None,
    }
}

#[test]
fn test_search_projection_rebuilds_empty_index_from_canonical_sqlite() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("symbols.db");
    let index_path = temp_dir.path().join("tantivy");
    std::fs::create_dir_all(&index_path)?;
    let mut db = SymbolDatabase::new(&db_path)?;
    let index = SearchIndex::open_or_create(&index_path)?;
    let projection = SearchProjection::tantivy("ws_test");

    db.bulk_store_fresh_atomic(
        &[make_file("src/lib.rs", "fn current_symbol() {}\n")],
        &[make_symbol("sym_1", "current_symbol", "src/lib.rs")],
        &[],
        &[],
        &[],
        "ws_test",
    )?;

    let state = projection.ensure_current_from_database(&mut db, &index)?;

    assert_eq!(
        index.num_docs(),
        2,
        "rebuild should repopulate symbol and file docs"
    );
    assert_eq!(state.canonical_revision, Some(1));
    assert_eq!(state.status.as_str(), "ready");
    Ok(())
}

#[test]
fn test_search_projection_marks_stale_when_projection_write_fails() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("symbols.db");
    let index_path = temp_dir.path().join("tantivy");
    std::fs::create_dir_all(&index_path)?;
    let mut db = SymbolDatabase::new(&db_path)?;
    let index = SearchIndex::open_or_create(&index_path)?;
    let projection = SearchProjection::tantivy("ws_test");

    db.bulk_store_fresh_atomic(
        &[make_file("src/lib.rs", "fn current_symbol() {}\n")],
        &[make_symbol("sym_1", "current_symbol", "src/lib.rs")],
        &[],
        &[],
        &[],
        "ws_test",
    )?;

    index.shutdown()?;
    let err = projection
        .ensure_current_from_database(&mut db, &index)
        .unwrap_err();
    assert!(
        !err.to_string().is_empty(),
        "projection failure should surface a real error"
    );

    let state = db
        .get_projection_state("tantivy", "ws_test")?
        .expect("failed projection should persist stale state");
    assert_eq!(state.canonical_revision, Some(1));
    assert_eq!(state.status.as_str(), "stale");
    Ok(())
}

#[test]
fn test_search_projection_rebuilds_after_canonical_revision_advances() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("symbols.db");
    let index_path = temp_dir.path().join("tantivy");
    std::fs::create_dir_all(&index_path)?;
    let mut db = SymbolDatabase::new(&db_path)?;
    let index = SearchIndex::open_or_create(&index_path)?;
    let projection = SearchProjection::tantivy("ws_test");

    db.bulk_store_fresh_atomic(
        &[make_file("src/lib.rs", "fn first_symbol() {}\n")],
        &[make_symbol("sym_1", "first_symbol", "src/lib.rs")],
        &[],
        &[],
        &[],
        "ws_test",
    )?;
    let initial = projection.ensure_current_from_database(&mut db, &index)?;
    assert_eq!(initial.canonical_revision, Some(1));

    db.incremental_update_atomic(
        &["src/lib.rs".to_string()],
        &[make_file("src/lib.rs", "fn second_symbol() {}\n")],
        &[make_symbol("sym_2", "second_symbol", "src/lib.rs")],
        &[],
        &[],
        &[],
        "ws_test",
    )?;

    let rebuilt = projection.ensure_current_from_database(&mut db, &index)?;
    assert_eq!(rebuilt.canonical_revision, Some(2));

    let results = index.search_symbols("second_symbol", &Default::default(), 10)?;
    assert_eq!(
        results.results.len(),
        1,
        "rebuilt projection should serve new symbol"
    );
    assert_eq!(results.results[0].name, "second_symbol");
    Ok(())
}
