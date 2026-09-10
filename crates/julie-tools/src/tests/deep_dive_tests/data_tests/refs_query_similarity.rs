use super::*;

const ENGINE: &str = "pub fn process() {}\n";
const CALL_PROCESS: &str = "fn run() {\n    process();\n}\n";
const TEST_CALLING_PROCESS: &str = "#[test]\nfn searches() {\n    process();\n}\n";

#[test]
fn test_identifier_fallback_adds_refs() {
    let (_dir, fixture) = fixture(&[
        ("src/engine.rs", ENGINE),
        ("src/main.rs", &at_line(24, CALL_PROCESS)),
    ]);
    let snapshot = fixture.snapshot();
    let target = only(&snapshot, "process");

    let ctx = build_symbol_context(&snapshot, target, "overview", 10, 10).unwrap();

    assert_eq!(ctx.incoming.len(), 1, "identifier fallback should add ref");
    assert_eq!(ctx.incoming[0].file_path, "src/main.rs");
    assert_eq!(ctx.incoming[0].line_number, 25);
    assert_eq!(ctx.incoming_total, 1);
}

#[test]
fn test_identifier_fallback_deduplicates() {
    let (_dir, fixture) = fixture(&[
        ("src/engine.rs", ENGINE),
        (
            "src/main.rs",
            &at_line(
                5,
                "fn main() {\n    let ready = true;\n    let _ = ready;\n    process();\n}\n",
            ),
        ),
        ("src/handler.rs", &at_line(41, CALL_PROCESS)),
    ]);
    let snapshot = fixture.snapshot();
    let target = only(&snapshot, "process");

    let ctx = build_symbol_context(&snapshot, target, "overview", 10, 10).unwrap();

    assert_eq!(
        ctx.incoming.len(),
        2,
        "should have 1 relationship + 1 identifier ref, got {}",
        ctx.incoming.len()
    );
    assert_eq!(ctx.incoming_total, 2);
}

#[test]
fn test_identifier_fallback_filters_own_file_definition_line() {
    let (_dir, fixture) = fixture(&[
        (
            "src/engine.rs",
            &at_line(10, "pub fn process() {\n    process();\n}\n"),
        ),
        ("src/main.rs", &at_line(29, CALL_PROCESS)),
    ]);
    let snapshot = fixture.snapshot();
    let target = only(&snapshot, "process");

    let ctx = build_symbol_context(&snapshot, target, "overview", 10, 10).unwrap();

    assert_eq!(
        ctx.incoming.len(),
        1,
        "should skip definition-site identifiers"
    );
    assert_eq!(ctx.incoming[0].file_path, "src/main.rs");
}

#[test]
fn test_build_context_populates_test_refs_at_full_depth() {
    let (_dir, fixture) = fixture(&[
        ("src/engine.rs", ENGINE),
        (
            "src/tests/search_tests.rs",
            &at_line(40, TEST_CALLING_PROCESS),
        ),
        ("src/main.rs", &at_line(24, CALL_PROCESS)),
    ]);
    let snapshot = fixture.snapshot();
    let target = only(&snapshot, "process");

    let ctx = build_symbol_context(&snapshot, target, "full", 10, 10).unwrap();

    assert_eq!(ctx.test_refs.len(), 1, "should have 1 test ref");
    assert_eq!(ctx.test_refs[0].file_path, "src/tests/search_tests.rs");
}

#[test]
fn test_build_context_no_test_refs_at_overview() {
    let (_dir, fixture) = fixture(&[
        ("src/engine.rs", ENGINE),
        (
            "src/tests/search_tests.rs",
            &at_line(40, TEST_CALLING_PROCESS),
        ),
    ]);
    let snapshot = fixture.snapshot();
    let target = only(&snapshot, "process");

    let ctx = build_symbol_context(&snapshot, target, "overview", 10, 10).unwrap();

    assert!(
        ctx.test_refs.is_empty(),
        "overview should not populate test_refs"
    );
}

#[test]
fn test_build_context_caps_incoming() {
    let callers: String = (0..5)
        .map(|i| format!("fn caller_{i}() {{\n    process();\n}}\n"))
        .collect();
    let (_dir, fixture) = fixture(&[("src/engine.rs", ENGINE), ("src/main.rs", &callers)]);
    let snapshot = fixture.snapshot();
    let target = only(&snapshot, "process");

    let ctx = build_symbol_context(&snapshot, target, "overview", 2, 10).unwrap();

    assert_eq!(ctx.incoming.len(), 2, "should cap at 2");
    assert_eq!(ctx.incoming_total, 5, "total should reflect all 5");
}

#[test]
fn test_deep_dive_query_returns_compact_list_when_too_many_matches() {
    let extra_files = [
        "src/a.rs", "src/b.rs", "src/c.rs", "src/d.rs", "src/e.rs", "src/f.rs",
    ];
    let files: Vec<(&str, &str)> = extra_files
        .iter()
        .map(|file| (*file, "pub fn extract() {}\n"))
        .collect();
    let (_dir, fixture) = fixture(&files);
    let snapshot = fixture.snapshot();

    let result = deep_dive_query(&snapshot, "extract", None, "overview", 10, 10).unwrap();

    assert!(
        result.contains("Found 6 definitions"),
        "Should report 6 definitions, got: {}",
        result
    );
    assert!(
        result.contains("context_file"),
        "Should suggest using context_file"
    );
    for file in &extra_files {
        assert!(
            result.contains(file),
            "Should list file path '{}' in compact output",
            file
        );
    }
    assert!(
        !result.contains("Callers"),
        "Should NOT build full context for 6+ matches"
    );
    assert!(
        !result.contains("Callees"),
        "Should NOT build full context for 6+ matches"
    );
}

#[test]
fn test_deep_dive_query_shows_full_context_at_threshold() {
    let all_files = [
        "src/engine.rs",
        "src/main.rs",
        "src/handler.rs",
        "src/a2.rs",
        "src/b2.rs",
    ];
    let files: Vec<(&str, &str)> = all_files.iter().map(|file| (*file, ENGINE)).collect();
    let (_dir, fixture) = fixture(&files);
    let snapshot = fixture.snapshot();

    let result = deep_dive_query(&snapshot, "process", None, "overview", 10, 10).unwrap();

    assert!(
        result.contains("Found 5 definitions"),
        "Should report 5 definitions, got: {}",
        result
    );
    assert!(
        result.contains("pub fn process()"),
        "Should include full context with signature at threshold of 5"
    );
}

#[test]
fn test_similar_symbols_skipped_when_no_embeddings() {
    let (_dir, fixture) = fixture(&[("src/engine.rs", "pub fn lonely_func() {}\n")]);
    let snapshot = fixture.snapshot();
    let lonely = only(&snapshot, "lonely_func");

    let ctx = build_symbol_context(&snapshot, lonely, "full", 10, 10).unwrap();
    assert!(
        ctx.similar.is_empty(),
        "Should be empty when no embeddings exist"
    );
}

#[test]
fn test_build_context_populates_test_refs_at_context_depth() {
    let (_dir, fixture) = fixture(&[
        ("src/engine.rs", ENGINE),
        (
            "src/tests/search_tests.rs",
            &at_line(40, TEST_CALLING_PROCESS),
        ),
    ]);
    let snapshot = fixture.snapshot();
    let target = only(&snapshot, "process");

    let ctx = build_symbol_context(&snapshot, target, "context", 10, 10).unwrap();
    assert_eq!(
        ctx.test_refs.len(),
        1,
        "context depth should populate test_refs"
    );
    assert_eq!(ctx.test_refs[0].file_path, "src/tests/search_tests.rs");

    let ctx_overview = build_symbol_context(&snapshot, target, "overview", 10, 10).unwrap();
    assert!(
        ctx_overview.test_refs.is_empty(),
        "overview should not populate test_refs"
    );
}

#[test]
fn test_build_context_uses_test_symbol_metadata_for_test_refs() {
    let (_dir, fixture) = fixture(&[
        ("src/engine.rs", ENGINE),
        (
            "integration/auth_flow.rs",
            &at_line(
                40,
                "#[test]\nfn auth_flow_succeeds() {\n    process();\n}\n",
            ),
        ),
    ]);
    let snapshot = fixture.snapshot();
    let target = only(&snapshot, "process");

    let ctx = build_symbol_context(&snapshot, target, "context", 10, 10).unwrap();
    assert_eq!(
        ctx.test_refs.len(),
        1,
        "test_refs should honor containing symbol metadata even when the file path lacks test markers"
    );
    assert_eq!(ctx.test_refs[0].file_path, "integration/auth_flow.rs");
    assert_eq!(
        ctx.test_refs[0].symbol.as_ref().unwrap().name,
        "auth_flow_succeeds"
    );
}

const FOO_HEADER: &str = "class Foo {\npublic:\n    Foo() {}\n    Foo(int a) {}\n    Foo(int a, int b) {}\n    Foo(double d) {}\n    Foo(char c) {}\n    Foo(long l) {}\n    Foo(float f) {}\n};\n";

#[test]
fn test_deep_dive_auto_selects_class_from_same_file_overloads() {
    let (_dir, fixture) = fixture(&[("include/foo.hpp", &at_line(77, FOO_HEADER))]);
    let snapshot = fixture.snapshot();
    assert!(
        find_symbol(snapshot.graph(), "Foo", None).len() > 5,
        "the header must define more than five symbols named Foo"
    );

    let result = deep_dive_query(&snapshot, "Foo", None, "overview", 10, 10).unwrap();

    assert!(
        result.contains("Auto-selected"),
        "Should contain auto-selection note, got:\n{}",
        result
    );
    assert!(
        result.contains("class Foo"),
        "Should show the class definition signature, got:\n{}",
        result
    );
    assert!(
        result.contains("include/foo.hpp:77"),
        "Should show class location, got:\n{}",
        result
    );
    assert!(
        !result.contains("Use context_file to disambiguate"),
        "Should NOT ask for disambiguation when auto-selecting, got:\n{}",
        result
    );
}

#[test]
fn test_deep_dive_still_disambiguates_when_results_span_multiple_files() {
    let files = [
        "src/engine.rs",
        "src/main.rs",
        "src/handler.rs",
        "lib/a.rs",
        "lib/b.rs",
        "lib/c.rs",
    ];
    let tree: Vec<(&str, &str)> = files
        .iter()
        .map(|file| (*file, "pub fn handle() {}\n"))
        .collect();
    let (_dir, fixture) = fixture(&tree);
    let snapshot = fixture.snapshot();

    let result = deep_dive_query(&snapshot, "handle", None, "overview", 10, 10).unwrap();

    assert!(
        result.contains("Use context_file to disambiguate"),
        "Should ask for disambiguation when results span multiple files, got:\n{}",
        result
    );
    assert!(
        !result.contains("Auto-selected"),
        "Should NOT auto-select when results span multiple files, got:\n{}",
        result
    );
}
