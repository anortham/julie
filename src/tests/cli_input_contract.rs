//! Contract tests for CLI request input parsing, sizing, validation, and parameter sources.

use clap::Parser;
use std::io::Cursor;
use std::path::Path;

use crate::cli_tools::CliToolCommand;
use crate::cli_tools::input::{
    MAX_INPUT_BYTES, parse_request_input, read_bounded_input, resolve_parameter_map,
};
use crate::cli_tools::replay::parse_replay_line;
use crate::cli_tools::subcommands::{DeepDiveArgs, EditArgs, GenericToolArgs};
use crate::request_engine::RequestFailure;

#[test]
fn cli_request_input_rejects_multiple_json_values() {
    let error = parse_request_input(br#"{"query":"one"} {"query":"two"}"#, "stdin").unwrap_err();
    assert_eq!(error.code, "INVALID_ARGUMENTS");
    assert!(parse_request_input(br#"{"query":"one"}"#, "stdin").is_ok());
    assert!(parse_request_input(b"[]", "stdin").is_err());
}

#[test]
fn cli_request_input_enforces_16_mib_ceiling() {
    let mut large = vec![b' '; MAX_INPUT_BYTES + 1];
    large[0] = b'{';
    large[MAX_INPUT_BYTES] = b'}';
    let err = parse_request_input(&large, "stdin").unwrap_err();
    assert_eq!(err.code, "INVALID_ARGUMENTS");
    assert!(
        err.message.contains("stdin: input exceeds 16 MiB"),
        "unexpected error message: {}",
        err.message
    );
}

#[test]
fn cli_request_input_rejects_non_object_roots() {
    let cases: &[&[u8]] = &[
        b"[]",
        b"[1, 2, 3]",
        b"\"hello\"",
        b"42",
        b"true",
        b"false",
        b"null",
    ];
    for &case in cases {
        let err = parse_request_input(case, "inline").unwrap_err();
        assert_eq!(err.code, "INVALID_ARGUMENTS");
        assert!(
            err.message.contains("expected object"),
            "expected 'expected object' for {:?}, got: {}",
            case,
            err.message
        );
    }
}

#[test]
fn cli_request_input_validates_utf8() {
    let invalid_utf8 = b"{\"query\": \xff\xfe}";
    let err = parse_request_input(invalid_utf8, "file").unwrap_err();
    assert_eq!(err.code, "INVALID_ARGUMENTS");
    assert!(
        err.message.contains("invalid UTF-8"),
        "expected 'invalid UTF-8', got: {}",
        err.message
    );
}

#[test]
fn cli_request_input_rejects_empty_and_whitespace() {
    let err_empty = parse_request_input(b"", "stdin").unwrap_err();
    assert_eq!(err_empty.code, "INVALID_ARGUMENTS");

    let err_ws = parse_request_input(b"   \r\n\t  ", "file.json").unwrap_err();
    assert_eq!(err_ws.code, "INVALID_ARGUMENTS");
}

#[test]
fn cli_request_input_allows_trailing_whitespace() {
    let map = parse_request_input(b"{\"query\":\"test\"}  \r\n\t ", "stdin").unwrap();
    assert_eq!(map.get("query").unwrap().as_str().unwrap(), "test");
}

#[test]
fn cli_parameter_source_defaulting_returns_empty_object() {
    let map = resolve_parameter_map(None, None, false).unwrap();
    assert!(map.is_empty());
}

#[test]
fn cli_parameter_source_inline_parsing() {
    let map = resolve_parameter_map(Some(r#"{"query":"test"}"#), None, false).unwrap();
    assert_eq!(map.get("query").unwrap().as_str().unwrap(), "test");
}

#[test]
fn cli_parameter_source_mutual_exclusivity() {
    let err = resolve_parameter_map(Some("{}"), Some(Path::new("dummy.json")), false).unwrap_err();
    assert_eq!(err.code, "INVALID_ARGUMENTS");

    let err2 = resolve_parameter_map(Some("{}"), None, true).unwrap_err();
    assert_eq!(err2.code, "INVALID_ARGUMENTS");
}

#[test]
fn cli_read_bounded_input_empty_fails() {
    let err = read_bounded_input(Cursor::new(b""), "stdin").unwrap_err();
    assert_eq!(err.code, "INVALID_ARGUMENTS");
}

#[test]
fn cli_read_bounded_input_exceeds_16mib_fails() {
    let large_data = vec![b'a'; MAX_INPUT_BYTES + 10];
    let err = read_bounded_input(Cursor::new(large_data), "stdin").unwrap_err();
    assert_eq!(err.code, "INVALID_ARGUMENTS");
    assert!(err.message.contains("stdin: input exceeds 16 MiB"));
}

#[test]
fn cli_clap_arg_group_mutual_exclusion() {
    let res = GenericToolArgs::try_parse_from([
        "tool",
        "fast_search",
        "--params",
        "{}",
        "--params-file",
        "params.json",
    ]);
    assert!(res.is_err());
    assert_eq!(
        res.unwrap_err().kind(),
        clap::error::ErrorKind::ArgumentConflict
    );

    let res = GenericToolArgs::try_parse_from([
        "tool",
        "fast_search",
        "--params",
        "{}",
        "--params-stdin",
    ]);
    assert!(res.is_err());
    assert_eq!(
        res.unwrap_err().kind(),
        clap::error::ErrorKind::ArgumentConflict
    );

    let res = GenericToolArgs::try_parse_from([
        "tool",
        "fast_search",
        "--params-file",
        "params.json",
        "--params-stdin",
    ]);
    assert!(res.is_err());
    assert_eq!(
        res.unwrap_err().kind(),
        clap::error::ErrorKind::ArgumentConflict
    );

    assert!(GenericToolArgs::try_parse_from(["tool", "fast_search"]).is_ok());
    assert!(GenericToolArgs::try_parse_from(["tool", "fast_search", "--params", "{}"]).is_ok());
    assert!(
        GenericToolArgs::try_parse_from(["tool", "fast_search", "--params-file", "f.json"]).is_ok()
    );
    assert!(GenericToolArgs::try_parse_from(["tool", "fast_search", "--params-stdin"]).is_ok());
}

#[test]
fn cli_subcommands_map_to_canonical_mcp_tools() {
    let deep_dive = DeepDiveArgs {
        symbol: "MyStruct".into(),
        depth: Some("callers".into()),
        context_file: None,
        workspace: None,
    };
    assert_eq!(deep_dive.tool_name(), "deep_dive");
    let map = deep_dive.to_tool_args_map().unwrap();
    assert_eq!(map["symbol"], "MyStruct");
    assert_eq!(map["depth"], "callers");

    let edit = EditArgs {
        file_path: "src/main.rs".into(),
        old_text: "foo".into(),
        new_text: "bar".into(),
        dry_run: true,
        occurrence: Some("first".into()),
        workspace: None,
    };
    assert_eq!(edit.tool_name(), "edit_file");
    let map = edit.to_tool_args_map().unwrap();
    assert_eq!(map["file_path"], "src/main.rs");
    assert_eq!(map["dry_run"], true);
}

#[test]
fn cli_replay_assigns_line_numbers_for_missing_request_ids() {
    let line =
        r#"{"schema_version":1,"request":{"name":"fast_search","arguments":{"query":"test"}}}"#;
    let (req_id, req) = parse_replay_line(line, 42).unwrap();
    assert_eq!(req_id, "42");
    assert_eq!(req.name, "fast_search");
    assert_eq!(req.arguments["query"], "test");
}

#[test]
fn cli_replay_halts_on_malformed_framing() {
    let invalid_json = r#"{"schema_version":1,"request":{incomplete"#;
    let err = parse_replay_line(invalid_json, 5).unwrap_err();
    assert_eq!(err.code, "INVALID_ARGUMENTS");
    assert!(err.message.contains("Line 5: malformed framing"));

    let non_object_args =
        r#"{"schema_version":1,"request":{"name":"fast_search","arguments":"not_object"}}"#;
    let err = parse_replay_line(non_object_args, 6).unwrap_err();
    assert_eq!(err.code, "INVALID_ARGUMENTS");
    assert!(err.message.contains("arguments must be a JSON object"));
}

#[test]
fn cli_request_failure_exit_code_contract() {
    assert_eq!(RequestFailure::invalid_arguments("").exit_code(), 2);
    assert_eq!(RequestFailure::unknown_tool("", &[]).exit_code(), 2);
    assert_eq!(RequestFailure::workspace_required("").exit_code(), 2);
    assert_eq!(RequestFailure::workspace_conflict("").exit_code(), 2);
    assert_eq!(RequestFailure::foreground_required("").exit_code(), 2);
    assert_eq!(RequestFailure::sensitive_root("").exit_code(), 2);
    assert_eq!(
        RequestFailure::new("TOOL_ERROR", "", false, serde_json::json!({})).exit_code(),
        3
    );
    assert_eq!(
        RequestFailure::semantics_not_ready("", serde_json::json!({})).exit_code(),
        4
    );
    assert_eq!(
        RequestFailure::new("UNAVAILABLE", "", false, serde_json::json!({})).exit_code(),
        4
    );
    assert_eq!(
        RequestFailure::new("BUSY", "", false, serde_json::json!({})).exit_code(),
        4
    );
    assert_eq!(
        RequestFailure::new("STALE_EDIT", "", false, serde_json::json!({})).exit_code(),
        5
    );
    assert_eq!(RequestFailure::deadline_exceeded("").exit_code(), 124);
    assert_eq!(RequestFailure::cancelled("").exit_code(), 130);
    assert_eq!(RequestFailure::internal("").exit_code(), 1);
}

// ===========================================================================
// Adversarial Stress Tests (Teamwork Preview Challenger M2.1)
// ===========================================================================

#[test]
fn adversarial_multi_json_values_and_trailing_tokens_strictly_rejected() {
    let invalid_inputs: &[&[u8]] = &[
        br#"{"a": 1} {"b": 2}"#,
        br#"{"a": 1}{"b": 2}"#,
        br#"{"a": 1}   {"b": 2}"#,
        b"{\"a\": 1}\n{\"b\": 2}",
        br#"{"a": 1} [1, 2]"#,
        br#"{"a": 1} "hello""#,
        br#"{"a": 1} 123"#,
        br#"{"a": 1} true"#,
        br#"{"a": 1} false"#,
        br#"{"a": 1} null"#,
        br#"{"a": 1} ;"#,
        b"{\"a\": 1} \0",
        br#"{"a": 1} /* comment */"#,
        br#"{"a": 1} undefined"#,
        br#"{"a": 1} NaN"#,
    ];

    for &input in invalid_inputs {
        let res = parse_request_input(input, "adversarial_multi");
        assert!(
            res.is_err(),
            "Expected failure for multi/trailing token input: {:?}",
            String::from_utf8_lossy(input)
        );
        let err = res.unwrap_err();
        assert_eq!(err.code, "INVALID_ARGUMENTS");
    }
}

#[test]
fn adversarial_non_object_json_roots_strictly_rejected() {
    let non_objects: &[&[u8]] = &[
        b"[]",
        b"[1, 2, 3]",
        b"[{\"nested\": \"object\"}]",
        b"\"hello world\"",
        b"\"\"",
        b"123",
        b"-999",
        b"0",
        b"3.14159265",
        b"1e10",
        b"true",
        b"false",
        b"null",
    ];

    for &input in non_objects {
        let res = parse_request_input(input, "adversarial_root");
        assert!(
            res.is_err(),
            "Expected failure for non-object root: {:?}",
            String::from_utf8_lossy(input)
        );
        let err = res.unwrap_err();
        assert_eq!(err.code, "INVALID_ARGUMENTS");
        assert!(
            err.message.contains("expected object"),
            "Expected 'expected object' in error message for {:?}, got: {}",
            String::from_utf8_lossy(input),
            err.message
        );
    }
}

#[test]
fn adversarial_16_mib_exact_boundary_and_streaming_exhaustion() {
    // 1. Exactly 16 MiB valid JSON object should succeed
    let prefix = b"{\"pad\":\"";
    let suffix = b"\"}";
    let pad_len = MAX_INPUT_BYTES - prefix.len() - suffix.len();
    let mut exact_16_mib = Vec::with_capacity(MAX_INPUT_BYTES);
    exact_16_mib.extend_from_slice(prefix);
    exact_16_mib.resize(exact_16_mib.len() + pad_len, b'x');
    exact_16_mib.extend_from_slice(suffix);
    assert_eq!(exact_16_mib.len(), MAX_INPUT_BYTES);

    let res_exact = parse_request_input(&exact_16_mib, "exact_16_mib");
    assert!(
        res_exact.is_ok(),
        "16 MiB boundary input should be accepted"
    );

    // 2. Exactly 16 MiB + 1 byte must fail with input exceeds 16 MiB
    let mut over_16_mib = exact_16_mib;
    over_16_mib.push(b' ');
    assert_eq!(over_16_mib.len(), MAX_INPUT_BYTES + 1);
    let err_over = parse_request_input(&over_16_mib, "over_16_mib").unwrap_err();
    assert_eq!(err_over.code, "INVALID_ARGUMENTS");
    assert!(err_over.message.contains("input exceeds 16 MiB"));

    // 3. Infinite reader streaming through read_bounded_input must terminate boundedly without OOM
    let infinite_reader = std::io::repeat(b' ');
    let err_stream = read_bounded_input(infinite_reader, "infinite_stream").unwrap_err();
    assert_eq!(err_stream.code, "INVALID_ARGUMENTS");
    assert!(err_stream.message.contains("input exceeds 16 MiB"));
}

#[test]
fn adversarial_non_utf8_sequences_strictly_rejected() {
    let non_utf8: &[&[u8]] = &[
        b"{\"key\": \"\xff\xfe\"}",
        b"\x80",
        b"\xff",
        b"{\"key\": \"\xed\xa0\x80\"}",
        b"\xc0\xaf",
        b"\xe0\x80\xaf",
        b"\xf0\x80\x80\xaf",
    ];

    for &bytes in non_utf8 {
        let res = parse_request_input(bytes, "adversarial_utf8");
        assert!(res.is_err(), "Expected failure for non-UTF8 input");
        let err = res.unwrap_err();
        assert_eq!(err.code, "INVALID_ARGUMENTS");
        assert!(
            err.message.contains("invalid UTF-8"),
            "Expected 'invalid UTF-8' in error message for {:?}, got: {}",
            bytes,
            err.message
        );
    }
}

#[test]
fn adversarial_empty_input_vs_missing_parameter_defaulting() {
    // Empty reader (e.g. empty file or stdin) fails
    let err_empty_reader = read_bounded_input(Cursor::new(b""), "empty_reader").unwrap_err();
    assert_eq!(err_empty_reader.code, "INVALID_ARGUMENTS");

    // Whitespace reader fails
    let err_ws_reader = read_bounded_input(Cursor::new(b"   \r\n\t  "), "ws_reader").unwrap_err();
    assert_eq!(err_ws_reader.code, "INVALID_ARGUMENTS");

    // Empty inline string fails
    let err_empty_inline = resolve_parameter_map(Some(""), None, false).unwrap_err();
    assert_eq!(err_empty_inline.code, "INVALID_ARGUMENTS");

    // Whitespace inline string fails
    let err_ws_inline = resolve_parameter_map(Some("   \t\n  "), None, false).unwrap_err();
    assert_eq!(err_ws_inline.code, "INVALID_ARGUMENTS");

    // Missing parameter source defaults cleanly to empty map ({})
    let default_map = resolve_parameter_map(None, None, false).unwrap();
    assert!(default_map.is_empty());
}

#[test]
fn adversarial_clap_arg_group_and_programmatic_conflicts() {
    use crate::cli::Cli;

    // Clap GenericToolArgs conflicts: all combinations of 2 and 3 flags
    let conflict_args = [
        vec![
            "tool",
            "fast_search",
            "--params",
            "{}",
            "--params-file",
            "p.json",
        ],
        vec!["tool", "fast_search", "--params", "{}", "--params-stdin"],
        vec![
            "tool",
            "fast_search",
            "--params-file",
            "p.json",
            "--params-stdin",
        ],
        vec![
            "tool",
            "fast_search",
            "--params",
            "{}",
            "--params-file",
            "p.json",
            "--params-stdin",
        ],
    ];

    for args in &conflict_args {
        let res = GenericToolArgs::try_parse_from(args);
        assert!(
            res.is_err(),
            "GenericToolArgs should reject conflict: {:?}",
            args
        );
        assert_eq!(
            res.unwrap_err().kind(),
            clap::error::ErrorKind::ArgumentConflict
        );
    }

    // Full Cli conflicts through julie-server
    let cli_conflict_args = [
        vec![
            "julie-server",
            "tool",
            "fast_search",
            "--params",
            "{}",
            "--params-file",
            "p.json",
        ],
        vec![
            "julie-server",
            "tool",
            "fast_search",
            "--params",
            "{}",
            "--params-stdin",
        ],
        vec![
            "julie-server",
            "tool",
            "fast_search",
            "--params-file",
            "p.json",
            "--params-stdin",
        ],
        vec![
            "julie-server",
            "tool",
            "fast_search",
            "--params",
            "{}",
            "--params-file",
            "p.json",
            "--params-stdin",
        ],
    ];

    for args in &cli_conflict_args {
        match Cli::try_parse_from(args) {
            Err(e) => assert_eq!(
                e.kind(),
                clap::error::ErrorKind::ArgumentConflict,
                "Expected ArgumentConflict for Cli: {:?}",
                args
            ),
            Ok(_) => panic!("Cli unexpectedly parsed conflicting args: {:?}", args),
        }
    }

    // Programmatic resolve_parameter_map conflicts
    let dummy_path = Path::new("test.json");
    let programmatic_conflicts = [
        (Some("{}"), Some(dummy_path), false),
        (Some("{}"), None, true),
        (None, Some(dummy_path), true),
        (Some("{}"), Some(dummy_path), true),
    ];

    for (p, pf, ps) in programmatic_conflicts {
        let res = resolve_parameter_map(p, pf, ps);
        assert!(
            res.is_err(),
            "resolve_parameter_map should reject conflict ({:?}, {:?}, {:?})",
            p,
            pf,
            ps
        );
        let err = res.unwrap_err();
        assert_eq!(err.code, "INVALID_ARGUMENTS");
        assert!(err.message.contains("Mutually exclusive parameter sources"));
    }
}

// ===========================================================================
// Direct CLI Execution Error Envelope & Exit Code Contracts (Remediation M2.2)
// ===========================================================================

use crate::cli_tools::run_cli_tool;

#[tokio::test]
async fn cli_run_tool_preserves_typed_request_failure_and_exit_code() {
    let temp = tempfile::TempDir::new().expect("failed to create temp dir");
    let unknown_args = GenericToolArgs {
        name: "unknown_tool_xyz".to_string(),
        params: None,
        params_file: None,
        params_stdin: false,
        foreground: false,
    };

    let result = run_cli_tool(&unknown_args, Some(temp.path().to_path_buf()), true).await;
    assert!(result.is_err(), "unknown tool must fail");

    let failure: RequestFailure = result.unwrap_err();
    assert_eq!(failure.code, "UNKNOWN_TOOL");
    assert_ne!(failure.code, "INVALID_ARGUMENTS");
    assert_eq!(failure.exit_code(), 2);
    assert!(
        failure.details.get("tool").is_some(),
        "details must contain unknown tool info"
    );

    // Assert exit code mapping contracts for critical codes
    assert_eq!(failure.exit_code(), 2);
    assert_eq!(RequestFailure::tool_error("").exit_code(), 3);
    assert_eq!(
        RequestFailure::new("STALE_EDIT", "", false, serde_json::json!({})).exit_code(),
        5
    );
    assert_eq!(RequestFailure::deadline_exceeded("").exit_code(), 124);
    assert_eq!(RequestFailure::cancelled("").exit_code(), 130);
}

#[tokio::test]
async fn cli_run_tool_preserves_tool_error_and_exit_code_3() {
    let temp = tempfile::TempDir::new().expect("failed to create temp dir");
    let tool_error_args = GenericToolArgs {
        name: "get_symbols".to_string(),
        params: Some(r#"{"file_path":"nonexistent_file_xyz.rs"}"#.to_string()),
        params_file: None,
        params_stdin: false,
        foreground: false,
    };

    let result = run_cli_tool(&tool_error_args, Some(temp.path().to_path_buf()), true).await;
    let failure: RequestFailure = result.expect_err("nonexistent file query must return Err");
    assert_eq!(failure.code, "TOOL_ERROR");
    assert_ne!(failure.code, "INVALID_ARGUMENTS");
    assert_eq!(failure.exit_code(), 3);
}

#[test]
fn cli_subprocess_unknown_tool_json_failure_envelope_and_exit_code() {
    let binary = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/debug/julie-server{}",
        std::env::consts::EXE_SUFFIX
    ));
    if !binary.exists() {
        eprintln!("Skipping subprocess test: target/debug/julie-server does not exist");
        return;
    }
    let temp = tempfile::TempDir::new().expect("failed to create temp dir");
    let output = std::process::Command::new(&binary)
        .args([
            "tool",
            "unknown_tool_xyz",
            "--workspace",
            temp.path().to_str().unwrap(),
            "--standalone",
            "--json",
        ])
        .output()
        .expect("failed to run julie-server");

    assert_eq!(
        output.status.code(),
        Some(2),
        "unknown tool must exit with code 2"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout must be valid JSON envelope");

    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["ok"], false);
    assert_eq!(
        json["error"]["code"], "UNKNOWN_TOOL",
        "error code in envelope must be UNKNOWN_TOOL, got: {}",
        json["error"]["code"]
    );
    assert_ne!(
        json["error"]["code"], "INVALID_ARGUMENTS",
        "error code must not be overwritten to INVALID_ARGUMENTS"
    );
}

#[test]
fn cli_subprocess_tool_domain_error_json_failure_envelope_and_exit_code_3() {
    let binary = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/debug/julie-server{}",
        std::env::consts::EXE_SUFFIX
    ));
    if !binary.exists() {
        eprintln!("Skipping subprocess test: target/debug/julie-server does not exist");
        return;
    }
    let temp = tempfile::TempDir::new().expect("failed to create temp dir");
    let output = std::process::Command::new(&binary)
        .args([
            "tool",
            "get_symbols",
            "--params",
            r#"{"file_path":"nonexistent_file_xyz.rs"}"#,
            "--workspace",
            temp.path().to_str().unwrap(),
            "--standalone",
            "--json",
        ])
        .output()
        .expect("failed to run julie-server");

    assert_eq!(
        output.status.code(),
        Some(3),
        "tool domain failure must exit with code 3"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout must be valid JSON envelope");

    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["ok"], false);
    assert_eq!(
        json["error"]["code"], "TOOL_ERROR",
        "error code in envelope must be TOOL_ERROR, got: {}",
        json["error"]["code"]
    );
    assert_ne!(
        json["error"]["code"], "INVALID_ARGUMENTS",
        "error code must not be overwritten to INVALID_ARGUMENTS"
    );
}

#[test]
fn cli_subcommands_parse_with_global_and_target_workspace_flags() {
    use crate::cli::{Cli, Command};
    use crate::cli_tools::subcommands::OutputFormat;
    use std::path::PathBuf;

    // 1. Subcommands that support both global --workspace and subcommand --target-workspace:
    // (deep-dive, edit, rename, rewrite, refs, call-path)
    struct TargetWsCase<'a> {
        subcommand: &'a str,
        extra_args: &'a [&'a str],
        verify: fn(&Command, Option<&str>),
    }

    let target_cases = [
        TargetWsCase {
            subcommand: "deep-dive",
            extra_args: &["MySymbol"],
            verify: |cmd, expected_target| match cmd {
                Command::DeepDive(args) => {
                    assert_eq!(args.symbol, "MySymbol");
                    assert_eq!(args.workspace.as_deref(), expected_target);
                }
                _ => panic!("expected DeepDive command"),
            },
        },
        TargetWsCase {
            subcommand: "edit",
            extra_args: &["src/main.rs", "-o", "foo", "-n", "bar"],
            verify: |cmd, expected_target| match cmd {
                Command::Edit(args) => {
                    assert_eq!(args.file_path, "src/main.rs");
                    assert_eq!(args.old_text, "foo");
                    assert_eq!(args.new_text, "bar");
                    assert_eq!(args.workspace.as_deref(), expected_target);
                }
                _ => panic!("expected Edit command"),
            },
        },
        TargetWsCase {
            subcommand: "refs",
            extra_args: &["MySymbol"],
            verify: |cmd, expected_target| match cmd {
                Command::Refs(args) => {
                    assert_eq!(args.symbol, "MySymbol");
                    assert_eq!(args.workspace.as_deref(), expected_target);
                }
                _ => panic!("expected Refs command"),
            },
        },
        TargetWsCase {
            subcommand: "call-path",
            extra_args: &["sym_a", "sym_b"],
            verify: |cmd, expected_target| match cmd {
                Command::CallPath(args) => {
                    assert_eq!(args.from, "sym_a");
                    assert_eq!(args.to, "sym_b");
                    assert_eq!(args.workspace.as_deref(), expected_target);
                }
                _ => panic!("expected CallPath command"),
            },
        },
    ];

    for case in &target_cases {
        // Flag position 1: Global --workspace before subcommand
        let mut argv = vec![
            "julie-server",
            "--workspace",
            "/global/path",
            case.subcommand,
        ];
        argv.extend_from_slice(case.extra_args);
        let cli = Cli::try_parse_from(&argv).unwrap_or_else(|e| {
            panic!(
                "failed to parse prefix --workspace for {}: {e}",
                case.subcommand
            )
        });
        assert_eq!(cli.workspace, Some(PathBuf::from("/global/path")));
        (case.verify)(cli.command.as_ref().unwrap(), None);

        // Flag position 2: --target-workspace after subcommand
        let mut argv = vec!["julie-server", case.subcommand];
        argv.extend_from_slice(case.extra_args);
        argv.extend_from_slice(&["--target-workspace", "/target/path"]);
        let cli = Cli::try_parse_from(&argv).unwrap_or_else(|e| {
            panic!(
                "failed to parse --target-workspace for {}: {e}",
                case.subcommand
            )
        });
        assert_eq!(cli.workspace, None);
        (case.verify)(cli.command.as_ref().unwrap(), Some("/target/path"));

        // Flag position 3: Global --workspace after subcommand (trailing global flag)
        let mut argv = vec!["julie-server", case.subcommand];
        argv.extend_from_slice(case.extra_args);
        argv.extend_from_slice(&["--workspace", "/global/path"]);
        let cli = Cli::try_parse_from(&argv).unwrap_or_else(|e| {
            panic!(
                "failed to parse trailing --workspace for {}: {e}",
                case.subcommand
            )
        });
        assert_eq!(cli.workspace, Some(PathBuf::from("/global/path")));
        (case.verify)(cli.command.as_ref().unwrap(), None);

        // Flag position 4: Both global --workspace AND subcommand --target-workspace
        let mut argv = vec![
            "julie-server",
            "--workspace",
            "/global/path",
            case.subcommand,
        ];
        argv.extend_from_slice(case.extra_args);
        argv.extend_from_slice(&["--target-workspace", "/target/path"]);
        let cli = Cli::try_parse_from(&argv).unwrap_or_else(|e| {
            panic!(
                "failed to parse both workspace flags for {}: {e}",
                case.subcommand
            )
        });
        assert_eq!(cli.workspace, Some(PathBuf::from("/global/path")));
        (case.verify)(cli.command.as_ref().unwrap(), Some("/target/path"));
    }

    // 2. Subcommands that support only global --workspace:
    // (search, symbols, context, blast-radius, workspace, patterns)
    let global_only_cases: &[(&str, &[&str])] = &[
        ("search", &["my_query"]),
        ("symbols", &["src/lib.rs"]),
        ("context", &["my_query"]),
        ("blast-radius", &["-f", "src/lib.rs"]),
        ("workspace", &["list"]),
        ("patterns", &[]),
    ];

    for &(subcommand, extra_args) in global_only_cases {
        // Global --workspace before subcommand
        let mut argv = vec!["julie-server", "--workspace", "/global/path", subcommand];
        argv.extend_from_slice(extra_args);
        let cli = Cli::try_parse_from(&argv)
            .unwrap_or_else(|e| panic!("failed to parse prefix --workspace for {subcommand}: {e}"));
        assert_eq!(cli.workspace, Some(PathBuf::from("/global/path")));

        // Global --workspace after subcommand
        let mut argv = vec!["julie-server", subcommand];
        argv.extend_from_slice(extra_args);
        argv.extend_from_slice(&["--workspace", "/global/path"]);
        let cli = Cli::try_parse_from(&argv).unwrap_or_else(|e| {
            panic!("failed to parse trailing --workspace for {subcommand}: {e}")
        });
        assert_eq!(cli.workspace, Some(PathBuf::from("/global/path")));

        // Rejection check: subcommands without --target-workspace must reject it
        let mut argv = vec!["julie-server", subcommand];
        argv.extend_from_slice(extra_args);
        argv.extend_from_slice(&["--target-workspace", "/target/path"]);
        assert!(
            Cli::try_parse_from(&argv).is_err(),
            "subcommand {subcommand} unexpectedly accepted --target-workspace"
        );
    }
}
