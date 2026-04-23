use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use tempfile::TempDir;

use crate::database::types::FileInfo;
use crate::database::{ProjectionStatus, SymbolDatabase};
use crate::extractors::{Symbol, SymbolKind};
use crate::search::{FileDocument, SearchIndex, SearchProjection, SymbolDocument};

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

fn make_file_without_content(path: &str, language: &str) -> FileInfo {
    FileInfo {
        path: path.to_string(),
        language: language.to_string(),
        hash: format!("hash_{path}"),
        size: 0,
        last_modified: 1000,
        last_indexed: 0,
        symbol_count: 0,
        line_count: 0,
        content: None,
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
        annotations: Vec::new(),
    }
}

#[test]
fn test_search_projection_preserves_existing_docs_across_legacy_upgrade() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("symbols.db");
    let index_path = temp_dir.path().join("tantivy");
    std::fs::create_dir_all(&index_path)?;

    {
        let mut db = SymbolDatabase::new(&db_path)?;
        let index = SearchIndex::open_or_create(&index_path)?;
        let projection = SearchProjection::tantivy("ws_test");

        db.bulk_store_fresh_atomic(
            &[make_file("src/lib.rs", "fn legacy_symbol() {}\n")],
            &[make_symbol("sym_legacy", "legacy_symbol", "src/lib.rs")],
            &[],
            &[],
            &[],
            "ws_test",
        )?;
        projection.ensure_current_from_database(&mut db, &index)?;
        assert_eq!(
            index.num_docs(),
            2,
            "fixture setup should create Tantivy docs"
        );
    }

    {
        let conn = rusqlite::Connection::open(&db_path)?;
        conn.execute("DELETE FROM schema_version WHERE version >= 15", [])?;
        conn.execute("DROP TABLE IF EXISTS indexing_repairs", [])?;
        conn.execute("DROP TABLE IF EXISTS canonical_revisions", [])?;
        conn.execute("DROP TABLE IF EXISTS projection_states", [])?;
        conn.execute(
            "INSERT OR REPLACE INTO schema_version (version, applied_at, description)
             VALUES (14, 0, 'legacy fixture')",
            [],
        )?;
    }

    let mut upgraded_db = SymbolDatabase::new(&db_path)?;
    let upgraded_index = SearchIndex::open_or_create(&index_path)?;
    let projection = SearchProjection::tantivy("ws_test");

    let state = projection.ensure_current_from_database(&mut upgraded_db, &upgraded_index)?;

    assert_eq!(
        upgraded_index.num_docs(),
        2,
        "upgrade repair should preserve the existing Tantivy symbol and file docs"
    );
    assert_eq!(state.status.as_str(), "ready");

    let results = upgraded_index.search_symbols("legacy_symbol", &Default::default(), 10)?;
    assert_eq!(
        results.results.len(),
        1,
        "legacy symbol should still be searchable"
    );
    assert_eq!(results.results[0].name, "legacy_symbol");
    Ok(())
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
fn test_search_projection_repairs_recreated_open_from_canonical_sqlite() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("symbols.db");
    let index_path = temp_dir.path().join("tantivy");
    std::fs::create_dir_all(&index_path)?;

    let mut db = SymbolDatabase::new(&db_path)?;
    let projection = SearchProjection::tantivy("ws_test");

    {
        let index = SearchIndex::open_or_create(&index_path)?;
        db.bulk_store_fresh_atomic(
            &[make_file("src/lib.rs", "fn repaired_symbol() {}\n")],
            &[make_symbol("sym_repair", "repaired_symbol", "src/lib.rs")],
            &[],
            &[],
            &[],
            "ws_test",
        )?;
        projection.ensure_current_from_database(&mut db, &index)?;
        assert_eq!(index.num_docs(), 2, "fixture setup should create docs");
    }

    let compat_marker_path = index_path.join("julie-search-compat.json");
    assert!(compat_marker_path.exists(), "compat marker should exist");
    std::fs::remove_file(&compat_marker_path)?;

    let configs = crate::search::LanguageConfigs::load_embedded();
    let open_outcome =
        SearchIndex::open_or_create_with_language_configs_outcome(&index_path, &configs)?;
    let repair_required = open_outcome.repair_required();
    assert!(
        repair_required,
        "missing compat marker should force recreated-open repair"
    );

    let reopened_index = open_outcome.into_index();
    assert_eq!(
        reopened_index.num_docs(),
        0,
        "recreated open should start empty before repair"
    );

    projection.repair_recreated_open_if_needed(&mut db, &reopened_index, repair_required, None)?;

    assert_eq!(
        reopened_index.num_docs(),
        2,
        "repair should repopulate symbol and file docs from SQLite"
    );

    let results = reopened_index.search_symbols("repaired_symbol", &Default::default(), 10)?;
    assert_eq!(
        results.results.len(),
        1,
        "repaired open should restore searchability"
    );
    assert_eq!(results.results[0].name, "repaired_symbol");
    Ok(())
}

#[test]
fn test_search_projection_indexes_file_rows_without_content_for_file_mode() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("symbols.db");
    let index_path = temp_dir.path().join("tantivy");
    std::fs::create_dir_all(&index_path)?;

    let mut db = SymbolDatabase::new(&db_path)?;
    let index = SearchIndex::open_or_create(&index_path)?;
    let projection = SearchProjection::tantivy("ws_test");

    db.store_file_info(&make_file_without_content("docs/guide.md", "markdown"))?;
    let raw = rusqlite::Connection::open(&db_path)?;
    raw.execute(
        "UPDATE files SET content = NULL WHERE path = 'docs/guide.md'",
        [],
    )?;

    let state = projection.ensure_current_from_database(&mut db, &index)?;

    assert_eq!(state.status, ProjectionStatus::Ready);
    assert_eq!(
        index.num_docs(),
        1,
        "content-less file rows should still produce a Tantivy file doc"
    );

    let results = index.search_files("guide.md", &Default::default(), 10)?;
    assert_eq!(results.results.len(), 1);
    assert_eq!(results.results[0].file_path, "docs/guide.md");
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

#[test]
fn test_search_projection_does_not_rebuild_when_current_revision_is_already_projected() -> Result<()>
{
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("symbols.db");
    let index_path = temp_dir.path().join("tantivy");
    std::fs::create_dir_all(&index_path)?;
    let mut db = SymbolDatabase::new(&db_path)?;
    let index = SearchIndex::open_or_create(&index_path)?;
    let projection = SearchProjection::tantivy("ws_test");

    db.bulk_store_fresh_atomic(
        &[
            make_file("src/lib.rs", "fn first_symbol() {}\n"),
            make_file("src/other.rs", "fn other_symbol() {}\n"),
        ],
        &[
            make_symbol("sym_1", "first_symbol", "src/lib.rs"),
            make_symbol("sym_2", "other_symbol", "src/other.rs"),
        ],
        &[],
        &[],
        &[],
        "ws_test",
    )?;
    projection.ensure_current_from_database(&mut db, &index)?;
    assert_eq!(
        index.num_docs(),
        4,
        "initial projection should index both files"
    );

    let updated_file = make_file("src/lib.rs", "fn second_symbol() {}\n");
    let updated_symbol = make_symbol("sym_3", "second_symbol", "src/lib.rs");
    db.incremental_update_atomic(
        &["src/lib.rs".to_string()],
        std::slice::from_ref(&updated_file),
        std::slice::from_ref(&updated_symbol),
        &[],
        &[],
        &[],
        "ws_test",
    )?;
    let target_revision = db
        .get_current_canonical_revision("ws_test")?
        .expect("incremental update should advance canonical revision");
    projection.project_documents(
        &mut db,
        &index,
        &[SymbolDocument::from_symbol(&updated_symbol)],
        &[FileDocument::from_file_info(&updated_file)],
        &["src/lib.rs".to_string()],
        Some(target_revision),
    )?;

    let before = db
        .get_projection_state("tantivy", "ws_test")?
        .expect("projection state should exist after project_documents");
    std::thread::sleep(std::time::Duration::from_secs(1));

    let state = projection.ensure_current_from_database(&mut db, &index)?;
    let after = db
        .get_projection_state("tantivy", "ws_test")?
        .expect("projection state should still exist after verification");

    assert_eq!(state.canonical_revision, Some(target_revision));
    assert_eq!(
        before.updated_at, after.updated_at,
        "current projection should return existing ready state without rewriting projection metadata"
    );
    assert_eq!(
        index.num_docs(),
        4,
        "current projection should not rebuild the full index"
    );

    let results = index.search_symbols("other_symbol", &Default::default(), 10)?;
    assert_eq!(
        results.results.len(),
        1,
        "other file docs should remain intact"
    );
    assert_eq!(results.results[0].name, "other_symbol");
    Ok(())
}

// B-I6: ensure_current_with_gate must flip search_ready during rebuild and
// leave it TRUE only when the projection ends up Ready.
#[test]
fn test_ensure_current_with_gate_flips_search_ready_on_rebuild() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("symbols.db");
    let index_path = temp_dir.path().join("tantivy");
    std::fs::create_dir_all(&index_path)?;

    let mut db = SymbolDatabase::new(&db_path)?;
    let index = SearchIndex::open_or_create(&index_path)?;
    let projection = SearchProjection::tantivy("ws_test");

    db.bulk_store_fresh_atomic(
        &[make_file("src/lib.rs", "fn alpha() {}\n")],
        &[make_symbol("sym_alpha", "alpha", "src/lib.rs")],
        &[],
        &[],
        &[],
        "ws_test",
    )?;

    // Precondition: Tantivy empty; search_ready starts false.
    assert_eq!(index.num_docs(), 0);
    let search_ready = AtomicBool::new(false);

    let state = projection.ensure_current_with_gate(&mut db, &index, &search_ready)?;

    assert_eq!(state.status, ProjectionStatus::Ready);
    assert!(
        search_ready.load(Ordering::Acquire),
        "search_ready must be TRUE after a successful rebuild ends in Ready state"
    );
    assert_eq!(index.num_docs(), 2);
    Ok(())
}

// B-I6: when the projection cannot become Ready (empty workspace → Missing),
// search_ready must remain FALSE so consumers don't query an empty index.
#[test]
fn test_ensure_current_with_gate_keeps_search_ready_false_on_missing() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("symbols.db");
    let index_path = temp_dir.path().join("tantivy");
    std::fs::create_dir_all(&index_path)?;

    let mut db = SymbolDatabase::new(&db_path)?;
    let index = SearchIndex::open_or_create(&index_path)?;
    let projection = SearchProjection::tantivy("ws_empty");

    let search_ready = AtomicBool::new(true);

    let state = projection.ensure_current_with_gate(&mut db, &index, &search_ready)?;

    assert_eq!(state.status, ProjectionStatus::Missing);
    assert!(
        !search_ready.load(Ordering::Acquire),
        "search_ready must be FALSE when projection ends up Missing"
    );
    Ok(())
}
