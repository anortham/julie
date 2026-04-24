use std::collections::BTreeSet;

use tempfile::TempDir;

use crate::database::{FileInfo, SymbolDatabase};
use crate::extractors::{AnnotationMarker, Symbol, SymbolKind};
use crate::search::{SearchFilter, SearchIndex, SearchProjection, SymbolDocument};
use crate::tools::search::text_search::definition_search_with_index_for_test;

fn marker(annotation: &str, annotation_key: &str, raw_text: &str) -> AnnotationMarker {
    AnnotationMarker {
        annotation: annotation.to_string(),
        annotation_key: annotation_key.to_string(),
        raw_text: Some(raw_text.to_string()),
        carrier: None,
    }
}

fn symbol(
    id: &str,
    name: &str,
    kind: SymbolKind,
    file_path: &str,
    parent_id: Option<&str>,
    annotations: Vec<AnnotationMarker>,
) -> Symbol {
    Symbol {
        id: id.to_string(),
        name: name.to_string(),
        kind,
        language: "rust".to_string(),
        file_path: file_path.to_string(),
        start_line: 10,
        start_column: 0,
        end_line: 12,
        end_column: 1,
        start_byte: 100,
        end_byte: 160,
        signature: Some(format!("fn {name}()")),
        doc_comment: None,
        visibility: None,
        parent_id: parent_id.map(str::to_string),
        metadata: None,
        annotations,
        semantic_group: None,
        confidence: Some(1.0),
        code_context: Some(format!("fn {name}() {{}}")),
        content_type: None,
    }
}

fn file_info(path: &str) -> FileInfo {
    FileInfo {
        path: path.to_string(),
        language: "rust".to_string(),
        hash: format!("hash-{path}"),
        size: 64,
        last_modified: 1_700_000_000,
        last_indexed: 0,
        symbol_count: 1,
        line_count: 20,
        content: Some(format!("// {path}")),
    }
}

fn projected_index(symbols: &[Symbol]) -> (TempDir, TempDir, SearchIndex) {
    let (db_dir, index_dir, _db, index) = projected_index_with_db(symbols);
    (db_dir, index_dir, index)
}

fn projected_index_with_db(symbols: &[Symbol]) -> (TempDir, TempDir, SymbolDatabase, SearchIndex) {
    let db_dir = TempDir::new().unwrap();
    let mut db = SymbolDatabase::new(&db_dir.path().join("symbols.db")).unwrap();

    let file_paths: BTreeSet<_> = symbols
        .iter()
        .map(|symbol| symbol.file_path.as_str())
        .collect();
    for path in file_paths {
        db.store_file_info(&file_info(path)).unwrap();
    }
    db.store_symbols(symbols).unwrap();

    let index_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(index_dir.path()).unwrap();
    let symbol_docs: Vec<_> = symbols.iter().map(SymbolDocument::from_symbol).collect();
    SearchProjection::tantivy("annotation-search-tests")
        .project_documents(&mut db, &index, &symbol_docs, &[], &[], Some(1))
        .unwrap();

    (db_dir, index_dir, db, index)
}

fn result_names(index: &SearchIndex, query: &str) -> Vec<String> {
    index
        .search_symbols(query, &SearchFilter::default(), 10)
        .unwrap()
        .results
        .into_iter()
        .map(|result| result.name)
        .collect()
}

#[test]
fn annotation_search_exact_key_does_not_match_symbol_names() {
    let symbols = vec![
        symbol(
            "annotated",
            "plain_handler",
            SymbolKind::Function,
            "src/routes.rs",
            None,
            vec![marker("Test", "test", "Test")],
        ),
        symbol(
            "name-only",
            "TestUtility",
            SymbolKind::Function,
            "src/helpers.rs",
            None,
            Vec::new(),
        ),
    ];
    let (_db_dir, _index_dir, index) = projected_index(&symbols);

    assert_eq!(result_names(&index, "@Test"), vec!["plain_handler"]);
    assert_eq!(result_names(&index, "Test"), vec!["TestUtility"]);
}

#[test]
fn annotation_search_matches_owner_name_context() {
    let symbols = vec![
        symbol(
            "user-controller",
            "UserController",
            SymbolKind::Class,
            "src/controllers.rs",
            None,
            Vec::new(),
        ),
        symbol(
            "user-route",
            "list_users",
            SymbolKind::Method,
            "src/controllers.rs",
            Some("user-controller"),
            vec![marker("GetMapping", "getmapping", "GetMapping(\"/users\")")],
        ),
        symbol(
            "admin-controller",
            "AdminController",
            SymbolKind::Class,
            "src/admin.rs",
            None,
            Vec::new(),
        ),
        symbol(
            "admin-route",
            "list_admins",
            SymbolKind::Method,
            "src/admin.rs",
            Some("admin-controller"),
            vec![marker(
                "GetMapping",
                "getmapping",
                "GetMapping(\"/admins\")",
            )],
        ),
    ];
    let (_db_dir, _index_dir, index) = projected_index(&symbols);

    assert_eq!(
        result_names(&index, "@GetMapping UserController"),
        vec!["list_users"]
    );
}

#[test]
fn annotation_search_normalizes_native_pasted_syntax() {
    let symbols = vec![
        symbol(
            "authorize",
            "secure_endpoint",
            SymbolKind::Function,
            "src/auth.rs",
            None,
            vec![marker("Authorize", "authorize", "Authorize")],
        ),
        symbol(
            "tokio-test",
            "async_case",
            SymbolKind::Function,
            "src/async_tests.rs",
            None,
            vec![marker("tokio::test", "tokio::test", "tokio::test")],
        ),
        symbol(
            "python-route",
            "python_route",
            SymbolKind::Function,
            "src/app.py",
            None,
            vec![marker("app.route", "app.route", "app.route(\"/x\")")],
        ),
    ];
    let (_db_dir, _index_dir, index) = projected_index(&symbols);

    assert_eq!(result_names(&index, "[Authorize]"), vec!["secure_endpoint"]);
    assert_eq!(result_names(&index, "#[tokio::test]"), vec!["async_case"]);
    assert_eq!(
        result_names(&index, "@app.route(\"/x\")"),
        vec!["python_route"]
    );
}

#[test]
fn annotation_search_or_fallback_keeps_annotation_filter_required() {
    let symbols = vec![
        symbol(
            "user-controller",
            "UserController",
            SymbolKind::Class,
            "src/controllers.rs",
            None,
            Vec::new(),
        ),
        symbol(
            "user-route",
            "list_users",
            SymbolKind::Method,
            "src/controllers.rs",
            Some("user-controller"),
            vec![marker("GetMapping", "getmapping", "GetMapping(\"/users\")")],
        ),
        symbol(
            "unannotated",
            "missing_usercontroller_match",
            SymbolKind::Function,
            "src/noise.rs",
            None,
            Vec::new(),
        ),
    ];
    let (_db_dir, _index_dir, index) = projected_index(&symbols);

    let results = index
        .search_symbols(
            "@GetMapping UserController missing",
            &SearchFilter::default(),
            10,
        )
        .unwrap();

    assert!(results.relaxed);
    assert_eq!(
        results
            .results
            .into_iter()
            .map(|result| result.name)
            .collect::<Vec<_>>(),
        vec!["list_users"]
    );
}

#[test]
fn annotation_text_search_hydrates_hits_without_sqlite_prepend_pollution() {
    let symbols = vec![
        symbol(
            "annotated",
            "plain_handler",
            SymbolKind::Function,
            "src/routes.rs",
            None,
            vec![marker("Test", "test", "Test")],
        ),
        symbol(
            "sqlite-rescue",
            "Noise.@Test",
            SymbolKind::Function,
            "src/noise.rs",
            None,
            Vec::new(),
        ),
    ];
    let (_db_dir, _index_dir, db, index) = projected_index_with_db(&symbols);

    let (results, _relaxed, pre_trunc) = definition_search_with_index_for_test(
        "@Test",
        &SearchFilter::default(),
        10,
        &index,
        Some(&db),
    )
    .unwrap();

    assert_eq!(pre_trunc, 1);
    assert_eq!(
        results
            .iter()
            .map(|symbol| symbol.name.as_str())
            .collect::<Vec<_>>(),
        vec!["plain_handler"]
    );
    assert_eq!(
        results[0].code_context.as_deref(),
        Some("fn plain_handler() {}")
    );
}
