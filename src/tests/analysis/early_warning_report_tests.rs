use crate::analysis::early_warnings::{EarlyWarningReportOptions, generate_early_warning_report};
use crate::database::{FileInfo, SymbolDatabase};
use crate::extractors::{AnnotationMarker, Symbol, SymbolKind};
use crate::search::language_config::LanguageConfigs;
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
        line_count: 16,
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

fn symbol(
    id: &str,
    name: &str,
    kind: SymbolKind,
    language: &str,
    file_path: &str,
    start_line: u32,
    parent_id: Option<&str>,
    annotations: Vec<AnnotationMarker>,
) -> Symbol {
    Symbol {
        id: id.to_string(),
        name: name.to_string(),
        kind,
        language: language.to_string(),
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
        parent_id: parent_id.map(str::to_string),
        metadata: None,
        semantic_group: None,
        confidence: Some(1.0),
        code_context: Some(format!("{name}() {{}}")),
        content_type: None,
        annotations,
    }
}

fn open_db() -> (TempDir, SymbolDatabase) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("early-warnings.db");
    let db = SymbolDatabase::new(&db_path).unwrap();
    (temp_dir, db)
}

fn options() -> EarlyWarningReportOptions {
    EarlyWarningReportOptions {
        workspace_id: "ws-report".to_string(),
        file_pattern: None,
        fresh: true,
        limit_per_section: None,
    }
}

#[test]
fn early_warning_report_entry_points_and_parent_auth() {
    let (_temp_dir, mut db) = open_db();
    let configs = LanguageConfigs::load_embedded();
    let file = file_info("Controllers/OrdersController.cs", "csharp");
    db.store_file_info(&file).unwrap();

    let controller = symbol(
        "controller",
        "OrdersController",
        SymbolKind::Class,
        "csharp",
        &file.path,
        3,
        None,
        vec![marker("Authorize", "authorize", Some("[Authorize]"))],
    );
    let covered_route = symbol(
        "covered-route",
        "ListOrders",
        SymbolKind::Method,
        "csharp",
        &file.path,
        8,
        Some("controller"),
        vec![marker("HttpGet", "httpget", Some("[HttpGet]"))],
    );
    let bypassed_route = symbol(
        "bypassed-route",
        "Health",
        SymbolKind::Method,
        "csharp",
        &file.path,
        14,
        None,
        vec![
            marker("HttpGet", "httpget", Some("[HttpGet(\"/health\")]")),
            marker("AllowAnonymous", "allowanonymous", Some("[AllowAnonymous]")),
        ],
    );
    db.store_symbols(&[controller, covered_route, bypassed_route])
        .unwrap();

    let report = generate_early_warning_report(&db, &configs, options()).unwrap();

    assert!(!report.from_cache);
    assert_eq!(report.summary.entry_points, 2);
    assert_eq!(report.summary.auth_coverage_candidates, 0);
    assert_eq!(report.summary.review_markers, 1);
    assert!(report.auth_coverage_candidates.is_empty());
    assert_eq!(report.entry_points[0].symbol_name, "ListOrders");
    assert_eq!(report.entry_points[0].symbol_kind, "method");
    assert_eq!(report.entry_points[0].language, "csharp");
    assert_eq!(report.entry_points[0].file_path, file.path);
    assert_eq!(report.entry_points[0].start_line, 8);
    assert_eq!(report.entry_points[0].annotation, "HttpGet");
    assert_eq!(report.entry_points[0].annotation_key, "httpget");
    assert_eq!(
        report.entry_points[0].raw_text.as_deref(),
        Some("[HttpGet]")
    );
    assert_eq!(report.review_markers[0].annotation_key, "allowanonymous");

    let json = serde_json::to_string(&report).unwrap();
    assert!(!json.contains("risk_score"));
    let roundtrip: crate::analysis::early_warnings::EarlyWarningReport =
        serde_json::from_str(&json).unwrap();
    assert_eq!(roundtrip, report);
}

#[test]
fn early_warning_report_counts_auth_candidates_once_per_symbol() {
    let (_temp_dir, mut db) = open_db();
    let configs = LanguageConfigs::load_embedded();
    let file = file_info("Controllers/MultiRouteController.cs", "csharp");
    db.store_file_info(&file).unwrap();

    db.store_symbols(&[symbol(
        "multi-route",
        "MultiRoute",
        SymbolKind::Method,
        "csharp",
        &file.path,
        5,
        None,
        vec![
            marker("HttpGet", "httpget", Some("[HttpGet(\"/a\")]")),
            marker("HttpPost", "httppost", Some("[HttpPost(\"/b\")]")),
        ],
    )])
    .unwrap();

    let report = generate_early_warning_report(&db, &configs, options()).unwrap();

    assert_eq!(report.summary.entry_points, 2);
    assert_eq!(report.entry_points.len(), 2);
    assert_eq!(report.summary.auth_coverage_candidates, 1);
    assert_eq!(report.auth_coverage_candidates.len(), 1);
    assert_eq!(report.auth_coverage_candidates[0].symbol_id, "multi-route");
}

#[test]
fn early_warning_report_empty_state_serializes_zero_counts() {
    let (_temp_dir, mut db) = open_db();
    let configs = LanguageConfigs::load_embedded();
    let file = file_info("src/plain.rs", "rust");
    db.store_file_info(&file).unwrap();
    db.store_symbols(&[symbol(
        "plain",
        "plain_handler",
        SymbolKind::Function,
        "rust",
        &file.path,
        5,
        None,
        Vec::new(),
    )])
    .unwrap();

    let report = generate_early_warning_report(&db, &configs, options()).unwrap();
    let json = serde_json::to_string(&report).unwrap();

    assert_eq!(report.summary.entry_points, 0);
    assert_eq!(report.summary.auth_coverage_candidates, 0);
    assert_eq!(report.summary.review_markers, 0);
    assert!(report.entry_points.is_empty());
    assert!(report.auth_coverage_candidates.is_empty());
    assert!(report.review_markers.is_empty());
    assert!(json.contains("\"entry_points\":0"));
    assert!(json.contains("\"auth_coverage_candidates\":0"));
    assert!(json.contains("\"review_markers\":0"));
}

#[test]
fn early_warning_report_limit_caps_rows_not_summary_counts() {
    let (_temp_dir, mut db) = open_db();
    let configs = LanguageConfigs::load_embedded();
    let file = file_info("Controllers/HealthController.cs", "csharp");
    db.store_file_info(&file).unwrap();

    db.store_symbols(&[
        symbol(
            "route-a",
            "Health",
            SymbolKind::Method,
            "csharp",
            &file.path,
            5,
            None,
            vec![marker("HttpGet", "httpget", Some("[HttpGet]"))],
        ),
        symbol(
            "route-b",
            "Status",
            SymbolKind::Method,
            "csharp",
            &file.path,
            12,
            None,
            vec![marker("HttpGet", "httpget", Some("[HttpGet(\"/status\")]"))],
        ),
    ])
    .unwrap();

    let report = generate_early_warning_report(
        &db,
        &configs,
        EarlyWarningReportOptions {
            limit_per_section: Some(1),
            ..options()
        },
    )
    .unwrap();

    assert_eq!(report.summary.entry_points, 2);
    assert_eq!(report.summary.auth_coverage_candidates, 2);
    assert_eq!(report.entry_points.len(), 1);
    assert_eq!(report.auth_coverage_candidates.len(), 1);
}

#[test]
fn early_warning_report_file_pattern_scopes_analysis() {
    let (_temp_dir, mut db) = open_db();
    let configs = LanguageConfigs::load_embedded();

    let api_file = file_info("src/api/users.py", "python");
    let util_file = file_info("src/utils/helpers.py", "python");
    db.store_file_info(&api_file).unwrap();
    db.store_file_info(&util_file).unwrap();

    db.store_symbols(&[
        symbol(
            "api-route",
            "get_users",
            SymbolKind::Function,
            "python",
            &api_file.path,
            10,
            None,
            vec![marker(
                "app.route",
                "app.route",
                Some("@app.route(\"/users\")"),
            )],
        ),
        symbol(
            "util-route",
            "health_check",
            SymbolKind::Function,
            "python",
            &util_file.path,
            5,
            None,
            vec![marker(
                "app.route",
                "app.route",
                Some("@app.route(\"/health\")"),
            )],
        ),
    ])
    .unwrap();

    let scoped = generate_early_warning_report(
        &db,
        &configs,
        EarlyWarningReportOptions {
            file_pattern: Some("src/api/**".to_string()),
            ..options()
        },
    )
    .unwrap();

    assert_eq!(scoped.summary.entry_points, 1, "only api file should match");
    assert_eq!(scoped.entry_points[0].symbol_name, "get_users");

    let unscoped = generate_early_warning_report(
        &db,
        &configs,
        EarlyWarningReportOptions {
            file_pattern: None,
            ..options()
        },
    )
    .unwrap();

    assert_eq!(
        unscoped.summary.entry_points, 2,
        "no filter should include both"
    );
}
