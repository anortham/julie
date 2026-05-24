use std::collections::HashMap;
use std::fs;

use tempfile::TempDir;

use crate::database::SymbolDatabase;
use crate::database::types::FileInfo;
use crate::external_extract::operations::{
    run_external_analyze, run_external_delete, run_external_info, run_external_scan,
    run_external_update,
};
use crate::external_extract::{
    EXTRACT_CONTRACT_VERSION, ExternalExtractArgs, ExternalExtractCommand, ExternalInfoSchemaState,
    read_external_extract_info,
};
use crate::extractors::{Identifier, IdentifierKind, Symbol, SymbolKind};
use crate::indexing_core::batch::ExtractedBatch;
use crate::indexing_core::extraction::extract_files_for_indexing;
use crate::indexing_core::persistence::{
    persist_force_rebuild, persist_incremental_scan, persist_single_file_delete,
};

fn make_file(path: &str, hash: &str) -> FileInfo {
    FileInfo {
        path: path.to_string(),
        language: "rust".to_string(),
        hash: hash.to_string(),
        size: 200,
        last_modified: 1000,
        last_indexed: 0,
        symbol_count: 1,
        line_count: 10,
        content: Some(format!("// {path}")),
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
        end_byte: 30,
        signature: None,
        doc_comment: None,
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: None,
        content_type: None,
        body_span: None,
        body_hash: None,
        annotations: Vec::new(),
    }
}

fn make_identifier_with_target(
    id: &str,
    file_path: &str,
    containing_symbol_id: &str,
    target_symbol_id: &str,
) -> Identifier {
    Identifier {
        id: id.to_string(),
        name: "target_call".to_string(),
        kind: IdentifierKind::Call,
        language: "rust".to_string(),
        file_path: file_path.to_string(),
        start_line: 2,
        start_column: 4,
        end_line: 2,
        end_column: 16,
        start_byte: 10,
        end_byte: 22,
        containing_symbol_id: Some(containing_symbol_id.to_string()),
        target_symbol_id: Some(target_symbol_id.to_string()),
        confidence: 1.0,
        code_context: None,
    }
}

fn batch_for(files: Vec<FileInfo>, symbols: Vec<Symbol>) -> ExtractedBatch {
    let mut batch = ExtractedBatch::new();
    batch.files_to_clean = files.iter().map(|file| file.path.clone()).collect();
    batch.all_file_infos = files;
    batch.all_symbols = symbols;
    batch.files_processed = batch.all_file_infos.len();
    batch
}

fn count_rows(db: &SymbolDatabase, table: &str) -> i64 {
    db.conn
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .expect("count rows")
}

fn scan_args(db: std::path::PathBuf, root: std::path::PathBuf, force: bool) -> ExternalExtractArgs {
    ExternalExtractArgs {
        db,
        root: Some(root),
        strict_schema: false,
        ignore_files: Vec::new(),
        workspace_id: Some("external_ws".to_string()),
        analyze: false,
        command: ExternalExtractCommand::Scan { force },
    }
}

fn update_args(
    db: std::path::PathBuf,
    root: std::path::PathBuf,
    file: std::path::PathBuf,
    ignore_files: Vec<std::path::PathBuf>,
) -> ExternalExtractArgs {
    ExternalExtractArgs {
        db,
        root: Some(root),
        strict_schema: false,
        ignore_files,
        workspace_id: Some("external_ws".to_string()),
        analyze: false,
        command: ExternalExtractCommand::Update { file },
    }
}

fn delete_args(
    db: std::path::PathBuf,
    root: std::path::PathBuf,
    file: std::path::PathBuf,
) -> ExternalExtractArgs {
    ExternalExtractArgs {
        db,
        root: Some(root),
        strict_schema: false,
        ignore_files: Vec::new(),
        workspace_id: Some("external_ws".to_string()),
        analyze: false,
        command: ExternalExtractCommand::Delete { file },
    }
}

fn analyze_args(db: std::path::PathBuf) -> ExternalExtractArgs {
    ExternalExtractArgs {
        db,
        root: None,
        strict_schema: false,
        ignore_files: Vec::new(),
        workspace_id: None,
        analyze: false,
        command: ExternalExtractCommand::Analyze,
    }
}

fn info_args(db: std::path::PathBuf) -> ExternalExtractArgs {
    ExternalExtractArgs {
        db,
        root: None,
        strict_schema: false,
        ignore_files: Vec::new(),
        workspace_id: None,
        analyze: false,
        command: ExternalExtractCommand::Info,
    }
}

fn current_revision(db_path: &std::path::Path) -> Option<i64> {
    let db = SymbolDatabase::new(db_path).expect("open db");
    db.get_current_canonical_revision("external_ws")
        .expect("current revision")
}

#[tokio::test]
async fn extract_scan_extracts_parser_backed_symbols_without_workspace_handler() {
    let temp_dir = TempDir::new().expect("temp dir");
    let workspace_root = temp_dir.path().canonicalize().expect("canonical root");
    let file_path = workspace_root.join("lib.rs");
    fs::write(
        &file_path,
        r#"
pub struct ExternalType;

pub fn external_entry() -> ExternalType {
    ExternalType
}
"#,
    )
    .expect("write rust source");

    let mut files_by_language = HashMap::new();
    files_by_language.insert("rust".to_string(), vec![file_path]);

    let batch = extract_files_for_indexing(files_by_language, &workspace_root)
        .await
        .expect("parser-backed external extraction should succeed");

    assert_eq!(batch.files_processed, 1);
    assert_eq!(batch.all_file_infos.len(), 1);
    assert_eq!(batch.all_file_infos[0].path, "lib.rs");
    assert_eq!(batch.files_to_clean, vec!["lib.rs".to_string()]);
    assert!(
        batch.repair_entries.is_empty(),
        "valid parser-backed files should not request repair: {:?}",
        batch.repair_entries
    );
    assert!(
        batch.all_symbols.iter().any(|symbol| {
            symbol.name == "external_entry"
                && symbol.language == "rust"
                && symbol.file_path == "lib.rs"
        }),
        "external extraction should return parser-backed symbols with relative paths: {:?}",
        batch
            .all_symbols
            .iter()
            .map(|symbol| (&symbol.name, &symbol.file_path))
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn extract_scan_routes_cpp_h_header_through_source_aware_detection() {
    let temp_dir = TempDir::new().expect("temp dir");
    let workspace_root = temp_dir.path().canonicalize().expect("canonical root");
    let include_dir = workspace_root.join("include");
    fs::create_dir_all(&include_dir).expect("create include dir");
    let file_path = include_dir.join("widget.h");
    fs::write(
        &file_path,
        r#"
#pragma once
namespace app {
class Widget {
public:
    int value() const { return 42; }
};
}
"#,
    )
    .expect("write cpp header");

    let mut files_by_language = HashMap::new();
    files_by_language.insert("c".to_string(), vec![file_path]);

    let batch = extract_files_for_indexing(files_by_language, &workspace_root)
        .await
        .expect("external extraction should parse source-aware C++ header");

    assert_eq!(batch.files_processed, 1);
    assert_eq!(batch.all_file_infos.len(), 1);
    assert_eq!(batch.all_file_infos[0].path, "include/widget.h");
    assert_eq!(batch.all_file_infos[0].language, "cpp");
    assert!(
        batch.all_symbols.iter().any(|symbol| {
            symbol.name == "Widget"
                && symbol.kind == SymbolKind::Class
                && symbol.language == "cpp"
                && symbol.file_path == "include/widget.h"
        }),
        "external extraction should store C++ symbols for .h header: {:?}",
        batch
            .all_symbols
            .iter()
            .map(|symbol| (&symbol.name, &symbol.kind, &symbol.language))
            .collect::<Vec<_>>()
    );
}

#[test]
fn extract_force_rebuild_is_atomic_after_extraction_success() {
    let tmp = TempDir::new().expect("temp dir");
    let db_path = tmp.path().join("external.db");
    let mut db = SymbolDatabase::new(&db_path).expect("db");

    let old_batch = batch_for(
        vec![make_file("old.rs", "old_hash")],
        vec![make_symbol("old_symbol", "old_entry", "old.rs")],
    );
    persist_force_rebuild(&mut db, "external_ws", &old_batch).expect("seed force rebuild");

    let new_batch = batch_for(
        vec![make_file("new.rs", "new_hash")],
        vec![make_symbol("new_symbol", "new_entry", "new.rs")],
    );
    let revision =
        persist_force_rebuild(&mut db, "external_ws", &new_batch).expect("replace force rebuild");

    assert_eq!(revision, Some(2));
    assert_eq!(count_rows(&db, "files"), 1);
    assert_eq!(count_rows(&db, "symbols"), 1);
    assert!(db.get_file_hash("old.rs").expect("old hash").is_none());
    assert_eq!(
        db.get_file_hash("new.rs").expect("new hash"),
        Some("new_hash".to_string())
    );

    let changes = db
        .get_revision_file_changes_between("external_ws", 1, 2)
        .expect("revision changes");
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].revision, 2);
    assert_eq!(changes[0].file_path, "new.rs");
}

#[test]
fn extract_mixed_scan_records_single_revision() {
    let tmp = TempDir::new().expect("temp dir");
    let db_path = tmp.path().join("external.db");
    let mut db = SymbolDatabase::new(&db_path).expect("db");

    let seed_batch = batch_for(
        vec![
            make_file("changed.rs", "old_changed_hash"),
            make_file("orphan.rs", "old_orphan_hash"),
        ],
        vec![
            make_symbol("changed_old", "changed_old", "changed.rs"),
            make_symbol("orphan_symbol", "orphan", "orphan.rs"),
        ],
    );
    persist_force_rebuild(&mut db, "external_ws", &seed_batch).expect("seed");

    let mixed_batch = batch_for(
        vec![make_file("changed.rs", "new_changed_hash")],
        vec![make_symbol("changed_new", "changed_new", "changed.rs")],
    );
    let revision = persist_incremental_scan(
        &mut db,
        "external_ws",
        &mixed_batch,
        &["orphan.rs".to_string()],
    )
    .expect("mixed scan");

    assert_eq!(revision, Some(2));
    assert_eq!(count_rows(&db, "files"), 1);
    assert!(
        db.get_file_hash("orphan.rs")
            .expect("orphan hash")
            .is_none()
    );

    let changes = db
        .get_revision_file_changes_between("external_ws", 1, 2)
        .expect("revision changes");
    assert_eq!(changes.len(), 2);
    assert!(changes.iter().any(|change| change.file_path == "changed.rs"
        && change.change_kind.as_str() == "modified"
        && change.revision == 2));
    assert!(changes.iter().any(|change| change.file_path == "orphan.rs"
        && change.change_kind.as_str() == "deleted"
        && change.revision == 2));
}

#[test]
fn extract_delete_clears_cross_file_identifier_targets() {
    let tmp = TempDir::new().expect("temp dir");
    let db_path = tmp.path().join("external.db");
    let mut db = SymbolDatabase::new(&db_path).expect("db");

    let mut seed_batch = batch_for(
        vec![
            make_file("caller.rs", "caller_hash"),
            make_file("target.rs", "target_hash"),
        ],
        vec![
            make_symbol("caller_symbol", "caller", "caller.rs"),
            make_symbol("target_symbol", "target", "target.rs"),
        ],
    );
    seed_batch.all_identifiers = vec![make_identifier_with_target(
        "call_ident",
        "caller.rs",
        "caller_symbol",
        "target_symbol",
    )];
    persist_force_rebuild(&mut db, "external_ws", &seed_batch).expect("seed");

    let revision =
        persist_single_file_delete(&mut db, "external_ws", "target.rs").expect("delete target");

    assert_eq!(revision, Some(2));
    assert!(
        db.get_file_hash("target.rs")
            .expect("target hash")
            .is_none()
    );
    let target: Option<String> = db
        .conn
        .query_row(
            "SELECT target_symbol_id FROM identifiers WHERE id = 'call_ident'",
            [],
            |row| row.get(0),
        )
        .expect("identifier row should remain");
    assert_eq!(target, None);
}

#[tokio::test]
async fn extract_scan_writes_caller_owned_sqlite_db() {
    let tmp = TempDir::new().expect("temp dir");
    let root = tmp.path().join("repo");
    std::fs::create_dir(&root).expect("repo dir");
    fs::write(root.join("lib.rs"), "pub fn scanned_entry() {}\n").expect("write source");
    let db_path = tmp.path().join("external.sqlite");

    let report = run_external_scan(&scan_args(db_path.clone(), root.clone(), false))
        .await
        .expect("scan succeeds");

    assert!(db_path.exists(), "scan creates caller-owned sqlite db");
    assert_eq!(report.operation, "scan");
    assert_eq!(report.workspace_id.as_deref(), Some("external_ws"));
    assert_eq!(report.files_scanned, 1);
    assert_eq!(report.files_updated, 1);
    assert_eq!(report.files_deleted, 0);
    assert!(report.symbols_extracted >= 1);

    let db = SymbolDatabase::new(&db_path).expect("open db");
    assert_eq!(count_rows(&db, "files"), 1);
    assert!(
        db.get_all_symbols()
            .expect("symbols")
            .iter()
            .any(|symbol| symbol.name == "scanned_entry" && symbol.file_path == "lib.rs")
    );
    let info = read_external_extract_info(&db_path).expect("read info");
    assert_eq!(
        info.metadata.expect("metadata").analysis_state,
        "stale",
        "scan should mark derived analysis stale after canonical writes"
    );
}

#[tokio::test]
async fn extract_scan_unchanged_produces_zero_revisions() {
    let tmp = TempDir::new().expect("temp dir");
    let root = tmp.path().join("repo");
    std::fs::create_dir(&root).expect("repo dir");
    fs::write(root.join("lib.rs"), "pub fn stable_entry() {}\n").expect("write source");
    let db_path = tmp.path().join("external.sqlite");

    run_external_scan(&scan_args(db_path.clone(), root.clone(), false))
        .await
        .expect("first scan");
    let first_revision = current_revision(&db_path);

    let report = run_external_scan(&scan_args(db_path.clone(), root.clone(), false))
        .await
        .expect("second scan");

    assert_eq!(report.files_scanned, 1);
    assert_eq!(report.files_updated, 0);
    assert_eq!(report.files_deleted, 0);
    assert_eq!(current_revision(&db_path), first_revision);
}

#[tokio::test]
async fn extract_scan_changed_and_orphaned_files_commit_one_revision() {
    let tmp = TempDir::new().expect("temp dir");
    let root = tmp.path().join("repo");
    std::fs::create_dir(&root).expect("repo dir");
    fs::write(root.join("changed.rs"), "pub fn old_entry() {}\n").expect("write changed");
    fs::write(root.join("orphan.rs"), "pub fn orphan_entry() {}\n").expect("write orphan");
    let db_path = tmp.path().join("external.sqlite");

    run_external_scan(&scan_args(db_path.clone(), root.clone(), false))
        .await
        .expect("first scan");
    fs::write(root.join("changed.rs"), "pub fn new_entry() {}\n").expect("modify changed");
    std::fs::remove_file(root.join("orphan.rs")).expect("remove orphan");

    let report = run_external_scan(&scan_args(db_path.clone(), root.clone(), false))
        .await
        .expect("second scan");

    assert_eq!(report.files_scanned, 1);
    assert_eq!(report.files_updated, 1);
    assert_eq!(report.files_deleted, 1);
    assert_eq!(current_revision(&db_path), Some(2));

    let db = SymbolDatabase::new(&db_path).expect("open db");
    let changes = db
        .get_revision_file_changes_between("external_ws", 1, 2)
        .expect("revision changes");
    assert_eq!(changes.len(), 2);
    assert!(changes.iter().any(|change| change.file_path == "changed.rs"
        && change.change_kind.as_str() == "modified"
        && change.revision == 2));
    assert!(changes.iter().any(|change| change.file_path == "orphan.rs"
        && change.change_kind.as_str() == "deleted"
        && change.revision == 2));
}

#[tokio::test]
async fn extract_scan_rejects_different_root_unless_force_rebuild() {
    let tmp = TempDir::new().expect("temp dir");
    let first_root = tmp.path().join("repo_one");
    let second_root = tmp.path().join("repo_two");
    std::fs::create_dir(&first_root).expect("first repo dir");
    std::fs::create_dir(&second_root).expect("second repo dir");
    fs::write(first_root.join("lib.rs"), "pub fn first_root_symbol() {}\n")
        .expect("write first source");
    fs::write(
        second_root.join("lib.rs"),
        "pub fn second_root_symbol() {}\n",
    )
    .expect("write second source");
    let db_path = tmp.path().join("external.sqlite");

    run_external_scan(&scan_args(db_path.clone(), first_root.clone(), false))
        .await
        .expect("initial scan");

    let mismatch = run_external_scan(&scan_args(db_path.clone(), second_root.clone(), false))
        .await
        .expect_err("non-force scan should reject a different root");
    assert!(
        mismatch.to_string().contains("root path mismatch"),
        "unexpected root mismatch error: {mismatch}"
    );

    run_external_scan(&scan_args(db_path.clone(), second_root.clone(), true))
        .await
        .expect("force scan accepts moved root");
    let metadata = read_external_extract_info(&db_path)
        .expect("info")
        .metadata
        .expect("metadata");
    assert_eq!(
        metadata.root_path,
        second_root
            .canonicalize()
            .expect("canonical second root")
            .display()
            .to_string()
    );
}

#[tokio::test]
async fn extract_update_unchanged_file_is_noop() {
    let tmp = TempDir::new().expect("temp dir");
    let root = tmp.path().join("repo");
    std::fs::create_dir(&root).expect("repo dir");
    fs::write(root.join("lib.rs"), "pub fn stable_entry() {}\n").expect("write source");
    let db_path = tmp.path().join("external.sqlite");

    run_external_scan(&scan_args(db_path.clone(), root.clone(), false))
        .await
        .expect("seed scan");
    let first_revision = current_revision(&db_path);

    let report = run_external_update(&update_args(
        db_path.clone(),
        root.clone(),
        "lib.rs".into(),
        Vec::new(),
    ))
    .await
    .expect("unchanged update");

    assert_eq!(report.files_updated, 0);
    assert_eq!(report.files_deleted, 0);
    assert_eq!(current_revision(&db_path), first_revision);
}

#[tokio::test]
async fn extract_update_changed_file_replaces_only_that_file() {
    let tmp = TempDir::new().expect("temp dir");
    let root = tmp.path().join("repo");
    std::fs::create_dir(&root).expect("repo dir");
    fs::write(root.join("a.rs"), "pub fn old_a() {}\n").expect("write a");
    fs::write(root.join("b.rs"), "pub fn stable_b() {}\n").expect("write b");
    let db_path = tmp.path().join("external.sqlite");

    run_external_scan(&scan_args(db_path.clone(), root.clone(), false))
        .await
        .expect("seed scan");
    fs::write(root.join("a.rs"), "pub fn new_a() {}\n").expect("modify a");

    let report = run_external_update(&update_args(
        db_path.clone(),
        root.clone(),
        "a.rs".into(),
        Vec::new(),
    ))
    .await
    .expect("changed update");

    assert_eq!(report.files_updated, 1);
    assert_eq!(report.files_deleted, 0);
    assert_eq!(current_revision(&db_path), Some(2));

    let db = SymbolDatabase::new(&db_path).expect("open db");
    let names: Vec<String> = db
        .get_all_symbols()
        .expect("symbols")
        .into_iter()
        .map(|symbol| symbol.name)
        .collect();
    assert!(names.contains(&"new_a".to_string()));
    assert!(names.contains(&"stable_b".to_string()));
    assert!(!names.contains(&"old_a".to_string()));
}

#[tokio::test]
async fn extract_update_preserves_existing_symbols_when_parser_returns_empty() {
    let tmp = TempDir::new().expect("temp dir");
    let root = tmp.path().join("repo");
    std::fs::create_dir(&root).expect("repo dir");
    fs::write(root.join("lib.rs"), "pub fn existing_symbol() {}\n").expect("write source");
    let db_path = tmp.path().join("external.sqlite");

    run_external_scan(&scan_args(db_path.clone(), root.clone(), false))
        .await
        .expect("seed scan");
    fs::write(
        root.join("lib.rs"),
        "// parser-backed file now has no symbols\n",
    )
    .expect("write empty parser-backed source");

    let error = run_external_update(&update_args(
        db_path.clone(),
        root.clone(),
        "lib.rs".into(),
        Vec::new(),
    ))
    .await
    .expect_err("empty parser-backed extraction should preserve known-good rows");
    assert!(
        error.to_string().contains("would remove existing symbols"),
        "unexpected empty extraction error: {error}"
    );

    let db = SymbolDatabase::new(&db_path).expect("open db");
    let remaining: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM symbols WHERE name = 'existing_symbol' AND file_path = 'lib.rs'",
            [],
            |row| row.get(0),
        )
        .expect("count remaining symbol");
    assert_eq!(remaining, 1);
}

#[tokio::test]
async fn extract_update_ignored_file_deletes_stale_rows() {
    let tmp = TempDir::new().expect("temp dir");
    let root = tmp.path().join("repo");
    std::fs::create_dir(&root).expect("repo dir");
    std::fs::create_dir(root.join("generated")).expect("generated dir");
    fs::write(root.join("generated/out.rs"), "pub fn generated() {}\n").expect("write generated");
    let db_path = tmp.path().join("external.sqlite");

    run_external_scan(&scan_args(db_path.clone(), root.clone(), false))
        .await
        .expect("seed scan");
    let ignore_file = root.join("external.ignore");
    fs::write(&ignore_file, "generated/\n").expect("write ignore");

    let report = run_external_update(&update_args(
        db_path.clone(),
        root.clone(),
        "generated/out.rs".into(),
        vec![ignore_file],
    ))
    .await
    .expect("ignored update");

    assert_eq!(report.files_updated, 0);
    assert_eq!(report.files_deleted, 1);
    assert_eq!(current_revision(&db_path), Some(2));
    let db = SymbolDatabase::new(&db_path).expect("open db");
    assert!(
        db.get_file_hash("generated/out.rs")
            .expect("generated hash")
            .is_none()
    );
}

#[tokio::test]
async fn extract_delete_missing_file_is_idempotent() {
    let tmp = TempDir::new().expect("temp dir");
    let root = tmp.path().join("repo");
    std::fs::create_dir(&root).expect("repo dir");
    fs::write(root.join("lib.rs"), "pub fn stable_entry() {}\n").expect("write source");
    let db_path = tmp.path().join("external.sqlite");

    run_external_scan(&scan_args(db_path.clone(), root.clone(), false))
        .await
        .expect("seed scan");
    let first_revision = current_revision(&db_path);

    for _ in 0..2 {
        let report = run_external_delete(&delete_args(
            db_path.clone(),
            root.clone(),
            "missing.rs".into(),
        ))
        .await
        .expect("delete missing");
        assert_eq!(report.files_deleted, 0);
    }

    assert_eq!(current_revision(&db_path), first_revision);
}

#[tokio::test]
async fn extract_update_marks_analysis_stale() {
    let tmp = TempDir::new().expect("temp dir");
    let root = tmp.path().join("repo");
    std::fs::create_dir(&root).expect("repo dir");
    fs::write(root.join("lib.rs"), "pub fn first_entry() {}\n").expect("write source");
    let db_path = tmp.path().join("external.sqlite");

    run_external_scan(&scan_args(db_path.clone(), root.clone(), false))
        .await
        .expect("seed scan");
    run_external_analyze(&analyze_args(db_path.clone()))
        .await
        .expect("analyze");
    assert_eq!(
        read_external_extract_info(&db_path)
            .expect("info")
            .metadata
            .expect("metadata")
            .analysis_state,
        "current"
    );

    fs::write(root.join("lib.rs"), "pub fn second_entry() {}\n").expect("modify source");
    run_external_update(&update_args(
        db_path.clone(),
        root.clone(),
        "lib.rs".into(),
        Vec::new(),
    ))
    .await
    .expect("update");

    assert_eq!(
        read_external_extract_info(&db_path)
            .expect("info")
            .metadata
            .expect("metadata")
            .analysis_state,
        "stale"
    );
}

#[tokio::test]
async fn extract_update_rolls_back_when_stale_metadata_write_fails() {
    let tmp = TempDir::new().expect("temp dir");
    let root = tmp.path().join("repo");
    std::fs::create_dir(&root).expect("repo dir");
    fs::write(root.join("lib.rs"), "pub fn first_entry() {}\n").expect("write source");
    let db_path = tmp.path().join("external.sqlite");

    run_external_scan(&scan_args(db_path.clone(), root.clone(), false))
        .await
        .expect("seed scan");
    run_external_analyze(&analyze_args(db_path.clone()))
        .await
        .expect("analyze");
    let revision_before = current_revision(&db_path);

    {
        let db = SymbolDatabase::new(&db_path).expect("open db");
        db.conn
            .execute_batch(
                "CREATE TRIGGER fail_external_stale_state
                 BEFORE UPDATE OF value ON external_extract_metadata
                 WHEN OLD.key = 'analysis_state' AND NEW.value = 'stale'
                 BEGIN
                    SELECT RAISE(ABORT, 'forced stale metadata failure');
                 END;",
            )
            .expect("create failure trigger");
    }

    fs::write(root.join("lib.rs"), "pub fn second_entry() {}\n").expect("modify source");
    let error = run_external_update(&update_args(
        db_path.clone(),
        root.clone(),
        "lib.rs".into(),
        Vec::new(),
    ))
    .await
    .expect_err("metadata failure should fail update");

    assert!(
        error.to_string().contains("forced stale metadata failure"),
        "unexpected update error: {error}"
    );
    assert_eq!(current_revision(&db_path), revision_before);
    let db = SymbolDatabase::new(&db_path).expect("open db");
    let first_count: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM symbols WHERE name = 'first_entry'",
            [],
            |row| row.get(0),
        )
        .expect("count first symbol");
    let second_count: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM symbols WHERE name = 'second_entry'",
            [],
            |row| row.get(0),
        )
        .expect("count second symbol");
    assert_eq!(first_count, 1);
    assert_eq!(second_count, 0);
}

#[tokio::test]
async fn extract_analyze_marks_current_revision_analyzed() {
    let tmp = TempDir::new().expect("temp dir");
    let root = tmp.path().join("repo");
    std::fs::create_dir(&root).expect("repo dir");
    fs::write(root.join("lib.rs"), "pub fn analyzed_entry() {}\n").expect("write source");
    let db_path = tmp.path().join("external.sqlite");

    run_external_scan(&scan_args(db_path.clone(), root.clone(), false))
        .await
        .expect("scan");
    let revision = current_revision(&db_path);

    let report = run_external_analyze(&analyze_args(db_path.clone()))
        .await
        .expect("analyze");

    assert_eq!(report.operation, "analyze");
    let metadata = read_external_extract_info(&db_path)
        .expect("info")
        .metadata
        .expect("metadata");
    assert_eq!(metadata.analysis_state, "current");
    assert_eq!(metadata.analyzed_revision, revision);
}

#[test]
fn extract_bulk_insert_nulls_dangling_parent_id() {
    let tmp = TempDir::new().expect("temp dir");
    let db_path = tmp.path().join("external.db");
    let mut db = SymbolDatabase::new(&db_path).expect("db");
    let file = make_file("lib.rs", "hash1");
    let mut child = make_symbol("sym_child", "child", "lib.rs");
    child.parent_id = Some("sym_missing_parent".to_string());
    let batch = batch_for(vec![file], vec![child]);

    persist_force_rebuild(&mut db, "external_ws", &batch).expect("force rebuild");

    let parent_id: Option<String> = db
        .conn
        .query_row(
            "SELECT parent_id FROM symbols WHERE id = 'sym_child'",
            [],
            |row| row.get(0),
        )
        .expect("read parent id");
    assert_eq!(parent_id, None);
}

#[tokio::test]
async fn extract_info_reports_contract_metadata_and_latest_revision() {
    let tmp = TempDir::new().expect("temp dir");
    let root = tmp.path().join("repo");
    std::fs::create_dir(&root).expect("repo dir");
    fs::write(root.join("lib.rs"), "pub fn first_entry() {}\n").expect("write source");
    let db_path = tmp.path().join("external.sqlite");

    run_external_scan(&scan_args(db_path.clone(), root.clone(), false))
        .await
        .expect("scan");
    run_external_analyze(&analyze_args(db_path.clone()))
        .await
        .expect("analyze");
    fs::write(root.join("lib.rs"), "pub fn second_entry() {}\n").expect("modify source");
    run_external_update(&update_args(
        db_path.clone(),
        root.clone(),
        "lib.rs".into(),
        Vec::new(),
    ))
    .await
    .expect("update");

    let report = run_external_info(&info_args(db_path.clone())).expect("info report");

    assert_eq!(
        report.julie_version.as_deref(),
        Some(env!("CARGO_PKG_VERSION"))
    );
    assert_eq!(report.schema_state, Some(ExternalInfoSchemaState::Current));
    assert_eq!(
        report.extract_contract_version,
        Some(EXTRACT_CONTRACT_VERSION)
    );
    assert_eq!(report.revision, Some(2));
    assert_eq!(report.analyzed_revision, None);
    assert_eq!(report.analysis_state.as_deref(), Some("stale"));
    assert!(report.missing_metadata_keys.is_empty());
    assert_eq!(report.files_total, 1);
    assert!(report.symbols_total >= 1);
    assert_eq!(report.types_total, 0);
}

#[tokio::test]
async fn extract_update_analyze_runs_under_one_operation_lock() {
    let tmp = TempDir::new().expect("temp dir");
    let root = tmp.path().join("repo");
    std::fs::create_dir(&root).expect("repo dir");
    fs::write(root.join("lib.rs"), "pub fn first_entry() {}\n").expect("write source");
    let db_path = tmp.path().join("external.sqlite");

    run_external_scan(&scan_args(db_path.clone(), root.clone(), false))
        .await
        .expect("scan");
    fs::write(root.join("lib.rs"), "pub fn analyzed_update() {}\n").expect("modify source");
    let mut args = update_args(db_path.clone(), root.clone(), "lib.rs".into(), Vec::new());
    args.analyze = true;

    run_external_update(&args).await.expect("update analyze");

    let metadata = read_external_extract_info(&db_path)
        .expect("info")
        .metadata
        .expect("metadata");
    assert_eq!(metadata.analysis_state, "current");
    assert_eq!(metadata.analyzed_revision, Some(2));
}
