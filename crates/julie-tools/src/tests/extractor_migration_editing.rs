use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use crate::editing::syntax::{SyntaxAdapter, SyntaxAdapterError, SyntaxConfig};

#[test]
fn migrated_parser_renames_identifiers_without_touching_literals() {
    let source = "class Example { run() { return \"Example\"; } }";
    let tool = crate::refactoring::SmartRefactorTool {
        operation: "rename_symbol".to_owned(),
        params: "{}".to_owned(),
        dry_run: true,
    };
    let output = tool
        .smart_text_replace(source, "Example", "Renamed", "sample.ts", false)
        .unwrap();
    assert_eq!(output, "class Renamed { run() { return \"Example\"; } }");
}

#[test]
fn adapter_rejects_pre_cancelled_input() {
    let adapter = SyntaxAdapter::default();
    let cancelled = AtomicBool::new(true);
    let result = adapter.parse_source(
        Path::new("sample.rs"),
        "fn main() {}",
        None,
        Some(&cancelled),
    );
    match result {
        Err(SyntaxAdapterError::Cancelled) => {}
        other => panic!("expected Cancelled, got {:?}", other),
    }
}

#[test]
fn adapter_rejects_expired_deadline() {
    let adapter = SyntaxAdapter::default();
    let expired = Instant::now() - Duration::from_secs(1);
    let result = adapter.parse_source(Path::new("sample.rs"), "fn main() {}", Some(expired), None);
    match result {
        Err(SyntaxAdapterError::DeadlineExceeded) => {}
        other => panic!("expected DeadlineExceeded, got {:?}", other),
    }
}

#[test]
fn adapter_rejects_oversized_source() {
    let config = SyntaxConfig {
        max_source_bytes: 10,
        default_timeout: Duration::from_secs(5),
        max_concurrent_parses: 2,
    };
    let adapter = SyntaxAdapter::new(config).unwrap();
    let content = "fn long_function_name_exceeds_max_bytes() {}";
    let result = adapter.parse_source(Path::new("sample.rs"), content, None, None);
    match result {
        Err(SyntaxAdapterError::InputTooLarge { bytes }) => {
            assert_eq!(bytes, content.len());
        }
        other => panic!("expected InputTooLarge, got {:?}", other),
    }
}

#[test]
fn adapter_cancels_worker_and_retains_permit_until_joined() {
    let config = SyntaxConfig {
        max_source_bytes: 1024 * 1024,
        default_timeout: Duration::from_secs(10),
        max_concurrent_parses: 1,
    };
    let adapter = SyntaxAdapter::new(config).unwrap();
    assert_eq!(adapter.available_permits(), 1);

    let cancelled = AtomicBool::new(true);
    let result = adapter.parse_source(
        Path::new("sample.rs"),
        "fn main() {}",
        None,
        Some(&cancelled),
    );
    assert!(matches!(result, Err(SyntaxAdapterError::Cancelled)));
    assert_eq!(adapter.available_permits(), 1);
}

#[test]
fn adapter_cancels_active_worker_and_retains_permit_until_joined() {
    let config = SyntaxConfig {
        max_source_bytes: 1024 * 1024,
        default_timeout: Duration::from_secs(5),
        max_concurrent_parses: 1,
    };
    let adapter = std::sync::Arc::new(SyntaxAdapter::new(config).unwrap());
    assert_eq!(adapter.available_permits(), 1);

    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (unblock_tx, unblock_rx) = std::sync::mpsc::channel();
    let cancelled = std::sync::Arc::new(AtomicBool::new(false));

    let adapter_clone = std::sync::Arc::clone(&adapter);
    let cancelled_clone = std::sync::Arc::clone(&cancelled);
    let caller_handle = std::thread::spawn(move || {
        adapter_clone.parse_and_extract_with_sync(
            Path::new("sample.rs"),
            "fn main() {}",
            Path::new("."),
            None,
            Some(&cancelled_clone),
            move || {
                let _ = started_tx.send(());
                let _ = unblock_rx.recv();
            },
        )
    });

    started_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("worker thread must start");

    assert_eq!(adapter.available_permits(), 0);

    cancelled.store(true, std::sync::atomic::Ordering::Release);

    let caller_result = caller_handle.join().expect("caller thread must join");
    assert!(matches!(caller_result, Err(SyntaxAdapterError::Cancelled)));

    assert_eq!(adapter.available_permits(), 0);

    let short_deadline = Instant::now() + Duration::from_millis(20);
    let blocked_res = adapter.parse_source(
        Path::new("sample.rs"),
        "fn main() {}",
        Some(short_deadline),
        None,
    );
    assert!(matches!(
        blocked_res,
        Err(SyntaxAdapterError::DeadlineExceeded)
    ));

    let _ = unblock_tx.send(());

    adapter.drain_cancelled_workers();

    assert_eq!(adapter.available_permits(), 1);

    let parse_res = adapter.parse_source(Path::new("sample.rs"), "fn main() {}", None, None);
    assert!(parse_res.is_ok());
}

#[test]
fn test_parse_and_extract_runs_under_admission_and_retains_permit() {
    let config = SyntaxConfig {
        max_source_bytes: 1024 * 1024,
        default_timeout: Duration::from_secs(5),
        max_concurrent_parses: 1,
    };
    let adapter = SyntaxAdapter::new(config).unwrap();
    assert_eq!(adapter.available_permits(), 1);

    adapter
        .limiter()
        .acquire(None, None)
        .expect("permit acquisition should succeed");
    assert_eq!(adapter.available_permits(), 0);

    let short_deadline = Instant::now() + Duration::from_millis(20);
    let blocked_res = adapter.parse_and_extract(
        Path::new("sample.rs"),
        "fn main() {}",
        Path::new("."),
        Some(short_deadline),
        None,
    );
    assert!(
        matches!(blocked_res, Err(SyntaxAdapterError::DeadlineExceeded)),
        "parse_and_extract must fail with DeadlineExceeded when admission permits are exhausted"
    );
    assert_eq!(adapter.available_permits(), 0);

    adapter.limiter().release();
    assert_eq!(adapter.available_permits(), 1);

    let (parsed, extracted) = adapter
        .parse_and_extract(
            Path::new("sample.rs"),
            "fn main() { println!(\"hello\"); }",
            Path::new("."),
            None,
            None,
        )
        .expect("parse_and_extract should succeed");
    assert_eq!(parsed.language, "rust");
    assert!(!extracted.symbols.is_empty());
    assert_eq!(
        adapter.available_permits(),
        1,
        "permit must be released back to 1 after completion"
    );
}

#[test]
fn fixture_cpp_header_handling() {
    let source = include_str!("../../../../fixtures/extractor-migration/cpp_header.h");
    let tool = crate::refactoring::SmartRefactorTool {
        operation: "rename_symbol".to_owned(),
        params: "{}".to_owned(),
        dry_run: true,
    };
    let replaced = tool
        .smart_text_replace(source, "Widget", "Gadget", "cpp_header.h", false)
        .expect("C++ header rename should succeed");
    assert!(replaced.contains("class Gadget"));
    assert!(replaced.contains("Gadget();"));
    assert!(replaced.contains("virtual ~Gadget();"));
    assert!(replaced.contains("namespace Sample"));
}

#[test]
fn fixture_c_sample_handling() {
    let source = include_str!("../../../../fixtures/extractor-migration/c_sample.c");
    let control = include_str!("../../../../fixtures/extractor-migration/c_sample.control.c");
    let tool = crate::refactoring::SmartRefactorTool {
        operation: "rename_symbol".to_owned(),
        params: "{}".to_owned(),
        dry_run: true,
    };
    let replaced = tool
        .smart_text_replace(
            source,
            "calculate_area",
            "compute_area",
            "c_sample.c",
            false,
        )
        .expect("C function rename should succeed");
    assert_eq!(replaced, control);

    let extracted =
        julie_extractors::extract_canonical("c_sample.c", source, Path::new(".")).unwrap();
    assert!(
        extracted.symbols.iter().any(|s| s.name == "calculate_area"),
        "C function symbol extracted"
    );
}

#[test]
fn fixture_rust_unicode_crlf_preservation() {
    let source = include_str!("../../../../fixtures/extractor-migration/rust_unicode_crlf.rs");
    let control =
        include_str!("../../../../fixtures/extractor-migration/rust_unicode_crlf.control.rs");
    let tool = crate::refactoring::SmartRefactorTool {
        operation: "rename_symbol".to_owned(),
        params: "{}".to_owned(),
        dry_run: true,
    };
    let output = tool
        .smart_text_replace(
            source,
            "café",
            "café_renamed",
            "rust_unicode_crlf.rs",
            false,
        )
        .unwrap();
    assert_eq!(output, control);
}

#[test]
fn fixture_sample_ts_preservation() {
    let source = include_str!("../../../../fixtures/extractor-migration/sample.ts");
    let control = include_str!("../../../../fixtures/extractor-migration/sample.control.ts");
    let tool = crate::refactoring::SmartRefactorTool {
        operation: "rename_symbol".to_owned(),
        params: "{}".to_owned(),
        dry_run: true,
    };
    let output = tool
        .smart_text_replace(source, "Example", "Renamed", "sample.ts", false)
        .unwrap();
    assert_eq!(output, control);
}

#[test]
fn fixture_vue_component_host_ast_and_embedded_characterization() {
    let source = include_str!("../../../../fixtures/extractor-migration/component.vue");
    let control = include_str!("../../../../fixtures/extractor-migration/component.control.vue");
    let adapter = SyntaxAdapter::default();
    let parsed = adapter
        .parse_source(Path::new("component.vue"), source, None, None)
        .unwrap();
    assert_eq!(parsed.language, "vue");
    assert!(parsed.diagnostics.is_empty());

    let root = parsed.tree.root_node();
    assert!(root.child_count() > 0);

    let tool = crate::refactoring::SmartRefactorTool {
        operation: "rename_symbol".to_owned(),
        params: "{}".to_owned(),
        dry_run: true,
    };
    let replaced = tool
        .smart_text_replace(source, "title", "componentTitle", "component.vue", false)
        .unwrap();
    assert_eq!(
        replaced, control,
        "Vue host AST preserves raw script content without naive text substitution"
    );

    let extracted =
        julie_extractors::extract_canonical("component.vue", source, Path::new(".")).unwrap();
    assert!(
        !extracted.symbols.is_empty(),
        "canonical extraction extracts Vue script symbols"
    );
}

#[test]
fn fixture_markdown_document_fenced_code_characterization() {
    let source = include_str!("../../../../fixtures/extractor-migration/document.md");
    let control = include_str!("../../../../fixtures/extractor-migration/document.control.md");
    let adapter = SyntaxAdapter::default();
    let parsed = adapter
        .parse_source(Path::new("document.md"), source, None, None)
        .unwrap();
    assert_eq!(parsed.language, "markdown");
    assert!(parsed.diagnostics.is_empty());

    let root = parsed.tree.root_node();
    assert!(root.child_count() > 0);

    let tool = crate::refactoring::SmartRefactorTool {
        operation: "rename_symbol".to_owned(),
        params: "{}".to_owned(),
        dry_run: true,
    };
    let replaced = tool
        .smart_text_replace(source, "main", "entrypoint", "document.md", false)
        .unwrap();
    assert_eq!(
        replaced, control,
        "Markdown host AST preserves fenced code blocks without naive text substitution"
    );

    let extracted =
        julie_extractors::extract_canonical("document.md", source, Path::new(".")).unwrap();
    let heading = extracted
        .symbols
        .iter()
        .find(|s| s.name.contains("Documentation"));
    assert!(heading.is_some(), "heading symbol extracted from markdown");
}

#[test]
fn fixture_sample_fs_parsing() {
    let source = include_str!("../../../../fixtures/extractor-migration/sample.fs");
    let adapter = SyntaxAdapter::default();
    let parsed = adapter
        .parse_source(Path::new("sample.fs"), source, None, None)
        .unwrap();
    assert_eq!(parsed.language, "fsharp");
    assert!(parsed.diagnostics.is_empty());
}

#[test]
fn fixture_qmldir_handling() {
    let source = include_str!("../../../../fixtures/extractor-migration/qmldir");
    let adapter = SyntaxAdapter::default();
    let parsed = adapter
        .parse_source(Path::new("qmldir"), source, None, None)
        .unwrap();
    assert_eq!(parsed.language, "qmldir");
    assert!(parsed.diagnostics.is_empty());
}

#[test]
fn fixture_jsonl_unsupported_container_rejection() {
    let source = include_str!("../../../../fixtures/extractor-migration/sample.jsonl");
    let adapter = SyntaxAdapter::default();
    let res = adapter.parse_source(Path::new("sample.jsonl"), source, None, None);
    assert!(matches!(
        res,
        Err(SyntaxAdapterError::UnsupportedContainer { .. })
    ));
}

#[test]
fn challenge_syntax_adapter_unsupported_container_jsonl() {
    let adapter = SyntaxAdapter::default();
    let content = "{\"id\": 1, \"name\": \"alpha\"}\n{\"id\": 2, \"name\": \"beta\"}\n";
    let result = adapter.parse_source(Path::new("records.jsonl"), content, None, None);
    match result {
        Err(SyntaxAdapterError::UnsupportedContainer { path }) => {
            assert_eq!(path, std::path::PathBuf::from("records.jsonl"));
        }
        other => panic!("expected UnsupportedContainer, got {:?}", other),
    }
}

#[test]
fn challenge_smart_text_replace_refuses_unsupported_container_jsonl() {
    let tool = crate::refactoring::SmartRefactorTool {
        operation: "rename_symbol".to_owned(),
        params: "{}".to_owned(),
        dry_run: true,
    };
    let content = "{\"name\": \"alpha\"}\n{\"name\": \"alpha\"}\n";
    let result = tool.smart_text_replace(content, "alpha", "beta", "records.jsonl", false);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("unsupported_container") || err_msg.contains("unsupported container"),
        "expected unsupported container refusal, got: {}",
        err_msg
    );
}

#[test]
fn challenge_smart_text_replace_designated_handling_unsupported_language_fallback() {
    let tool = crate::refactoring::SmartRefactorTool {
        operation: "rename_symbol".to_owned(),
        params: "{}".to_owned(),
        dry_run: true,
    };
    let content = "DB_HOST=old_host\nDB_PORT=5432\n";
    let result = tool
        .smart_text_replace(content, "old_host", "new_host", "app.env", false)
        .unwrap();
    assert_eq!(result, "DB_HOST=new_host\nDB_PORT=5432\n");
}

#[test]
fn challenge_smart_text_replace_refuses_parse_error_trees_across_languages() {
    let tool = crate::refactoring::SmartRefactorTool {
        operation: "rename_symbol".to_owned(),
        params: "{}".to_owned(),
        dry_run: true,
    };

    let bad_ts = "function calculate( { return 42; }";
    let err_ts = tool
        .smart_text_replace(bad_ts, "calculate", "compute", "sample.ts", false)
        .expect_err("must reject TS parse error");
    assert!(err_ts.to_string().contains("parse error"));

    let bad_rs = "fn calculate( { 42 }";
    let err_rs = tool
        .smart_text_replace(bad_rs, "calculate", "compute", "sample.rs", false)
        .expect_err("must reject Rust parse error");
    assert!(err_rs.to_string().contains("parse error"));

    let bad_py = "def calculate(:\n    return 42\n";
    let err_py = tool
        .smart_text_replace(bad_py, "calculate", "compute", "sample.py", false)
        .expect_err("must reject Python parse error");
    assert!(err_py.to_string().contains("parse error"));
}

#[test]
fn challenge_syntax_adapter_cancellation_and_deadline_invariants() {
    let cancelled_err = SyntaxAdapterError::Cancelled;
    assert_eq!(cancelled_err.error_kind(), "cancelled");
    assert!(!cancelled_err.is_unsupported_language());

    let deadline_err = SyntaxAdapterError::DeadlineExceeded;
    assert_eq!(deadline_err.error_kind(), "deadline_exceeded");
    assert!(!deadline_err.is_unsupported_language());

    let container_err = SyntaxAdapterError::UnsupportedContainer {
        path: std::path::PathBuf::from("data.jsonl"),
    };
    assert_eq!(container_err.error_kind(), "unsupported_container");
    assert!(!container_err.is_unsupported_language());

    let input_too_large_err = SyntaxAdapterError::InputTooLarge { bytes: 99999999 };
    assert_eq!(input_too_large_err.error_kind(), "input_too_large");
    assert!(!input_too_large_err.is_unsupported_language());

    let adapter = SyntaxAdapter::default();
    let cancelled = AtomicBool::new(true);
    let res_cancelled =
        adapter.parse_source(Path::new("test.ts"), "const x = 1;", None, Some(&cancelled));
    assert!(matches!(res_cancelled, Err(SyntaxAdapterError::Cancelled)));

    let expired = Instant::now() - Duration::from_millis(50);
    let res_expired =
        adapter.parse_source(Path::new("test.ts"), "const x = 1;", Some(expired), None);
    assert!(matches!(
        res_expired,
        Err(SyntaxAdapterError::DeadlineExceeded)
    ));
}

#[test]
fn ast_editing_javascript_renames_parameter_without_touching_strings() {
    let source = "function greet(user) { return \"hello user \" + user; }";
    let tool = crate::refactoring::SmartRefactorTool {
        operation: "rename_symbol".to_owned(),
        params: "{}".to_owned(),
        dry_run: true,
    };
    let output = tool
        .smart_text_replace(source, "user", "person", "sample.js", false)
        .unwrap();
    assert_eq!(
        output,
        "function greet(person) { return \"hello user \" + person; }"
    );
}

#[test]
fn ast_editing_jsx_renames_props_identifier() {
    let source = "function Component(props) { return <div>{props.title}</div>; }";
    let tool = crate::refactoring::SmartRefactorTool {
        operation: "rename_symbol".to_owned(),
        params: "{}".to_owned(),
        dry_run: true,
    };
    let output = tool
        .smart_text_replace(source, "props", "properties", "sample.jsx", false)
        .unwrap();
    assert_eq!(
        output,
        "function Component(properties) { return <div>{properties.title}</div>; }"
    );
}

#[test]
fn ast_editing_tsx_renames_props_with_type_annotation() {
    let source = "interface Props { name: string; }\nfunction View(props: Props) { return <span>{props.name}</span>; }";
    let tool = crate::refactoring::SmartRefactorTool {
        operation: "rename_symbol".to_owned(),
        params: "{}".to_owned(),
        dry_run: true,
    };
    let output = tool
        .smart_text_replace(source, "props", "viewProps", "sample.tsx", false)
        .unwrap();
    assert_eq!(
        output,
        "interface Props { name: string; }\nfunction View(viewProps: Props) { return <span>{viewProps.name}</span>; }"
    );
}

#[test]
fn ast_editing_python_renames_parameter_without_touching_strings() {
    let source = "def calculate(tax):\n    note = \"tax is applied\"\n    return tax * 2\n";
    let tool = crate::refactoring::SmartRefactorTool {
        operation: "rename_symbol".to_owned(),
        params: "{}".to_owned(),
        dry_run: true,
    };
    let output = tool
        .smart_text_replace(source, "tax", "vat", "sample.py", false)
        .unwrap();
    assert_eq!(
        output,
        "def calculate(vat):\n    note = \"tax is applied\"\n    return vat * 2\n"
    );
}

#[test]
fn ast_editing_java_renames_parameter_identifier() {
    let source = "public class Sample { public int compute(int delta) { return delta * 2; } }";
    let tool = crate::refactoring::SmartRefactorTool {
        operation: "rename_symbol".to_owned(),
        params: "{}".to_owned(),
        dry_run: true,
    };
    let output = tool
        .smart_text_replace(source, "delta", "multiplier", "Sample.java", false)
        .unwrap();
    assert_eq!(
        output,
        "public class Sample { public int compute(int multiplier) { return multiplier * 2; } }"
    );
}

#[test]
fn ast_editing_csharp_renames_parameter_identifier() {
    let source = "public class Calculator { public int Add(int first, int second) { return first + second; } }";
    let tool = crate::refactoring::SmartRefactorTool {
        operation: "rename_symbol".to_owned(),
        params: "{}".to_owned(),
        dry_run: true,
    };
    let output = tool
        .smart_text_replace(source, "first", "initial", "Sample.cs", false)
        .unwrap();
    assert_eq!(
        output,
        "public class Calculator { public int Add(int initial, int second) { return initial + second; } }"
    );
}

#[test]
fn ast_editing_go_renames_parameter_identifier() {
    let source = "package main\nfunc Multiply(factor int) int {\n\treturn factor * 2\n}\n";
    let tool = crate::refactoring::SmartRefactorTool {
        operation: "rename_symbol".to_owned(),
        params: "{}".to_owned(),
        dry_run: true,
    };
    let output = tool
        .smart_text_replace(source, "factor", "scalar", "sample.go", false)
        .unwrap();
    assert_eq!(
        output,
        "package main\nfunc Multiply(scalar int) int {\n\treturn scalar * 2\n}\n"
    );
}

#[test]
fn ast_editing_json_syntax_handling() {
    let source = "{\n  \"name\": \"julie\",\n  \"count\": 42\n}\n";
    let adapter = SyntaxAdapter::default();
    let parsed = adapter
        .parse_source(Path::new("package.json"), source, None, None)
        .unwrap();
    assert_eq!(parsed.language, "json");
    assert!(parsed.diagnostics.is_empty());

    let tool = crate::refactoring::SmartRefactorTool {
        operation: "rename_symbol".to_owned(),
        params: "{}".to_owned(),
        dry_run: true,
    };
    let output = tool
        .smart_text_replace(source, "julie", "julie-renamed", "package.json", false)
        .unwrap();
    assert_eq!(output, source);

    let malformed = "{\n  \"name\": \n";
    let err = tool
        .smart_text_replace(malformed, "name", "renamed", "package.json", false)
        .unwrap_err();
    assert!(err.to_string().contains("parse error"));
}
