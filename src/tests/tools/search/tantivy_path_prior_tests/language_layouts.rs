use super::make_result;

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

    assert!(
        svc.score > 1.0,
        "C# source should be boosted, got {}",
        svc.score
    );
    assert!(
        test.score < 1.0,
        "C# .Tests path should be penalized, got {}",
        test.score
    );
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

    assert!(
        src.score > 1.0,
        "Python source should be boosted, got {}",
        src.score
    );
    assert!(
        test_dir.score < 1.0,
        "Python tests/ path should be penalized, got {}",
        test_dir.score
    );
    assert!(
        test_file.score < 1.0,
        "Python test_*.py should be penalized, got {}",
        test_file.score
    );
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

    assert!(
        main.score > 1.0,
        "Java main source should be boosted, got {}",
        main.score
    );
    assert!(
        test.score < 1.0,
        "Java test path should be penalized, got {}",
        test.score
    );
    assert!(
        main.score > test.score,
        "Java source should outrank Java test"
    );
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

    assert!(
        src.score > 1.0,
        "Go source should be boosted, got {}",
        src.score
    );
    assert!(
        test.score < 1.0,
        "Go _test.go should be penalized, got {}",
        test.score
    );
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

    assert!(
        src.score > 1.0,
        "JS/TS source should be boosted, got {}",
        src.score
    );
    assert!(
        jest.score < 1.0,
        "__tests__/ path should be penalized, got {}",
        jest.score
    );
    assert!(
        spec.score < 1.0,
        "*.spec.ts should be penalized, got {}",
        spec.score
    );
    assert!(
        test_file.score < 1.0,
        "*.test.js should be penalized, got {}",
        test_file.score
    );
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

    assert!(
        src.score > 1.0,
        "Ruby source should be boosted, got {}",
        src.score
    );
    assert!(
        spec.score < 1.0,
        "Ruby spec/ path should be penalized, got {}",
        spec.score
    );
    assert!(
        test.score < 1.0,
        "Ruby test/ path should be penalized, got {}",
        test.score
    );
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

    assert!(
        docs.score < 1.0,
        "docs/ should be penalized, got {}",
        docs.score
    );
    assert!(
        doc.score < 1.0,
        "doc/ should be penalized, got {}",
        doc.score
    );
    assert!(
        documentation.score < 1.0,
        "documentation/ should be penalized, got {}",
        documentation.score
    );
    assert!(src.score > docs.score, "source should outrank docs");
    assert!(src.score > doc.score, "source should outrank doc");
    assert!(
        src.score > documentation.score,
        "source should outrank documentation"
    );
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

    assert!(
        fixtures.score < 1.0,
        "fixtures/ should be penalized, got {}",
        fixtures.score
    );
    assert!(
        testdata.score < 1.0,
        "testdata/ should be penalized, got {}",
        testdata.score
    );
    assert!(
        test_data.score < 1.0,
        "test_data/ should be penalized, got {}",
        test_data.score
    );
    assert!(
        dunder_fixtures.score < 1.0,
        "__fixtures__/ should be penalized, got {}",
        dunder_fixtures.score
    );
    assert!(
        snapshots.score < 1.0,
        "snapshots/ should be penalized, got {}",
        snapshots.score
    );
    assert!(src.score > fixtures.score, "source should outrank fixtures");
    assert!(src.score > testdata.score, "source should outrank testdata");
}
