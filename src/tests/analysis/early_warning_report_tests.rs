use crate::analysis::early_warnings::{EarlyWarningReportOptions, generate_early_warning_report};
use crate::database::{FileInfo, SymbolDatabase};
use crate::extractors::{AnnotationMarker, Symbol, SymbolKind};
use crate::search::language_config::LanguageConfigs;
use std::collections::HashMap;
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
    assert_eq!(report.summary.scheduler_signals, 0);
    assert_eq!(report.summary.entry_point_linkage_gaps, 0);
    assert_eq!(report.summary.high_centrality_linkage_gaps, 0);
    assert!(report.entry_points.is_empty());
    assert!(report.auth_coverage_candidates.is_empty());
    assert!(report.review_markers.is_empty());
    assert!(report.scheduler_signals.is_empty());
    assert!(report.entry_point_linkage_gaps.is_empty());
    assert!(report.high_centrality_linkage_gaps.is_empty());
    assert!(json.contains("\"entry_points\":0"));
    assert!(json.contains("\"auth_coverage_candidates\":0"));
    assert!(json.contains("\"review_markers\":0"));
    assert!(json.contains("\"scheduler_signals\":0"));
    assert!(json.contains("\"entry_point_linkage_gaps\":0"));
    assert!(json.contains("\"high_centrality_linkage_gaps\":0"));
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

// ---------------------------------------------------------------------------
// Task 7: Scheduler signal tests
// ---------------------------------------------------------------------------

fn symbol_with_metadata(
    id: &str,
    name: &str,
    kind: SymbolKind,
    language: &str,
    file_path: &str,
    start_line: u32,
    parent_id: Option<&str>,
    annotations: Vec<AnnotationMarker>,
    metadata: Option<HashMap<String, serde_json::Value>>,
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
        metadata,
        semantic_group: None,
        confidence: Some(1.0),
        code_context: Some(format!("{name}() {{}}")),
        content_type: None,
        annotations,
    }
}

#[test]
fn early_warning_report_scheduler_signal_detection() {
    let (_temp_dir, mut db) = open_db();
    let configs = LanguageConfigs::load_embedded();
    let file = file_info("tasks/jobs.py", "python");
    db.store_file_info(&file).unwrap();

    let scheduled_task = symbol(
        "task-1",
        "send_daily_email",
        SymbolKind::Function,
        "python",
        &file.path,
        10,
        None,
        vec![marker("celery.task", "celery.task", Some("@celery.task"))],
    );
    let plain_func = symbol(
        "plain-1",
        "helper",
        SymbolKind::Function,
        "python",
        &file.path,
        20,
        None,
        Vec::new(),
    );
    db.store_symbols(&[scheduled_task, plain_func]).unwrap();

    let report = generate_early_warning_report(&db, &configs, options()).unwrap();

    assert_eq!(report.summary.scheduler_signals, 1);
    assert_eq!(report.scheduler_signals.len(), 1);
    assert_eq!(report.scheduler_signals[0].symbol_name, "send_daily_email");
    assert_eq!(report.scheduler_signals[0].annotation_key, "celery.task");
    assert_eq!(report.scheduler_signals[0].language, "python");
}

// ---------------------------------------------------------------------------
// Task 8: Linkage gap tests
// ---------------------------------------------------------------------------

#[test]
fn early_warning_report_entry_point_linkage_gap_no_test_linkage() {
    let (_temp_dir, mut db) = open_db();
    let configs = LanguageConfigs::load_embedded();
    let file = file_info("Controllers/UsersController.cs", "csharp");
    db.store_file_info(&file).unwrap();

    // Entry point without test_linkage metadata => gap
    let route = symbol(
        "route-no-linkage",
        "GetUsers",
        SymbolKind::Method,
        "csharp",
        &file.path,
        5,
        None,
        vec![marker("HttpGet", "httpget", Some("[HttpGet]"))],
    );
    db.store_symbols(&[route]).unwrap();

    let report = generate_early_warning_report(&db, &configs, options()).unwrap();

    assert_eq!(report.summary.entry_points, 1);
    assert_eq!(report.summary.entry_point_linkage_gaps, 1);
    assert_eq!(report.entry_point_linkage_gaps.len(), 1);
    assert_eq!(report.entry_point_linkage_gaps[0].symbol_name, "GetUsers");
    assert_eq!(
        report.entry_point_linkage_gaps[0].entry_annotation,
        "HttpGet"
    );
}

#[test]
fn early_warning_report_entry_point_with_test_linkage_is_not_a_gap() {
    let (_temp_dir, mut db) = open_db();
    let configs = LanguageConfigs::load_embedded();
    let file = file_info("Controllers/OrdersController.cs", "csharp");
    db.store_file_info(&file).unwrap();

    // Entry point WITH test_linkage metadata => not a gap
    let mut metadata = HashMap::new();
    metadata.insert(
        "test_linkage".to_string(),
        serde_json::json!({"tests": ["test_get_orders"]}),
    );
    let route = symbol_with_metadata(
        "route-with-linkage",
        "GetOrders",
        SymbolKind::Method,
        "csharp",
        &file.path,
        5,
        None,
        vec![marker("HttpGet", "httpget", Some("[HttpGet]"))],
        Some(metadata),
    );
    db.store_symbols(&[route]).unwrap();

    let report = generate_early_warning_report(&db, &configs, options()).unwrap();

    assert_eq!(report.summary.entry_points, 1);
    assert_eq!(report.summary.entry_point_linkage_gaps, 0);
    assert!(report.entry_point_linkage_gaps.is_empty());
}

#[test]
fn early_warning_report_entry_point_with_test_coverage_is_not_a_gap() {
    let (_temp_dir, mut db) = open_db();
    let configs = LanguageConfigs::load_embedded();
    let file = file_info("Controllers/LegacyCoverageController.cs", "csharp");
    db.store_file_info(&file).unwrap();

    let mut metadata = HashMap::new();
    metadata.insert(
        "test_coverage".to_string(),
        serde_json::json!({"tests": ["test_get_legacy_orders"]}),
    );
    let route = symbol_with_metadata(
        "route-with-legacy-coverage",
        "GetLegacyOrders",
        SymbolKind::Method,
        "csharp",
        &file.path,
        5,
        None,
        vec![marker("HttpGet", "httpget", Some("[HttpGet]"))],
        Some(metadata),
    );
    db.store_symbols(&[route]).unwrap();

    let report = generate_early_warning_report(&db, &configs, options()).unwrap();

    assert_eq!(report.summary.entry_points, 1);
    assert_eq!(report.summary.entry_point_linkage_gaps, 0);
    assert!(report.entry_point_linkage_gaps.is_empty());
}

#[test]
fn early_warning_report_high_centrality_linkage_gap_query() {
    let (_temp_dir, mut db) = open_db();
    let configs = LanguageConfigs::load_embedded();
    let file = file_info("src/core/engine.rs", "rust");
    db.store_file_info(&file).unwrap();

    // Symbol with high centrality, no test_linkage => gap
    let high_centrality = symbol(
        "engine-run",
        "run",
        SymbolKind::Function,
        "rust",
        &file.path,
        10,
        None,
        Vec::new(),
    );
    // Symbol with test_linkage => not a gap
    let mut metadata = HashMap::new();
    metadata.insert(
        "test_linkage".to_string(),
        serde_json::json!({"tests": ["test_process"]}),
    );
    let linked = symbol_with_metadata(
        "engine-process",
        "process",
        SymbolKind::Function,
        "rust",
        &file.path,
        30,
        None,
        Vec::new(),
        Some(metadata),
    );
    // Test symbol with high centrality => excluded (is_test)
    let mut test_meta = HashMap::new();
    test_meta.insert("is_test".to_string(), serde_json::json!(1));
    let test_sym = symbol_with_metadata(
        "test-engine",
        "test_run",
        SymbolKind::Function,
        "rust",
        &file.path,
        50,
        None,
        Vec::new(),
        Some(test_meta),
    );

    db.store_symbols(&[high_centrality, linked, test_sym])
        .unwrap();

    // Set reference_score on engine-run and test-engine to make them "high centrality"
    db.conn
        .execute(
            "UPDATE symbols SET reference_score = 5.0 WHERE id = ?1",
            rusqlite::params!["engine-run"],
        )
        .unwrap();
    db.conn
        .execute(
            "UPDATE symbols SET reference_score = 3.0 WHERE id = ?1",
            rusqlite::params!["engine-process"],
        )
        .unwrap();
    db.conn
        .execute(
            "UPDATE symbols SET reference_score = 4.0 WHERE id = ?1",
            rusqlite::params!["test-engine"],
        )
        .unwrap();

    let report = generate_early_warning_report(&db, &configs, options()).unwrap();

    // Only engine-run should appear: high centrality, no test_linkage, not a test
    assert_eq!(report.summary.high_centrality_linkage_gaps, 1);
    assert_eq!(report.high_centrality_linkage_gaps.len(), 1);
    assert_eq!(report.high_centrality_linkage_gaps[0].symbol_name, "run");
    assert_eq!(report.high_centrality_linkage_gaps[0].reference_score, 5.0);
}

#[test]
fn early_warning_report_high_centrality_with_test_coverage_is_not_a_gap() {
    let (_temp_dir, mut db) = open_db();
    let configs = LanguageConfigs::load_embedded();
    let file = file_info("src/core/legacy.rs", "rust");
    db.store_file_info(&file).unwrap();

    let mut metadata = HashMap::new();
    metadata.insert(
        "test_coverage".to_string(),
        serde_json::json!({"tests": ["test_legacy_run"]}),
    );
    let covered = symbol_with_metadata(
        "legacy-covered",
        "legacy_run",
        SymbolKind::Function,
        "rust",
        &file.path,
        10,
        None,
        Vec::new(),
        Some(metadata),
    );
    db.store_symbols(&[covered]).unwrap();
    db.conn
        .execute(
            "UPDATE symbols SET reference_score = 10.0 WHERE id = ?1",
            rusqlite::params!["legacy-covered"],
        )
        .unwrap();

    let report = generate_early_warning_report(&db, &configs, options()).unwrap();

    assert_eq!(report.summary.high_centrality_linkage_gaps, 0);
    assert!(report.high_centrality_linkage_gaps.is_empty());
}

#[test]
fn early_warning_report_high_centrality_file_pattern_searches_past_unscoped_limit() {
    let (_temp_dir, mut db) = open_db();
    let configs = LanguageConfigs::load_embedded();
    let api_file = file_info("src/api/routes.rs", "rust");
    let core_file = file_info("src/core/engine.rs", "rust");
    db.store_file_info(&api_file).unwrap();
    db.store_file_info(&core_file).unwrap();

    let mut symbols = Vec::new();
    for idx in 0..81 {
        symbols.push(symbol(
            &format!("core-{idx}"),
            &format!("core_helper_{idx}"),
            SymbolKind::Function,
            "rust",
            &core_file.path,
            idx + 1,
            None,
            Vec::new(),
        ));
    }
    symbols.push(symbol(
        "api-route",
        "serve_route",
        SymbolKind::Function,
        "rust",
        &api_file.path,
        200,
        None,
        Vec::new(),
    ));
    db.store_symbols(&symbols).unwrap();

    for idx in 0..81 {
        db.conn
            .execute(
                "UPDATE symbols SET reference_score = ?1 WHERE id = ?2",
                rusqlite::params![100.0 - f64::from(idx), format!("core-{idx}")],
            )
            .unwrap();
    }
    db.conn
        .execute(
            "UPDATE symbols SET reference_score = 1.0 WHERE id = ?1",
            rusqlite::params!["api-route"],
        )
        .unwrap();

    let report = generate_early_warning_report(
        &db,
        &configs,
        EarlyWarningReportOptions {
            file_pattern: Some("src/api/**".to_string()),
            ..options()
        },
    )
    .unwrap();

    assert_eq!(report.summary.high_centrality_linkage_gaps, 1);
    assert_eq!(report.high_centrality_linkage_gaps.len(), 1);
    assert_eq!(
        report.high_centrality_linkage_gaps[0].symbol_id,
        "api-route"
    );
}

#[test]
fn early_warning_report_high_centrality_summary_counts_all_matching_gaps() {
    let (_temp_dir, mut db) = open_db();
    let configs = LanguageConfigs::load_embedded();
    let file = file_info("src/core/many.rs", "rust");
    db.store_file_info(&file).unwrap();

    let mut symbols = Vec::new();
    for idx in 0..25 {
        symbols.push(symbol(
            &format!("gap-{idx}"),
            &format!("gap_symbol_{idx}"),
            SymbolKind::Function,
            "rust",
            &file.path,
            idx + 1,
            None,
            Vec::new(),
        ));
    }
    db.store_symbols(&symbols).unwrap();

    for idx in 0..25 {
        db.conn
            .execute(
                "UPDATE symbols SET reference_score = ?1 WHERE id = ?2",
                rusqlite::params![100.0 - f64::from(idx), format!("gap-{idx}")],
            )
            .unwrap();
    }

    let report = generate_early_warning_report(
        &db,
        &configs,
        EarlyWarningReportOptions {
            limit_per_section: Some(5),
            ..options()
        },
    )
    .unwrap();

    assert_eq!(report.summary.high_centrality_linkage_gaps, 25);
    assert_eq!(report.high_centrality_linkage_gaps.len(), 5);
    assert_eq!(report.high_centrality_linkage_gaps[0].symbol_id, "gap-0");
}
