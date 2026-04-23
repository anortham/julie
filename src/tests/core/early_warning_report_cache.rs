use crate::analysis::early_warnings::{EarlyWarningReportOptions, generate_early_warning_report};
use crate::database::{FileInfo, ProjectionStatus, SymbolDatabase};
use crate::extractors::{AnnotationMarker, Symbol, SymbolKind};
use crate::search::language_config::LanguageConfigs;
use rusqlite::{Connection, params};
use tempfile::TempDir;

fn file_info(path: &str, language: &str) -> FileInfo {
    FileInfo {
        path: path.to_string(),
        language: language.to_string(),
        hash: format!("hash-{path}"),
        size: 128,
        last_modified: 1_700_000_000,
        last_indexed: 0,
        symbol_count: 1,
        line_count: 12,
        content: Some(format!("// {path}")),
    }
}

fn marker(annotation: &str, annotation_key: &str, raw_text: Option<&str>) -> AnnotationMarker {
    AnnotationMarker {
        annotation: annotation.to_string(),
        annotation_key: annotation_key.to_string(),
        raw_text: raw_text.map(str::to_string),
        carrier: None,
    }
}

fn symbol(id: &str, name: &str, file_path: &str, start_line: u32) -> Symbol {
    Symbol {
        id: id.to_string(),
        name: name.to_string(),
        kind: SymbolKind::Method,
        language: "csharp".to_string(),
        file_path: file_path.to_string(),
        start_line,
        start_column: 4,
        end_line: start_line + 3,
        end_column: 1,
        start_byte: 20,
        end_byte: 80,
        signature: Some(format!("{name}()")),
        doc_comment: None,
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: Some(1.0),
        code_context: Some(format!("{name}() {{}}")),
        content_type: None,
        annotations: vec![marker("HttpGet", "httpget", Some("[HttpGet]"))],
    }
}

fn open_db(name: &str) -> (TempDir, SymbolDatabase) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join(name);
    let db = SymbolDatabase::new(&db_path).unwrap();
    (temp_dir, db)
}

fn table_count(db: &SymbolDatabase) -> i64 {
    db.conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type = 'table' AND name = 'early_warning_reports'",
            [],
            |row| row.get(0),
        )
        .unwrap()
}

fn report_indexes(db: &SymbolDatabase) -> Vec<String> {
    let mut stmt = db
        .conn
        .prepare(
            "SELECT name FROM sqlite_master
             WHERE type = 'index' AND tbl_name = 'early_warning_reports'
             ORDER BY name",
        )
        .unwrap();
    stmt.query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
}

fn cache_row_count(db: &SymbolDatabase) -> i64 {
    db.conn
        .query_row("SELECT COUNT(*) FROM early_warning_reports", [], |row| {
            row.get(0)
        })
        .unwrap()
}

fn options(fresh: bool) -> EarlyWarningReportOptions {
    EarlyWarningReportOptions {
        workspace_id: "ws-cache".to_string(),
        file_pattern: None,
        fresh,
        limit_per_section: None,
    }
}

fn seed_revision(db: &mut SymbolDatabase, path: &str, symbol_id: &str) -> i64 {
    let file = file_info(path, "csharp");
    let symbol = symbol(symbol_id, "GetStatus", path, 5);
    db.bulk_store_fresh_atomic(&[file], &[symbol], &[], &[], &[], "ws-cache")
        .unwrap();
    let revision = db
        .get_latest_canonical_revision("ws-cache")
        .unwrap()
        .unwrap()
        .revision;
    db.upsert_projection_state(
        "tantivy",
        "ws-cache",
        ProjectionStatus::Ready,
        Some(revision),
        Some(revision),
        None,
    )
    .unwrap();
    revision
}

fn seed_revision_with_two_routes(db: &mut SymbolDatabase) -> i64 {
    let path = "Controllers/StatusController.cs";
    let file = file_info(path, "csharp");
    let symbols = [
        symbol("status-v1", "GetStatus", path, 5),
        symbol("health-v1", "GetHealth", path, 12),
    ];
    db.bulk_store_fresh_atomic(&[file], &symbols, &[], &[], &[], "ws-cache")
        .unwrap();
    let revision = db
        .get_latest_canonical_revision("ws-cache")
        .unwrap()
        .unwrap()
        .revision;
    db.upsert_projection_state(
        "tantivy",
        "ws-cache",
        ProjectionStatus::Ready,
        Some(revision),
        Some(revision),
        None,
    )
    .unwrap();
    revision
}

#[test]
fn fresh_database_creates_early_warning_reports() {
    let (_temp_dir, db) = open_db("fresh.db");
    let indexes = report_indexes(&db);

    assert_eq!(table_count(&db), 1);
    assert!(
        indexes
            .iter()
            .any(|name| name == "idx_early_warning_reports_workspace_generated")
    );
    assert!(
        indexes
            .iter()
            .any(|name| name == "idx_early_warning_reports_cache_key")
    );
}

#[test]
fn migration_021_creates_early_warning_reports_for_existing_database() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("migrated.db");

    {
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_version (
                version INTEGER PRIMARY KEY,
                applied_at INTEGER NOT NULL,
                description TEXT NOT NULL
            );
            INSERT INTO schema_version (version, applied_at, description)
            VALUES (20, 1, 'Add symbol_annotations table');",
        )
        .unwrap();
    }

    let db = SymbolDatabase::new(&db_path).unwrap();
    let indexes = report_indexes(&db);

    assert_eq!(
        db.get_schema_version().unwrap(),
        crate::database::LATEST_SCHEMA_VERSION
    );
    assert_eq!(table_count(&db), 1);
    assert!(
        indexes
            .iter()
            .any(|name| name == "idx_early_warning_reports_workspace_generated")
    );
    assert!(
        indexes
            .iter()
            .any(|name| name == "idx_early_warning_reports_cache_key")
    );
}

#[test]
fn early_warning_report_cache_invalidation() {
    let (_temp_dir, mut db) = open_db("cache.db");
    let configs = LanguageConfigs::load_embedded();
    let first_revision = seed_revision(&mut db, "Controllers/StatusController.cs", "status-v1");

    let first = generate_early_warning_report(&db, &configs, options(false)).unwrap();
    let second = generate_early_warning_report(&db, &configs, options(false)).unwrap();

    assert!(!first.from_cache);
    assert!(second.from_cache);
    assert_eq!(first.generated_at, second.generated_at);
    assert_eq!(first.canonical_revision, first_revision);
    assert_eq!(cache_row_count(&db), 1);

    let second_revision = seed_revision(&mut db, "Controllers/StatusControllerV2.cs", "status-v2");
    let after_revision_change =
        generate_early_warning_report(&db, &configs, options(false)).unwrap();

    assert!(!after_revision_change.from_cache);
    assert_eq!(after_revision_change.canonical_revision, second_revision);
    assert_eq!(cache_row_count(&db), 1);

    db.conn
        .execute(
            "UPDATE early_warning_reports
             SET config_schema_version = 0
             WHERE workspace_id = ?1 AND canonical_revision = ?2",
            params!["ws-cache", second_revision],
        )
        .unwrap();
    let after_config_change = generate_early_warning_report(&db, &configs, options(false)).unwrap();

    assert!(!after_config_change.from_cache);
    assert_eq!(after_config_change.config_schema_version, 1);
    assert_eq!(cache_row_count(&db), 1);

    db.conn
        .execute(
            "UPDATE early_warning_reports
             SET generated_at = 1
             WHERE workspace_id = ?1
               AND canonical_revision = ?2
               AND config_schema_version = ?3",
            params![
                "ws-cache",
                second_revision,
                after_config_change.config_schema_version
            ],
        )
        .unwrap();
    let refreshed = generate_early_warning_report(&db, &configs, options(true)).unwrap();
    let stored_generated_at: i64 = db
        .conn
        .query_row(
            "SELECT generated_at FROM early_warning_reports
             WHERE workspace_id = ?1
               AND canonical_revision = ?2
               AND config_schema_version = ?3",
            params!["ws-cache", second_revision, refreshed.config_schema_version],
            |row| row.get(0),
        )
        .unwrap();

    assert!(!refreshed.from_cache);
    assert!(refreshed.generated_at > 1);
    assert_eq!(stored_generated_at, refreshed.generated_at);
    assert_eq!(cache_row_count(&db), 1);
}

#[test]
fn early_warning_report_cache_stores_full_rows_before_limit() {
    let (_temp_dir, mut db) = open_db("cache-limit.db");
    let configs = LanguageConfigs::load_embedded();
    seed_revision_with_two_routes(&mut db);

    let limited = generate_early_warning_report(
        &db,
        &configs,
        EarlyWarningReportOptions {
            limit_per_section: Some(1),
            ..options(false)
        },
    )
    .unwrap();
    let unbounded = generate_early_warning_report(&db, &configs, options(false)).unwrap();

    assert!(!limited.from_cache);
    assert_eq!(limited.summary.entry_points, 2);
    assert_eq!(limited.entry_points.len(), 1);
    assert!(unbounded.from_cache);
    assert_eq!(unbounded.summary.entry_points, 2);
    assert_eq!(unbounded.entry_points.len(), 2);
    assert_eq!(cache_row_count(&db), 1);
}
