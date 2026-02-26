//! RED tests for NL-only path-prior scoring.

use crate::search::index::SymbolSearchResult;

fn make_result(id: &str, file_path: &str, score: f32) -> SymbolSearchResult {
    SymbolSearchResult {
        id: id.to_string(),
        name: format!("sym_{id}"),
        signature: String::new(),
        doc_comment: String::new(),
        file_path: file_path.to_string(),
        kind: "function".to_string(),
        language: "rust".to_string(),
        start_line: 1,
        score,
    }
}

#[test]
fn test_nl_like_query_applies_conservative_src_boost_and_non_code_penalties() {
    let mut results = vec![
        make_result("src", "src/tools/search/index.rs", 1.0),
        make_result("docs", "docs/SEARCH_FLOW.md", 1.0),
        make_result(
            "tests",
            "src/tests/tools/search/tantivy_integration_tests.rs",
            1.0,
        ),
        make_result("fixtures", "fixtures/real-world/sample.rs", 1.0),
    ];

    crate::search::scoring::apply_nl_path_prior(&mut results, "workspace routing");

    let src = results.iter().find(|r| r.id == "src").unwrap();
    let docs = results.iter().find(|r| r.id == "docs").unwrap();
    let tests = results.iter().find(|r| r.id == "tests").unwrap();
    let fixtures = results.iter().find(|r| r.id == "fixtures").unwrap();
    let src_mult = src.score;
    let docs_mult = docs.score;
    let tests_mult = tests.score;
    let fixtures_mult = fixtures.score;

    assert!(
        src_mult >= 1.03,
        "src/** boost should be meaningfully above no-op"
    );
    assert!(src_mult <= 1.20, "src/** boost should remain conservative");

    assert!(docs_mult <= 0.97, "docs/** should receive a real penalty");
    assert!(
        tests_mult <= 0.97,
        "src/tests/** should receive a real penalty"
    );
    assert!(
        fixtures_mult <= 0.97,
        "fixtures/** should receive a real penalty"
    );

    assert!(src.score > docs.score, "src/** should outrank docs/**");
    assert!(
        src.score > tests.score,
        "src/** should outrank src/tests/**"
    );
    assert!(
        src.score > fixtures.score,
        "src/** should outrank fixtures/**"
    );

    assert!(
        src_mult / docs_mult >= 1.08,
        "src/** should beat docs/** by a non-trivial margin"
    );
    assert!(
        src_mult / tests_mult >= 1.08,
        "src/** should beat src/tests/** by a non-trivial margin"
    );
    assert!(
        src_mult / fixtures_mult >= 1.08,
        "src/** should beat fixtures/** by a non-trivial margin"
    );
}

#[test]
fn test_identifier_query_does_not_apply_path_prior() {
    let mut results = vec![
        make_result("docs", "docs/SEARCH_FLOW.md", 2.0),
        make_result("src", "src/tools/search/index.rs", 1.0),
    ];

    let before = results
        .iter()
        .map(|r| (r.id.clone(), r.score))
        .collect::<Vec<_>>();

    crate::search::scoring::apply_nl_path_prior(&mut results, "get_reference_scores");

    let after = results
        .iter()
        .map(|r| (r.id.clone(), r.score))
        .collect::<Vec<_>>();

    assert_eq!(
        after, before,
        "identifier query should not trigger path prior"
    );
}

#[test]
fn test_mixed_nl_and_identifier_query_does_not_apply_path_prior() {
    let mut results = vec![
        make_result("docs", "docs/SEARCH_FLOW.md", 2.0),
        make_result("src", "src/tools/search/index.rs", 1.0),
    ];

    let before = results
        .iter()
        .map(|r| (r.id.clone(), r.score))
        .collect::<Vec<_>>();

    crate::search::scoring::apply_nl_path_prior(&mut results, "workspace get_reference_scores");

    let after = results
        .iter()
        .map(|r| (r.id.clone(), r.score))
        .collect::<Vec<_>>();

    assert_eq!(
        after, before,
        "mixed NL + identifier query should not trigger path prior"
    );
}

#[test]
fn test_single_word_query_is_no_op() {
    let mut results = vec![
        make_result("src", "src/tools/search/index.rs", 1.0),
        make_result("docs", "docs/SEARCH_FLOW.md", 1.0),
    ];

    let before = results
        .iter()
        .map(|r| (r.id.clone(), r.score))
        .collect::<Vec<_>>();

    crate::search::scoring::apply_nl_path_prior(&mut results, "workspace");

    let after = results
        .iter()
        .map(|r| (r.id.clone(), r.score))
        .collect::<Vec<_>>();

    assert_eq!(after, before, "single-word query should be a no-op");
}

#[test]
fn test_multi_word_nl_with_numeric_token_still_applies_path_prior() {
    let mut results = vec![
        make_result("src", "src/tools/auth/token_refresh.rs", 1.0),
        make_result("docs", "docs/OAUTH2.md", 1.0),
    ];

    crate::search::scoring::apply_nl_path_prior(&mut results, "oauth2 token refresh");

    let src = results.iter().find(|r| r.id == "src").unwrap();
    let docs = results.iter().find(|r| r.id == "docs").unwrap();

    assert!(src.score > 1.0, "multi-word NL should boost src/**");
    assert!(docs.score < 1.0, "multi-word NL should penalize docs/**");
    assert!(
        src.score / docs.score >= 1.08,
        "multi-word NL query should produce a non-trivial src/docs gap"
    );
}

#[test]
fn test_camel_case_identifier_query_does_not_apply_path_prior() {
    let mut results = vec![
        make_result("docs", "docs/SEARCH_FLOW.md", 2.0),
        make_result("src", "src/tools/search/index.rs", 1.0),
    ];

    let before = results
        .iter()
        .map(|r| (r.id.clone(), r.score))
        .collect::<Vec<_>>();

    crate::search::scoring::apply_nl_path_prior(&mut results, "getReferenceScores");

    let after = results
        .iter()
        .map(|r| (r.id.clone(), r.score))
        .collect::<Vec<_>>();

    assert_eq!(
        after, before,
        "camelCase identifier query should not trigger path prior"
    );
}

#[test]
fn test_empty_query_is_no_op_and_does_not_panic() {
    let mut results = vec![
        make_result("docs", "docs/SEARCH_FLOW.md", 2.0),
        make_result("src", "src/tools/search/index.rs", 1.0),
    ];

    let before = results
        .iter()
        .map(|r| (r.id.clone(), r.score))
        .collect::<Vec<_>>();

    crate::search::scoring::apply_nl_path_prior(&mut results, "");

    let after = results
        .iter()
        .map(|r| (r.id.clone(), r.score))
        .collect::<Vec<_>>();

    assert_eq!(after, before, "empty query should be a no-op");
}

#[test]
fn test_whitespace_only_query_is_no_op_and_does_not_panic() {
    let mut results = vec![
        make_result("docs", "docs/SEARCH_FLOW.md", 2.0),
        make_result("src", "src/tools/search/index.rs", 1.0),
    ];

    let before = results
        .iter()
        .map(|r| (r.id.clone(), r.score))
        .collect::<Vec<_>>();

    crate::search::scoring::apply_nl_path_prior(&mut results, "   \t\n  ");

    let after = results
        .iter()
        .map(|r| (r.id.clone(), r.score))
        .collect::<Vec<_>>();

    assert_eq!(after, before, "whitespace-only query should be a no-op");
}

// ──────────────────────────────────────────────────────────────────────────────
// Language-agnostic path classification tests
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_csharp_project_layout() {
    let mut results = vec![
        make_result("svc", "MyProject/Services/UserService.cs", 1.0),
        make_result("test", "MyProject.Tests/Services/UserServiceTests.cs", 1.0),
    ];

    crate::search::scoring::apply_nl_path_prior(&mut results, "user service");

    let svc = results.iter().find(|r| r.id == "svc").unwrap();
    let test = results.iter().find(|r| r.id == "test").unwrap();

    assert!(svc.score > 1.0, "C# source should be boosted, got {}", svc.score);
    assert!(test.score < 1.0, "C# .Tests path should be penalized, got {}", test.score);
    assert!(svc.score > test.score, "C# source should outrank C# test");
}

#[test]
fn test_python_project_layout() {
    let mut results = vec![
        make_result("src", "mypackage/auth.py", 1.0),
        make_result("test_dir", "tests/test_auth.py", 1.0),
        make_result("test_file", "mypackage/test_utils.py", 1.0),
    ];

    crate::search::scoring::apply_nl_path_prior(&mut results, "authentication logic");

    let src = results.iter().find(|r| r.id == "src").unwrap();
    let test_dir = results.iter().find(|r| r.id == "test_dir").unwrap();
    let test_file = results.iter().find(|r| r.id == "test_file").unwrap();

    assert!(src.score > 1.0, "Python source should be boosted, got {}", src.score);
    assert!(test_dir.score < 1.0, "Python tests/ path should be penalized, got {}", test_dir.score);
    assert!(test_file.score < 1.0, "Python test_*.py should be penalized, got {}", test_file.score);
}

#[test]
fn test_java_project_layout() {
    let mut results = vec![
        make_result("main", "src/main/java/com/example/Service.java", 1.0),
        make_result("test", "src/test/java/com/example/ServiceTest.java", 1.0),
    ];

    crate::search::scoring::apply_nl_path_prior(&mut results, "service implementation");

    let main = results.iter().find(|r| r.id == "main").unwrap();
    let test = results.iter().find(|r| r.id == "test").unwrap();

    assert!(main.score > 1.0, "Java main source should be boosted, got {}", main.score);
    assert!(test.score < 1.0, "Java test path should be penalized, got {}", test.score);
    assert!(main.score > test.score, "Java source should outrank Java test");
}

#[test]
fn test_go_project_layout() {
    let mut results = vec![
        make_result("src", "pkg/handler/auth.go", 1.0),
        make_result("test", "pkg/handler/auth_test.go", 1.0),
    ];

    crate::search::scoring::apply_nl_path_prior(&mut results, "authentication handler");

    let src = results.iter().find(|r| r.id == "src").unwrap();
    let test = results.iter().find(|r| r.id == "test").unwrap();

    assert!(src.score > 1.0, "Go source should be boosted, got {}", src.score);
    assert!(test.score < 1.0, "Go _test.go should be penalized, got {}", test.score);
    assert!(src.score > test.score, "Go source should outrank Go test");
}

#[test]
fn test_javascript_typescript_project_layout() {
    let mut results = vec![
        make_result("src", "src/components/Auth.tsx", 1.0),
        make_result("jest", "__tests__/Auth.test.tsx", 1.0),
        make_result("spec", "src/components/Auth.spec.ts", 1.0),
        make_result("test_file", "src/components/Auth.test.js", 1.0),
    ];

    crate::search::scoring::apply_nl_path_prior(&mut results, "authentication component");

    let src = results.iter().find(|r| r.id == "src").unwrap();
    let jest = results.iter().find(|r| r.id == "jest").unwrap();
    let spec = results.iter().find(|r| r.id == "spec").unwrap();
    let test_file = results.iter().find(|r| r.id == "test_file").unwrap();

    assert!(src.score > 1.0, "JS/TS source should be boosted, got {}", src.score);
    assert!(jest.score < 1.0, "__tests__/ path should be penalized, got {}", jest.score);
    assert!(spec.score < 1.0, "*.spec.ts should be penalized, got {}", spec.score);
    assert!(test_file.score < 1.0, "*.test.js should be penalized, got {}", test_file.score);
}

#[test]
fn test_ruby_project_layout() {
    let mut results = vec![
        make_result("src", "lib/auth.rb", 1.0),
        make_result("spec_dir", "spec/auth_spec.rb", 1.0),
        make_result("test_dir", "test/auth_test.rb", 1.0),
    ];

    crate::search::scoring::apply_nl_path_prior(&mut results, "authentication module");

    let src = results.iter().find(|r| r.id == "src").unwrap();
    let spec = results.iter().find(|r| r.id == "spec_dir").unwrap();
    let test = results.iter().find(|r| r.id == "test_dir").unwrap();

    assert!(src.score > 1.0, "Ruby source should be boosted, got {}", src.score);
    assert!(spec.score < 1.0, "Ruby spec/ path should be penalized, got {}", spec.score);
    assert!(test.score < 1.0, "Ruby test/ path should be penalized, got {}", test.score);
}

#[test]
fn test_generic_docs_penalization() {
    let mut results = vec![
        make_result("docs", "docs/architecture.md", 1.0),
        make_result("doc", "doc/api.md", 1.0),
        make_result("documentation", "documentation/guide.md", 1.0),
        make_result("src", "src/core/engine.rs", 1.0),
    ];

    crate::search::scoring::apply_nl_path_prior(&mut results, "engine architecture");

    let docs = results.iter().find(|r| r.id == "docs").unwrap();
    let doc = results.iter().find(|r| r.id == "doc").unwrap();
    let documentation = results.iter().find(|r| r.id == "documentation").unwrap();
    let src = results.iter().find(|r| r.id == "src").unwrap();

    assert!(docs.score < 1.0, "docs/ should be penalized, got {}", docs.score);
    assert!(doc.score < 1.0, "doc/ should be penalized, got {}", doc.score);
    assert!(documentation.score < 1.0, "documentation/ should be penalized, got {}", documentation.score);
    assert!(src.score > docs.score, "source should outrank docs");
    assert!(src.score > doc.score, "source should outrank doc");
    assert!(src.score > documentation.score, "source should outrank documentation");
}

#[test]
fn test_generic_fixtures_penalization() {
    let mut results = vec![
        make_result("fixtures", "fixtures/sample_data.json", 1.0),
        make_result("testdata", "testdata/input.json", 1.0),
        make_result("test_data", "test_data/expected.json", 1.0),
        make_result("__fixtures__", "__fixtures__/mock_response.json", 1.0),
        make_result("snapshots", "snapshots/__snapshots__/App.snap", 1.0),
        make_result("src", "src/parser/json_parser.rs", 1.0),
    ];

    crate::search::scoring::apply_nl_path_prior(&mut results, "json parsing");

    let fixtures = results.iter().find(|r| r.id == "fixtures").unwrap();
    let testdata = results.iter().find(|r| r.id == "testdata").unwrap();
    let test_data = results.iter().find(|r| r.id == "test_data").unwrap();
    let dunder_fixtures = results.iter().find(|r| r.id == "__fixtures__").unwrap();
    let snapshots = results.iter().find(|r| r.id == "snapshots").unwrap();
    let src = results.iter().find(|r| r.id == "src").unwrap();

    assert!(fixtures.score < 1.0, "fixtures/ should be penalized, got {}", fixtures.score);
    assert!(testdata.score < 1.0, "testdata/ should be penalized, got {}", testdata.score);
    assert!(test_data.score < 1.0, "test_data/ should be penalized, got {}", test_data.score);
    assert!(dunder_fixtures.score < 1.0, "__fixtures__/ should be penalized, got {}", dunder_fixtures.score);
    assert!(snapshots.score < 1.0, "snapshots/ should be penalized, got {}", snapshots.score);
    assert!(src.score > fixtures.score, "source should outrank fixtures");
    assert!(src.score > testdata.score, "source should outrank testdata");
}

// ──────────────────────────────────────────────────────────────────────────────
// Direct helper function tests
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_is_test_path_detects_various_layouts() {
    use crate::search::scoring::{is_test_path, is_docs_path, is_fixture_path};

    // Test directories
    assert!(is_test_path("tests/test_auth.py"), "tests/ directory");
    assert!(is_test_path("test/auth_test.rb"), "test/ directory");
    assert!(is_test_path("src/tests/integration.rs"), "src/tests/ directory");
    assert!(is_test_path("MyProject.Tests/UserServiceTests.cs"), ".Tests/ in C#");
    assert!(is_test_path("__tests__/Auth.test.tsx"), "__tests__/ jest directory");
    assert!(is_test_path("src/test/java/com/example/ServiceTest.java"), "src/test/ Java");
    assert!(is_test_path("spec/auth_spec.rb"), "spec/ Ruby directory");

    // Test file patterns (no test directory, but test file naming)
    assert!(is_test_path("pkg/handler/auth_test.go"), "Go _test.go file");
    assert!(is_test_path("src/components/Auth.test.tsx"), "*.test.tsx file");
    assert!(is_test_path("src/components/Auth.spec.ts"), "*.spec.ts file");
    assert!(is_test_path("src/components/Auth.test.js"), "*.test.js file");
    assert!(is_test_path("src/components/Auth.spec.js"), "*.spec.js file");
    assert!(is_test_path("mypackage/test_utils.py"), "Python test_*.py file");

    // Non-test paths
    assert!(!is_test_path("src/core/engine.rs"), "regular source file");
    assert!(!is_test_path("lib/auth.rb"), "regular lib file");
    assert!(!is_test_path("pkg/handler/auth.go"), "regular Go file");
    assert!(!is_test_path("src/components/Auth.tsx"), "regular component");
    assert!(!is_test_path("contest/results.py"), "contest should not match test");
    assert!(!is_test_path("src/testing_utils.rs"), "testing_utils is not a test file itself");
}

#[test]
fn test_is_docs_path_detects_various_layouts() {
    use crate::search::scoring::is_docs_path;

    assert!(is_docs_path("docs/architecture.md"), "docs/ directory");
    assert!(is_docs_path("doc/api.md"), "doc/ directory");
    assert!(is_docs_path("documentation/guide.md"), "documentation/ directory");
    assert!(is_docs_path("project/docs/setup.md"), "nested docs/ directory");

    assert!(!is_docs_path("src/core/document.rs"), "document in source is not docs");
    assert!(!is_docs_path("lib/doctor.rb"), "doctor is not docs");
}

#[test]
fn test_is_fixture_path_detects_various_layouts() {
    use crate::search::scoring::is_fixture_path;

    assert!(is_fixture_path("fixtures/sample.json"), "fixtures/ directory");
    assert!(is_fixture_path("fixture/data.json"), "fixture/ directory");
    assert!(is_fixture_path("testdata/input.json"), "testdata/ directory");
    assert!(is_fixture_path("test_data/expected.json"), "test_data/ directory");
    assert!(is_fixture_path("__fixtures__/mock.json"), "__fixtures__/ directory");
    assert!(is_fixture_path("snapshots/__snapshots__/App.snap"), "snapshots/ directory");

    assert!(!is_fixture_path("src/core/engine.rs"), "regular source file");
    assert!(!is_fixture_path("lib/data_loader.rb"), "data in source is not fixture");
}

// ──────────────────────────────────────────────────────────────────────────────
// Title-case / case-sensitivity tests
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_swift_project_layout() {
    let mut results = vec![
        make_result("src", "Sources/Auth/AuthService.swift", 1.0),
        make_result("test", "Tests/AuthTests/AuthServiceTests.swift", 1.0),
    ];

    crate::search::scoring::apply_nl_path_prior(&mut results, "authentication service");

    let src = results.iter().find(|r| r.id == "src").unwrap();
    let test = results.iter().find(|r| r.id == "test").unwrap();

    assert!(src.score > 1.0, "Swift source should be boosted, got {}", src.score);
    assert!(test.score < 1.0, "Swift Tests/ should be penalized, got {}", test.score);
    assert!(src.score > test.score, "Swift source should outrank Swift test");
}

#[test]
fn test_title_case_path_segments_detected() {
    use crate::search::scoring::{is_test_path, is_docs_path, is_fixture_path};

    // Title-case test directories (Swift, some C# projects)
    assert!(is_test_path("Tests/AuthTests/AuthServiceTests.swift"), "Swift Tests/");
    assert!(is_test_path("Test/SomeTest.cs"), "Title-case Test/");
    assert!(is_test_path("Spec/models/user_spec.rb"), "Title-case Spec/");

    // Title-case docs directories
    assert!(is_docs_path("Docs/architecture.md"), "Title-case Docs/");
    assert!(is_docs_path("Doc/api.md"), "Title-case Doc/");
    assert!(is_docs_path("Documentation/guide.md"), "Title-case Documentation/");

    // Title-case fixture directories
    assert!(is_fixture_path("Fixtures/sample.json"), "Title-case Fixtures/");
    assert!(is_fixture_path("Fixture/data.json"), "Title-case Fixture/");
    assert!(is_fixture_path("Snapshots/App.snap"), "Title-case Snapshots/");
}

// ──────────────────────────────────────────────────────────────────────────────
// Original tests
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_nl_path_prior_is_deterministic_for_same_inputs() {
    let baseline = vec![
        make_result("a", "src/core/workspace_router.rs", 1.0),
        make_result("b", "docs/workspace-routing.md", 1.0),
        make_result("c", "src/tests/tools/search/quality.rs", 1.0),
        make_result("d", "fixtures/real-world/router.rs", 1.0),
    ];

    let mut run_one = baseline.clone();
    let mut run_two = baseline.clone();

    crate::search::scoring::apply_nl_path_prior(&mut run_one, "workspace routing");
    crate::search::scoring::apply_nl_path_prior(&mut run_two, "workspace routing");

    let one = run_one
        .iter()
        .map(|r| (r.id.clone(), r.score))
        .collect::<Vec<_>>();
    let two = run_two
        .iter()
        .map(|r| (r.id.clone(), r.score))
        .collect::<Vec<_>>();

    assert_eq!(
        one, two,
        "same inputs should produce identical ranking and scores"
    );
    assert!(
        run_one.windows(2).all(|w| w[0].score >= w[1].score),
        "results should remain sorted by descending score after prior"
    );
}
