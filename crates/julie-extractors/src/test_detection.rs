//! Test symbol detection for all 31 supported languages.
//!
//! Provides [`is_test_symbol`] — a pure, data-driven function that determines whether
//! a symbol is a test based on its language, name, file path, kind, decorators/attributes,
//! and doc comment. No tree-sitter, no file I/O.

use crate::base::SymbolKind;

/// Callable symbol kinds — only these can be actual test functions/methods.
fn is_callable(kind: &SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Function | SymbolKind::Method | SymbolKind::Constructor
    )
}

/// Check whether `file_path` looks like it lives in a test directory or is a test file.
///
/// Language-agnostic: works for Rust, Python, Java, C#, Go, JS/TS, Ruby, Swift, etc.
fn is_test_path(file_path: &str) -> bool {
    // Segment-level checks (directory names)
    for segment in file_path.split('/') {
        match segment {
            "test" | "tests" | "Test" | "Tests" | "spec" | "Spec" | "__tests__" | "autotests" => {
                return true;
            }
            _ => {}
        }
        // C# convention: MyProject.Tests/
        if segment.ends_with(".Tests") || segment.ends_with(".Test") {
            return true;
        }
    }

    // File-name patterns
    let file_name = file_path.rsplit('/').next().unwrap_or(file_path);
    if file_name.ends_with("_test.go")
        || file_name.contains(".test.")
        || file_name.contains(".spec.")
        || file_name.starts_with("test_")
        || file_name.starts_with("tst_")
    {
        return true;
    }

    false
}

/// Determine if a symbol is a test symbol.
///
/// Two-tier approach:
/// 1. **Language-specific**: check attributes, decorators, annotations, doc comments, and
///    language-idiomatic naming conventions.
/// 2. **Generic fallback**: for the ~20 languages without specific test framework conventions,
///    check if the function name starts with `test_` or `Test` AND the file is in a test path.
///
/// Only callable symbols (Function, Method, Constructor) can be tests. Classes, structs,
/// interfaces, etc. return `false` — they are containers, not tests.
///
/// `doc_comment` is currently only used for PHP's `@test` annotation pattern.
pub fn is_test_symbol(
    language: &str,
    name: &str,
    file_path: &str,
    kind: &SymbolKind,
    decorators: &[String],
    attributes: &[String],
    doc_comment: Option<&str>,
) -> bool {
    // Gate: only callable symbols can be tests
    if !is_callable(kind) {
        return false;
    }

    match language {
        "rust" => detect_rust(attributes),
        "python" => detect_python(name, decorators),
        "java" | "kotlin" => detect_java_kotlin(decorators, attributes),
        "scala" => detect_scala(name, file_path, decorators, attributes),
        "elixir" => detect_elixir(name, file_path),
        "csharp" | "vbnet" | "razor" => detect_csharp(attributes),
        "go" => detect_go(name, file_path),
        "javascript" | "typescript" => detect_js_ts(name, file_path),
        "php" => detect_php(name, file_path, doc_comment),
        "ruby" => detect_ruby(name, file_path),
        "swift" => detect_swift(name, file_path),
        "dart" => detect_dart(name, file_path, decorators),
        _ => detect_generic(name, file_path),
    }
}

// ---------------------------------------------------------------------------
// Language-specific detectors
// ---------------------------------------------------------------------------

fn detect_rust(attributes: &[String]) -> bool {
    attributes
        .iter()
        .any(|a| a == "test" || a == "tokio::test" || a == "rstest")
}

fn detect_python(name: &str, decorators: &[String]) -> bool {
    // Decorator-based: pytest.* or unittest.*
    if decorators
        .iter()
        .any(|d| d.starts_with("pytest") || d.starts_with("unittest"))
    {
        return true;
    }
    // unittest lifecycle methods (setUp/tearDown and class-level variants)
    if matches!(name, "setUp" | "tearDown" | "setUpClass" | "tearDownClass") {
        return true;
    }
    // Name-based: test_ prefix for functions/methods (class filter already handled by kind gate)
    name.starts_with("test_")
}

fn detect_scala(name: &str, file_path: &str, decorators: &[String], attributes: &[String]) -> bool {
    // JUnit-style: @Test annotation (used by some Scala projects)
    if detect_java_kotlin(decorators, attributes) {
        return true;
    }
    // ScalaTest/MUnit/Specs2: tests are methods in test files —
    // no @Test annotation, but the file lives in a test directory.
    // Also catch common ScalaTest lifecycle methods.
    if is_test_path(file_path) {
        return true;
    }
    // Name-based: test prefix (MUnit convention)
    name.starts_with("test")
}

fn detect_java_kotlin(decorators: &[String], attributes: &[String]) -> bool {
    let test_annotations = [
        "Test",
        "ParameterizedTest",
        "RepeatedTest",
        "BeforeEach",
        "AfterEach",
        "BeforeAll",
        "AfterAll",
        "Before",
        "After",
        "BeforeClass",
        "AfterClass",
    ];
    decorators
        .iter()
        .chain(attributes.iter())
        .any(|a| test_annotations.contains(&a.as_str()))
}

fn detect_csharp(attributes: &[String]) -> bool {
    let test_attrs = [
        "Test",
        "TestMethod",
        "Fact",
        "Theory",
        "SetUp",
        "TearDown",
        "OneTimeSetUp",
        "OneTimeTearDown",
        "TestInitialize",
        "TestCleanup",
        "ClassInitialize",
        "ClassCleanup",
    ];
    attributes.iter().any(|a| {
        // C# extractors may produce bracketed attributes like "[Fact]" or bare "Fact".
        // Strip surrounding brackets before matching.
        let stripped = a.strip_prefix('[').or_else(|| a.strip_prefix('<')).unwrap_or(a);
        let stripped = stripped.strip_suffix(']').or_else(|| stripped.strip_suffix('>')).unwrap_or(stripped);
        test_attrs.contains(&stripped)
    })
}

fn detect_go(name: &str, file_path: &str) -> bool {
    // Go tests require BOTH: recognized prefix AND _test.go file suffix
    let file_name = file_path.rsplit('/').next().unwrap_or(file_path);
    (name.starts_with("Test") || name.starts_with("Fuzz") || name.starts_with("Example"))
        && file_name.ends_with("_test.go")
}

/// Known limitation: in Jest/Mocha, `test()`/`describe()` are call expressions, not named
/// function definitions. Symbol-level detection will mostly catch path-based heuristics.
/// The name check is a secondary signal.
fn detect_js_ts(name: &str, file_path: &str) -> bool {
    // Must be a test runner function AND in a test/spec file
    let is_test_fn = matches!(name, "describe" | "it" | "test");
    let file_name = file_path.rsplit('/').next().unwrap_or(file_path);
    let in_test_file =
        file_name.contains(".test.") || file_name.contains(".spec.") || is_test_path(file_path);
    is_test_fn && in_test_file
}

fn detect_php(name: &str, file_path: &str, doc_comment: Option<&str>) -> bool {
    // @test annotation in doc comment — genuine test marker regardless of path
    if let Some(doc) = doc_comment {
        if doc.contains("@test") {
            return true;
        }
    }
    // Name prefix — requires test path to avoid false positives on production code
    // (e.g. testConnection() in a service class)
    name.starts_with("test") && is_test_path(file_path)
}

fn detect_ruby(name: &str, file_path: &str) -> bool {
    // test_ prefix AND in spec/ or test/ directory
    name.starts_with("test_") && is_test_path(file_path)
}

fn detect_swift(name: &str, file_path: &str) -> bool {
    // XCTest convention: test* prefix + lifecycle methods — all require test path
    // to avoid false positives on production code with similarly-named methods
    is_test_path(file_path)
        && (name.starts_with("test")
            || matches!(
                name,
                "setUp" | "tearDown" | "setUpWithError" | "tearDownWithError"
            ))
}

fn detect_elixir(name: &str, file_path: &str) -> bool {
    // ExUnit convention: test_ prefix or test/ directory
    name.starts_with("test_") || name.starts_with("test ") || is_test_path(file_path)
}

fn detect_dart(name: &str, file_path: &str, decorators: &[String]) -> bool {
    // isTest decorator — definitive, no path guard needed
    if decorators.iter().any(|d| d.contains("isTest")) {
        return true;
    }
    // Name prefix — requires test path to avoid false positives on production Dart functions
    name.starts_with("test") && is_test_path(file_path)
}

// ---------------------------------------------------------------------------
// Generic fallback — for the ~20 languages without specific frameworks
// ---------------------------------------------------------------------------

fn detect_generic(name: &str, file_path: &str) -> bool {
    let has_test_name = name.starts_with("test_") || name.starts_with("Test");
    has_test_name && is_test_path(file_path)
}
