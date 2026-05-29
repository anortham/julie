use std::path::PathBuf;

use clap::CommandFactory;
use serde_json::json;

use crate::cli_tools::OutputFormat;
use crate::external_extract::{
    ExternalExtractArgs, ExternalExtractCommand, ExternalExtractError, ExternalExtractReport,
    ExternalExtractStatus, ExternalInfoSchemaState, format_external_extract_report,
};

#[test]
fn external_extract_args_parse_scan_update_delete_analyze_info() {
    ExternalExtractArgs::command().debug_assert();

    let scan = ExternalExtractArgs::try_parse_from([
        "extract",
        "scan",
        "--db",
        "external.sqlite",
        "--root",
        "/repo",
        "--strict-schema",
        "--ignore-file",
        ".gitignore",
        "--workspace-id",
        "ws_1",
        "--analyze",
    ])
    .expect("scan parses");

    assert_eq!(scan.db, PathBuf::from("external.sqlite"));
    assert_eq!(scan.root, Some(PathBuf::from("/repo")));
    assert!(scan.strict_schema);
    assert_eq!(scan.ignore_files, vec![PathBuf::from(".gitignore")]);
    assert_eq!(scan.workspace_id.as_deref(), Some("ws_1"));
    assert!(scan.analyze);
    assert!(matches!(
        scan.command,
        ExternalExtractCommand::Scan { force: false }
    ));

    for command in ["update", "delete"] {
        let args = ExternalExtractArgs::try_parse_from([
            "extract",
            command,
            "--db",
            "external.sqlite",
            "--root",
            "/repo",
            "--file",
            "src/lib.rs",
        ])
        .unwrap_or_else(|error| panic!("{command} should parse: {error}"));

        assert_eq!(args.db, PathBuf::from("external.sqlite"));
        assert_eq!(args.root, Some(PathBuf::from("/repo")));
        assert!(!args.strict_schema);
        assert!(args.ignore_files.is_empty());
        assert_eq!(args.workspace_id, None);
        assert!(!args.analyze);
        match args.command {
            ExternalExtractCommand::Update { file } | ExternalExtractCommand::Delete { file } => {
                assert_eq!(file, PathBuf::from("src/lib.rs"));
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    let analyze =
        ExternalExtractArgs::try_parse_from(["extract", "analyze", "--db", "external.sqlite"])
            .expect("analyze parses");

    assert_eq!(analyze.command.as_str(), "analyze");
    assert_eq!(analyze.root, None);
    assert!(!analyze.analyze);
}

#[test]
fn external_extract_args_update_delete_require_file() {
    for command in ["update", "delete"] {
        let missing_file = ExternalExtractArgs::try_parse_from([
            "extract",
            command,
            "--db",
            "external.sqlite",
            "--root",
            "/repo",
        ])
        .unwrap_err();

        assert!(missing_file.to_string().contains("--file"));

        let args = ExternalExtractArgs::try_parse_from([
            "extract",
            command,
            "--db",
            "external.sqlite",
            "--root",
            "/repo",
            "--file",
            "src/lib.rs",
        ])
        .unwrap_or_else(|error| panic!("{command} with --file should parse: {error}"));

        match args.command {
            ExternalExtractCommand::Update { file } | ExternalExtractCommand::Delete { file } => {
                assert_eq!(file, PathBuf::from("src/lib.rs"));
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }
}

#[test]
fn external_extract_args_scan_accepts_force() {
    let scan = ExternalExtractArgs::try_parse_from([
        "extract",
        "scan",
        "--db",
        "external.sqlite",
        "--root",
        "/repo",
        "--force",
    ])
    .expect("scan --force parses");

    assert!(matches!(
        scan.command,
        ExternalExtractCommand::Scan { force: true }
    ));

    let update_with_force = ExternalExtractArgs::try_parse_from([
        "extract",
        "update",
        "--db",
        "external.sqlite",
        "--root",
        "/repo",
        "--file",
        "src/lib.rs",
        "--force",
    ])
    .expect_err("update should not accept --force");

    assert!(update_with_force.to_string().contains("--force"));

    let info_with_file = ExternalExtractArgs::try_parse_from([
        "extract",
        "info",
        "--db",
        "external.sqlite",
        "--file",
        "src/lib.rs",
    ])
    .expect_err("info should stay file-free");

    assert!(info_with_file.to_string().contains("--file"));
}

#[test]
fn external_extract_args_info_does_not_require_root() {
    let info = ExternalExtractArgs::try_parse_from(["extract", "info", "--db", "external.sqlite"])
        .expect("info should only require --db");

    assert_eq!(info.db, PathBuf::from("external.sqlite"));
    assert_eq!(info.root, None);

    let missing_root =
        ExternalExtractArgs::try_parse_from(["extract", "scan", "--db", "external.sqlite"])
            .expect_err("scan should require --root");

    assert!(missing_root.to_string().contains("--root"));
}

#[test]
fn external_extract_report_json_is_deterministic() {
    let report = ExternalExtractReport {
        status: ExternalExtractStatus::Failed,
        operation: "scan".to_string(),
        workspace_id: Some("ws_1".to_string()),
        db: PathBuf::from("external.sqlite"),
        root: Some(PathBuf::from("/repo")),
        julie_version: Some("7.9.3".to_string()),
        schema_version: Some(26),
        schema_state: Some(ExternalInfoSchemaState::Current),
        extract_contract_version: Some(1),
        revision: Some(42),
        analyzed_revision: Some(41),
        analysis_state: Some("stale".to_string()),
        missing_metadata_keys: Vec::new(),
        files_scanned: 3,
        files_updated: 2,
        files_deleted: 1,
        symbols_extracted: 5,
        files_total: 7,
        symbols_total: 11,
        relationships_total: 13,
        identifiers_total: 17,
        types_total: 19,
        type_arguments_total: 23,
        errors: vec![ExternalExtractError {
            code: "schema_mismatch".to_string(),
            message: "database schema is not compatible".to_string(),
            path: Some(PathBuf::from("external.sqlite")),
        }],
    };

    let json = serde_json::to_value(&report).expect("report serializes");

    assert_eq!(
        json,
        json!({
            "status": "failed",
            "operation": "scan",
            "workspace_id": "ws_1",
            "db_path": "external.sqlite",
            "root": "/repo",
            "julie_version": "7.9.3",
            "schema_version": 26,
            "schema_state": "current",
            "extract_contract_version": 1,
            "revision": 42,
            "analyzed_revision": 41,
            "analysis_state": "stale",
            "missing_metadata_keys": [],
            "files_scanned": 3,
            "files_updated": 2,
            "files_deleted": 1,
            "symbols_extracted": 5,
            "files_total": 7,
            "symbols_total": 11,
            "relationships_total": 13,
            "identifiers_total": 17,
            "types_total": 19,
            "type_arguments_total": 23,
            "errors": [
                {
                    "code": "schema_mismatch",
                    "message": "database schema is not compatible",
                    "path": "external.sqlite"
                }
            ]
        })
    );
}

#[test]
fn external_extract_report_formats_text_json_and_markdown() {
    let report = ExternalExtractReport {
        status: ExternalExtractStatus::Changed,
        operation: "update".to_string(),
        workspace_id: Some("ws_1".to_string()),
        db: PathBuf::from("external.sqlite"),
        root: Some(PathBuf::from("/repo")),
        julie_version: Some("7.9.3".to_string()),
        schema_version: Some(26),
        schema_state: Some(ExternalInfoSchemaState::Current),
        extract_contract_version: Some(1),
        revision: Some(42),
        analyzed_revision: Some(41),
        analysis_state: Some("stale".to_string()),
        missing_metadata_keys: Vec::new(),
        files_scanned: 1,
        files_updated: 1,
        files_deleted: 0,
        symbols_extracted: 4,
        files_total: 9,
        symbols_total: 12,
        relationships_total: 14,
        identifiers_total: 16,
        types_total: 18,
        type_arguments_total: 20,
        errors: Vec::new(),
    };

    let text =
        format_external_extract_report(&report, OutputFormat::Text).expect("text report formats");
    assert!(text.contains("extract update: changed"));
    assert!(text.contains("db_path=external.sqlite"));
    assert!(text.contains("revision=42"));
    assert!(text.contains("extract_contract_version=1"));
    assert!(text.contains("files_updated=1"));
    assert!(text.contains("type_arguments_total=20"));

    let json =
        format_external_extract_report(&report, OutputFormat::Json).expect("json report formats");
    assert!(json.contains("\"status\": \"changed\""));
    assert!(json.contains("\"db_path\": \"external.sqlite\""));

    let markdown = format_external_extract_report(&report, OutputFormat::Markdown)
        .expect("markdown report formats");
    assert!(markdown.starts_with("# External Extract\n"));
    assert!(markdown.contains("| Operation | update |"));
    assert!(markdown.contains("| Status | changed |"));
    assert!(markdown.contains("| Type Arguments Total | 20 |"));
}
