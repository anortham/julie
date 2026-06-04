use super::make_result;

// ──────────────────────────────────────────────────────────────────────────────
// Direct helper function tests
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_is_test_path_detects_various_layouts() {
    use julie_index::search::scoring::is_test_path;

    // Test directories
    assert!(is_test_path("tests/test_auth.py"), "tests/ directory");
    assert!(is_test_path("test/auth_test.rb"), "test/ directory");
    assert!(
        is_test_path("src/tests/integration.rs"),
        "src/tests/ directory"
    );
    assert!(
        is_test_path("MyProject.Tests/UserServiceTests.cs"),
        ".Tests/ in C#"
    );
    assert!(
        is_test_path("__tests__/Auth.test.tsx"),
        "__tests__/ jest directory"
    );
    assert!(
        is_test_path("src/test/java/com/example/ServiceTest.java"),
        "src/test/ Java"
    );
    assert!(is_test_path("spec/auth_spec.rb"), "spec/ Ruby directory");

    // Test file patterns (no test directory, but test file naming)
    assert!(is_test_path("pkg/handler/auth_test.go"), "Go _test.go file");
    assert!(
        is_test_path("src/components/Auth.test.tsx"),
        "*.test.tsx file"
    );
    assert!(
        is_test_path("src/components/Auth.spec.ts"),
        "*.spec.ts file"
    );
    assert!(
        is_test_path("src/components/Auth.test.js"),
        "*.test.js file"
    );
    assert!(
        is_test_path("src/components/Auth.spec.js"),
        "*.spec.js file"
    );
    assert!(
        is_test_path("mypackage/test_utils.py"),
        "Python test_*.py file"
    );

    // Non-test paths
    assert!(!is_test_path("src/core/engine.rs"), "regular source file");
    assert!(!is_test_path("lib/auth.rb"), "regular lib file");
    assert!(!is_test_path("pkg/handler/auth.go"), "regular Go file");
    assert!(
        !is_test_path("src/components/Auth.tsx"),
        "regular component"
    );
    assert!(
        !is_test_path("contest/results.py"),
        "contest should not match test"
    );
    assert!(
        !is_test_path("src/testing_utils.rs"),
        "testing_utils is not a test file itself"
    );
}

#[test]
fn test_is_docs_path_detects_various_layouts() {
    use julie_index::search::scoring::is_docs_path;

    assert!(is_docs_path("docs/architecture.md"), "docs/ directory");
    assert!(is_docs_path("doc/api.md"), "doc/ directory");
    assert!(
        is_docs_path("documentation/guide.md"),
        "documentation/ directory"
    );
    assert!(
        is_docs_path("project/docs/setup.md"),
        "nested docs/ directory"
    );

    assert!(
        !is_docs_path("src/core/document.rs"),
        "document in source is not docs"
    );
    assert!(!is_docs_path("lib/doctor.rb"), "doctor is not docs");
}

#[test]
fn test_is_fixture_path_detects_various_layouts() {
    use julie_index::search::scoring::is_fixture_path;

    assert!(
        is_fixture_path("fixtures/sample.json"),
        "fixtures/ directory"
    );
    assert!(is_fixture_path("fixture/data.json"), "fixture/ directory");
    assert!(
        is_fixture_path("testdata/input.json"),
        "testdata/ directory"
    );
    assert!(
        is_fixture_path("test_data/expected.json"),
        "test_data/ directory"
    );
    assert!(
        is_fixture_path("__fixtures__/mock.json"),
        "__fixtures__/ directory"
    );
    assert!(
        is_fixture_path("snapshots/__snapshots__/App.snap"),
        "snapshots/ directory"
    );

    assert!(
        !is_fixture_path("src/core/engine.rs"),
        "regular source file"
    );
    assert!(
        !is_fixture_path("lib/data_loader.rb"),
        "data in source is not fixture"
    );
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

    julie_index::search::scoring::apply_nl_path_prior(&mut results, "authentication service");

    let src = results.iter().find(|r| r.id == "src").unwrap();
    let test = results.iter().find(|r| r.id == "test").unwrap();

    assert!(
        src.score > 1.0,
        "Swift source should be boosted, got {}",
        src.score
    );
    assert!(
        test.score < 1.0,
        "Swift Tests/ should be penalized, got {}",
        test.score
    );
    assert!(
        src.score > test.score,
        "Swift source should outrank Swift test"
    );
}

#[test]
fn test_title_case_path_segments_detected() {
    use julie_index::search::scoring::{is_docs_path, is_fixture_path, is_test_path};

    // Title-case test directories (Swift, some C# projects)
    assert!(
        is_test_path("Tests/AuthTests/AuthServiceTests.swift"),
        "Swift Tests/"
    );
    assert!(is_test_path("Test/SomeTest.cs"), "Title-case Test/");
    assert!(is_test_path("Spec/models/user_spec.rb"), "Title-case Spec/");

    // Title-case docs directories
    assert!(is_docs_path("Docs/architecture.md"), "Title-case Docs/");
    assert!(is_docs_path("Doc/api.md"), "Title-case Doc/");
    assert!(
        is_docs_path("Documentation/guide.md"),
        "Title-case Documentation/"
    );

    // Title-case fixture directories
    assert!(
        is_fixture_path("Fixtures/sample.json"),
        "Title-case Fixtures/"
    );
    assert!(is_fixture_path("Fixture/data.json"), "Title-case Fixture/");
    assert!(
        is_fixture_path("Snapshots/App.snap"),
        "Title-case Snapshots/"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Benchmark path and fixture penalty strength tests
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn test_is_fixture_path_matches_benchmarks() {
    use julie_index::search::scoring::is_fixture_path;

    assert!(
        is_fixture_path("fixtures/benchmarks/queries.jsonl"),
        "fixtures/benchmarks/ should match (benchmarks segment)"
    );
    assert!(
        is_fixture_path("benchmarks/perf_data.json"),
        "benchmarks/ at root should match"
    );
    assert!(
        is_fixture_path("src/benchmarks/load_test.rs"),
        "src/benchmarks/ should match"
    );
    assert!(
        is_fixture_path("Benchmarks/data.csv"),
        "Title-case Benchmarks/ should match"
    );
    assert!(
        is_fixture_path("benchmark/suite.rs"),
        "singular benchmark/ should match"
    );
    // Existing patterns still work
    assert!(
        is_fixture_path("fixtures/test_data.json"),
        "fixtures/ still works"
    );
    assert!(
        is_fixture_path("__fixtures__/mock.json"),
        "__fixtures__/ still works"
    );
}
