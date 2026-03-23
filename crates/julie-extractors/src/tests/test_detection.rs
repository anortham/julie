// Tests for test_detection module — is_test_symbol() function
//
// Covers all 33 languages via language-specific rules + generic fallback.

use crate::base::SymbolKind;
use crate::test_detection::is_test_symbol;

// ---------------------------------------------------------------------------
// Helper: shorthand for common no-decorators/no-attributes/no-doc calls
// ---------------------------------------------------------------------------

fn check(
    language: &str,
    name: &str,
    file_path: &str,
    kind: &SymbolKind,
    decorators: &[String],
    attributes: &[String],
    doc_comment: Option<&str>,
) -> bool {
    is_test_symbol(
        language,
        name,
        file_path,
        kind,
        decorators,
        attributes,
        doc_comment,
    )
}

fn s(val: &str) -> String {
    val.to_string()
}

// ===========================================================================
// Rust
// ===========================================================================

#[test]
fn rust_test_attribute() {
    assert!(check(
        "rust",
        "test_add",
        "src/tests/math.rs",
        &SymbolKind::Function,
        &[],
        &[s("test")],
        None,
    ));
}

#[test]
fn rust_tokio_test_attribute() {
    assert!(check(
        "rust",
        "test_async_fetch",
        "src/lib.rs",
        &SymbolKind::Function,
        &[],
        &[s("tokio::test")],
        None,
    ));
}

#[test]
fn rust_rstest_attribute() {
    assert!(check(
        "rust",
        "my_parameterized",
        "src/lib.rs",
        &SymbolKind::Function,
        &[],
        &[s("rstest")],
        None,
    ));
}

#[test]
fn rust_no_test_attr() {
    assert!(!check(
        "rust",
        "process_data",
        "src/lib.rs",
        &SymbolKind::Function,
        &[],
        &[s("inline")],
        None,
    ));
}

// ===========================================================================
// Python
// ===========================================================================

#[test]
fn python_pytest_decorator() {
    assert!(check(
        "python",
        "test_payment",
        "tests/test_payment.py",
        &SymbolKind::Function,
        &[s("pytest.mark.parametrize")],
        &[],
        None,
    ));
}

#[test]
fn python_unittest_decorator() {
    assert!(check(
        "python",
        "test_thing",
        "tests/test_thing.py",
        &SymbolKind::Method,
        &[s("unittest.skip")],
        &[],
        None,
    ));
}

#[test]
fn python_test_prefix_function() {
    assert!(check(
        "python",
        "test_login",
        "tests/test_auth.py",
        &SymbolKind::Function,
        &[],
        &[],
        None,
    ));
}

#[test]
fn python_test_class_returns_false() {
    // Classes are containers, not tests themselves
    assert!(!check(
        "python",
        "TestPaymentProcessor",
        "tests/test_payment.py",
        &SymbolKind::Class,
        &[],
        &[],
        None,
    ));
}

#[test]
fn python_regular_function() {
    assert!(!check(
        "python",
        "process_payment",
        "src/payment.py",
        &SymbolKind::Function,
        &[],
        &[],
        None,
    ));
}

// ===========================================================================
// Java / Kotlin
// ===========================================================================

#[test]
fn java_test_annotation() {
    assert!(check(
        "java",
        "shouldProcessPayment",
        "src/test/java/PaymentTest.java",
        &SymbolKind::Method,
        &[s("Test")],
        &[],
        None,
    ));
}

#[test]
fn java_parameterized_test() {
    assert!(check(
        "java",
        "testWithParams",
        "src/test/java/PaymentTest.java",
        &SymbolKind::Method,
        &[s("ParameterizedTest")],
        &[],
        None,
    ));
}

#[test]
fn java_repeated_test() {
    assert!(check(
        "java",
        "testRepeated",
        "src/test/java/PaymentTest.java",
        &SymbolKind::Method,
        &[s("RepeatedTest")],
        &[],
        None,
    ));
}

#[test]
fn java_regular_method() {
    assert!(!check(
        "java",
        "processPayment",
        "src/main/java/Payment.java",
        &SymbolKind::Method,
        &[s("Override")],
        &[],
        None,
    ));
}

#[test]
fn kotlin_test_annotation() {
    assert!(check(
        "kotlin",
        "shouldReturnUser",
        "src/test/kotlin/UserTest.kt",
        &SymbolKind::Method,
        &[s("Test")],
        &[],
        None,
    ));
}

// ===========================================================================
// C#
// ===========================================================================

#[test]
fn csharp_fact_attribute() {
    assert!(check(
        "csharp",
        "ShouldProcessOrder",
        "MyProject.Tests/OrderTests.cs",
        &SymbolKind::Method,
        &[],
        &[s("Fact")],
        None,
    ));
}

#[test]
fn csharp_theory_attribute() {
    assert!(check(
        "csharp",
        "ShouldCalculateTotal",
        "MyProject.Tests/OrderTests.cs",
        &SymbolKind::Method,
        &[],
        &[s("Theory")],
        None,
    ));
}

#[test]
fn csharp_test_attribute() {
    assert!(check(
        "csharp",
        "TestOrder",
        "MyProject.Tests/OrderTests.cs",
        &SymbolKind::Method,
        &[],
        &[s("Test")],
        None,
    ));
}

#[test]
fn csharp_test_method_attribute() {
    assert!(check(
        "csharp",
        "TestMethod1",
        "MyProject.Tests/OrderTests.cs",
        &SymbolKind::Method,
        &[],
        &[s("TestMethod")],
        None,
    ));
}

#[test]
fn csharp_bracketed_fact_attribute() {
    // C# extractors produce bracketed text like "[Fact]" — must still match
    assert!(check(
        "csharp",
        "ShouldValidateInput",
        "MyProject.Tests/ValidationTests.cs",
        &SymbolKind::Method,
        &[],
        &[s("[Fact]")],
        None,
    ));
}

#[test]
fn csharp_bracketed_theory_attribute() {
    assert!(check(
        "csharp",
        "ShouldCalculateDiscount",
        "MyProject.Tests/PricingTests.cs",
        &SymbolKind::Method,
        &[],
        &[s("[Theory]")],
        None,
    ));
}

// ===========================================================================
// Razor (routes to C# detection)
// ===========================================================================

#[test]
fn razor_routes_to_csharp_fact_attribute() {
    // Razor files with C# attributes should route through detect_csharp
    assert!(check(
        "razor",
        "ShouldRenderComponent",
        "MyProject.Tests/Components/ButtonTests.cshtml",
        &SymbolKind::Method,
        &[],
        &[s("[Fact]")],
        None,
    ));
}

#[test]
fn razor_routes_to_csharp_test_attribute() {
    assert!(check(
        "razor",
        "TestRender",
        "MyProject.Tests/Views/IndexTests.cshtml",
        &SymbolKind::Method,
        &[],
        &[s("Test")],
        None,
    ));
}

#[test]
fn razor_no_test_attribute_returns_false() {
    // Razor method without test attributes should not be flagged
    assert!(!check(
        "razor",
        "OnGet",
        "MyProject/Pages/Index.cshtml",
        &SymbolKind::Method,
        &[],
        &[s("[HttpGet]")],
        None,
    ));
}

// ===========================================================================
// Go
// ===========================================================================

#[test]
fn go_test_function_in_test_file() {
    assert!(check(
        "go",
        "TestProcessPayment",
        "payment/payment_test.go",
        &SymbolKind::Function,
        &[],
        &[],
        None,
    ));
}

#[test]
fn go_test_name_not_in_test_file() {
    // Go requires BOTH the Test prefix AND the _test.go file
    assert!(!check(
        "go",
        "TestHelper",
        "payment/helpers.go",
        &SymbolKind::Function,
        &[],
        &[],
        None,
    ));
}

#[test]
fn go_benchmark_not_test() {
    // BenchmarkX isn't a test function for our purposes
    assert!(!check(
        "go",
        "BenchmarkProcess",
        "payment/payment_test.go",
        &SymbolKind::Function,
        &[],
        &[],
        None,
    ));
}

#[test]
fn go_fuzz_function_in_test_file() {
    assert!(check(
        "go",
        "FuzzParseInput",
        "parser/parser_test.go",
        &SymbolKind::Function,
        &[],
        &[],
        None,
    ));
}

#[test]
fn go_example_function_in_test_file() {
    assert!(check(
        "go",
        "ExampleProcessPayment",
        "payment/payment_test.go",
        &SymbolKind::Function,
        &[],
        &[],
        None,
    ));
}

// ===========================================================================
// JavaScript / TypeScript
// ===========================================================================

#[test]
fn js_test_in_test_file() {
    assert!(check(
        "javascript",
        "test",
        "src/__tests__/payment.test.js",
        &SymbolKind::Function,
        &[],
        &[],
        None,
    ));
}

#[test]
fn ts_describe_in_spec_file() {
    assert!(check(
        "typescript",
        "describe",
        "src/payment.spec.ts",
        &SymbolKind::Function,
        &[],
        &[],
        None,
    ));
}

#[test]
fn js_it_in_test_file() {
    assert!(check(
        "javascript",
        "it",
        "tests/payment.test.js",
        &SymbolKind::Function,
        &[],
        &[],
        None,
    ));
}

#[test]
fn ts_test_function_not_in_test_file() {
    // "test" function in production code is NOT a test
    assert!(!check(
        "typescript",
        "test",
        "src/utils.ts",
        &SymbolKind::Function,
        &[],
        &[],
        None,
    ));
}

// ===========================================================================
// PHP
// ===========================================================================

#[test]
fn php_test_in_doc_comment() {
    assert!(check(
        "php",
        "itShouldProcess",
        "tests/PaymentTest.php",
        &SymbolKind::Method,
        &[],
        &[],
        Some("/** @test */"),
    ));
}

#[test]
fn php_test_prefix() {
    assert!(check(
        "php",
        "testProcessPayment",
        "tests/PaymentTest.php",
        &SymbolKind::Method,
        &[],
        &[],
        None,
    ));
}

// ===========================================================================
// Ruby
// ===========================================================================

#[test]
fn ruby_test_prefix_in_spec_dir() {
    assert!(check(
        "ruby",
        "test_process_payment",
        "spec/payment_spec.rb",
        &SymbolKind::Method,
        &[],
        &[],
        None,
    ));
}

#[test]
fn ruby_test_prefix_in_test_dir() {
    assert!(check(
        "ruby",
        "test_login",
        "test/auth_test.rb",
        &SymbolKind::Method,
        &[],
        &[],
        None,
    ));
}

// ===========================================================================
// Swift
// ===========================================================================

#[test]
fn swift_test_prefix_method() {
    assert!(check(
        "swift",
        "testLogin",
        "Tests/AuthTests.swift",
        &SymbolKind::Method,
        &[],
        &[],
        None,
    ));
}

#[test]
fn swift_test_prefix_function() {
    assert!(check(
        "swift",
        "testNetworkCall",
        "Tests/NetworkTests.swift",
        &SymbolKind::Function,
        &[],
        &[],
        None,
    ));
}

#[test]
fn swift_class_with_test_prefix_returns_false() {
    // Classes aren't callable
    assert!(!check(
        "swift",
        "TestHelper",
        "Tests/Helpers.swift",
        &SymbolKind::Class,
        &[],
        &[],
        None,
    ));
}

// ===========================================================================
// Dart
// ===========================================================================

#[test]
fn dart_is_test_decorator() {
    assert!(check(
        "dart",
        "myTest",
        "test/widget_test.dart",
        &SymbolKind::Function,
        &[s("isTest")],
        &[],
        None,
    ));
}

#[test]
fn dart_test_prefix() {
    assert!(check(
        "dart",
        "testWidgetRendering",
        "test/widget_test.dart",
        &SymbolKind::Function,
        &[],
        &[],
        None,
    ));
}

#[test]
fn dart_test_prefix_in_production_code_returns_false() {
    // testWidgetRendering in lib/widgets.dart is NOT a test — path guard prevents false positive
    assert!(!check(
        "dart",
        "testWidgetRendering",
        "lib/widgets.dart",
        &SymbolKind::Function,
        &[],
        &[],
        None,
    ));
}

// ===========================================================================
// Generic fallback (covers remaining ~20 languages)
// ===========================================================================

#[test]
fn generic_test_underscore_prefix_in_test_path() {
    assert!(check(
        "lua",
        "test_something",
        "tests/test_util.lua",
        &SymbolKind::Function,
        &[],
        &[],
        None,
    ));
}

#[test]
fn generic_test_capital_prefix_in_test_path() {
    assert!(check(
        "zig",
        "TestAllocator",
        "tests/allocator_test.zig",
        &SymbolKind::Function,
        &[],
        &[],
        None,
    ));
}

// ===========================================================================
// Test lifecycle methods (setUp, tearDown, etc.)
// ===========================================================================

#[test]
fn csharp_setup_is_test() {
    assert!(check(
        "csharp",
        "SetUp",
        "Tests/MyTests.cs",
        &SymbolKind::Method,
        &[],
        &[s("SetUp")],
        None
    ));
}

#[test]
fn csharp_teardown_is_test() {
    assert!(check(
        "csharp",
        "TearDown",
        "Tests/MyTests.cs",
        &SymbolKind::Method,
        &[],
        &[s("TearDown")],
        None
    ));
}

#[test]
fn csharp_onetime_setup_is_test() {
    assert!(check(
        "csharp",
        "Initialize",
        "Tests/MyTests.cs",
        &SymbolKind::Method,
        &[],
        &[s("OneTimeSetUp")],
        None
    ));
}

#[test]
fn java_before_each_is_test() {
    assert!(check(
        "java",
        "setup",
        "src/test/MyTest.java",
        &SymbolKind::Method,
        &[s("BeforeEach")],
        &[],
        None
    ));
}

#[test]
fn python_setup_is_test() {
    assert!(check(
        "python",
        "setUp",
        "tests/test_foo.py",
        &SymbolKind::Method,
        &[],
        &[],
        None
    ));
}

#[test]
fn swift_setup_is_test() {
    assert!(check(
        "swift",
        "setUp",
        "Tests/MyTests.swift",
        &SymbolKind::Method,
        &[],
        &[],
        None
    ));
}

// ===========================================================================
// PHP — path guard for name-prefix detection
// ===========================================================================

#[test]
fn php_test_prefix_in_production_code_returns_false() {
    // testConnection() in a production PHP service should NOT be flagged as a test
    assert!(!check(
        "php",
        "testConnection",
        "src/services/database.php",
        &SymbolKind::Method,
        &[],
        &[],
        None,
    ));
}

#[test]
fn php_test_annotation_in_production_code_returns_true() {
    // @test doc annotation is a genuine test marker regardless of file path
    assert!(check(
        "php",
        "someMethod",
        "src/services/database.php",
        &SymbolKind::Method,
        &[],
        &[],
        Some("/** @test */"),
    ));
}

// ===========================================================================
// Swift — path guard for name-prefix detection
// ===========================================================================

#[test]
fn swift_test_prefix_in_production_code_returns_false() {
    // testConnection() in a production Swift file should NOT be flagged as a test
    assert!(!check(
        "swift",
        "testConnection",
        "Sources/App/Database.swift",
        &SymbolKind::Method,
        &[],
        &[],
        None,
    ));
}

#[test]
fn swift_setup_in_production_code_returns_false() {
    // setUp() outside a test directory is NOT a test lifecycle method
    assert!(!check(
        "swift",
        "setUp",
        "Sources/App/Database.swift",
        &SymbolKind::Method,
        &[],
        &[],
        None,
    ));
}

// ===========================================================================
// False positive prevention
// ===========================================================================

#[test]
fn false_positive_production_function_with_test_in_name() {
    // A production utility function that happens to have "test" in its name
    // should NOT be flagged as a test if it's not in a test path
    assert!(!check(
        "rust",
        "test_connection_pool",
        "src/database/pool.rs",
        &SymbolKind::Function,
        &[],
        &[],
        None,
    ));
}

#[test]
fn false_positive_test_helper_in_prod_code() {
    assert!(!check(
        "python",
        "create_test_user",
        "src/factories.py",
        &SymbolKind::Function,
        &[],
        &[],
        None,
    ));
}

// ===========================================================================
// Non-callable symbol kind filter
// ===========================================================================

#[test]
fn struct_named_test_fixture_returns_false() {
    assert!(!check(
        "rust",
        "TestFixture",
        "src/tests/fixtures.rs",
        &SymbolKind::Struct,
        &[],
        &[s("test")],
        None,
    ));
}

#[test]
fn enum_named_test_variant_returns_false() {
    assert!(!check(
        "java",
        "TestStatus",
        "src/test/java/Status.java",
        &SymbolKind::Enum,
        &[s("Test")],
        &[],
        None,
    ));
}

#[test]
fn interface_returns_false() {
    assert!(!check(
        "csharp",
        "ITestService",
        "MyProject.Tests/ITestService.cs",
        &SymbolKind::Interface,
        &[],
        &[s("Fact")],
        None,
    ));
}

#[test]
fn variable_returns_false() {
    assert!(!check(
        "javascript",
        "test",
        "src/__tests__/payment.test.js",
        &SymbolKind::Variable,
        &[],
        &[],
        None,
    ));
}

#[test]
fn constant_returns_false() {
    assert!(!check(
        "typescript",
        "TEST_TIMEOUT",
        "src/payment.spec.ts",
        &SymbolKind::Constant,
        &[],
        &[],
        None,
    ));
}

// ===========================================================================
// Constructor edge case — constructors ARE callable
// ===========================================================================

#[test]
fn constructor_with_test_attr_returns_true() {
    // Constructors are callable, so if they have test attributes, they count
    assert!(check(
        "csharp",
        "TestSetup",
        "MyProject.Tests/Setup.cs",
        &SymbolKind::Constructor,
        &[],
        &[s("TestMethod")],
        None,
    ));
}

// ===========================================================================
// Integration tests — run actual extractors and verify is_test metadata
// ===========================================================================

/// Helper: extract symbols from code using the specified language extractor
fn extract_symbols_for(language: &str, file_path: &str, code: &str) -> Vec<crate::base::Symbol> {
    let workspace_root = std::path::PathBuf::from("/tmp/test");
    let tree = super::helpers::init_parser(code, language);
    let results = crate::factory::extract_symbols_and_relationships(
        &tree,
        file_path,
        code,
        language,
        &workspace_root,
    )
    .expect("Extraction should succeed");
    results.symbols
}

/// Helper: check if a symbol has is_test=true in its metadata
fn has_is_test(symbol: &crate::base::Symbol) -> bool {
    symbol
        .metadata
        .as_ref()
        .and_then(|m| m.get("is_test"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

#[test]
fn integration_rust_test_function_detected() {
    let code = r#"
#[test]
fn test_addition() {
    assert_eq!(2 + 2, 4);
}

fn regular_function() {
    println!("hello");
}
"#;
    let symbols = extract_symbols_for("rust", "src/tests/math.rs", code);

    let test_fn = symbols.iter().find(|s| s.name == "test_addition");
    assert!(test_fn.is_some(), "Should extract test_addition");
    assert!(
        has_is_test(test_fn.unwrap()),
        "test_addition should have is_test=true"
    );

    let regular_fn = symbols.iter().find(|s| s.name == "regular_function");
    assert!(regular_fn.is_some(), "Should extract regular_function");
    assert!(
        !has_is_test(regular_fn.unwrap()),
        "regular_function should NOT have is_test"
    );
}

#[test]
fn integration_python_test_function_detected() {
    let code = r#"
def test_payment_processing():
    assert process_payment() == True

def helper_function():
    return 42
"#;
    let symbols = extract_symbols_for("python", "tests/test_payment.py", code);

    let test_fn = symbols.iter().find(|s| s.name == "test_payment_processing");
    assert!(test_fn.is_some(), "Should extract test_payment_processing");
    assert!(
        has_is_test(test_fn.unwrap()),
        "test_payment_processing should have is_test=true"
    );

    let helper_fn = symbols.iter().find(|s| s.name == "helper_function");
    assert!(helper_fn.is_some(), "Should extract helper_function");
    assert!(
        !has_is_test(helper_fn.unwrap()),
        "helper_function should NOT have is_test"
    );
}

#[test]
fn integration_go_test_function_detected() {
    let code = r#"
package payment

import "testing"

func TestProcessPayment(t *testing.T) {
    if true {
        t.Fatal("failed")
    }
}

func processPayment() bool {
    return true
}
"#;
    let symbols = extract_symbols_for("go", "payment/payment_test.go", code);

    let test_fn = symbols.iter().find(|s| s.name == "TestProcessPayment");
    assert!(test_fn.is_some(), "Should extract TestProcessPayment");
    assert!(
        has_is_test(test_fn.unwrap()),
        "TestProcessPayment should have is_test=true"
    );

    let regular_fn = symbols.iter().find(|s| s.name == "processPayment");
    assert!(regular_fn.is_some(), "Should extract processPayment");
    assert!(
        !has_is_test(regular_fn.unwrap()),
        "processPayment should NOT have is_test"
    );
}

#[test]
fn integration_regular_function_no_test_metadata() {
    // A regular Rust function outside test context should not get is_test
    let code = r#"
pub fn calculate_sum(a: i32, b: i32) -> i32 {
    a + b
}
"#;
    let symbols = extract_symbols_for("rust", "src/math.rs", code);

    let sum_fn = symbols.iter().find(|s| s.name == "calculate_sum");
    assert!(sum_fn.is_some(), "Should extract calculate_sum");
    assert!(
        !has_is_test(sum_fn.unwrap()),
        "calculate_sum should NOT have is_test"
    );
}

// ===========================================================================
// Comprehensive dispatch test — exercises every `match language` arm in
// `is_test_symbol` plus the `is_callable` gate and generic fallback.
// ===========================================================================

#[test]
fn test_is_test_symbol_dispatch_across_languages() {
    // Each tuple: (language, name, file_path, kind, decorators, attributes, doc_comment, expected)
    type Case = (
        &'static str,
        &'static str,
        &'static str,
        SymbolKind,
        Vec<String>,
        Vec<String>,
        Option<&'static str>,
        bool,
    );

    let cases: Vec<Case> = vec![
        // --- Rust: attribute-driven only ---
        (
            "rust",
            "test_add",
            "src/tests/math.rs",
            SymbolKind::Function,
            vec![],
            vec![s("test")],
            None,
            true,
        ),
        (
            "rust",
            "test_async",
            "src/lib.rs",
            SymbolKind::Function,
            vec![],
            vec![s("tokio::test")],
            None,
            true,
        ),
        // Rust: test_ prefix WITHOUT #[test] attr → false (Rust is attribute-only)
        (
            "rust",
            "test_something",
            "src/tests/foo.rs",
            SymbolKind::Function,
            vec![],
            vec![],
            None,
            false,
        ),
        // Rust: regular function
        (
            "rust",
            "process_data",
            "src/lib.rs",
            SymbolKind::Function,
            vec![],
            vec![],
            None,
            false,
        ),
        // --- Python: decorator or name prefix (no path guard) ---
        (
            "python",
            "test_login",
            "tests/test_auth.py",
            SymbolKind::Function,
            vec![],
            vec![],
            None,
            true,
        ),
        // Python: test_ prefix works even in non-test path (no path guard in detect_python)
        (
            "python",
            "test_login",
            "src/auth.py",
            SymbolKind::Function,
            vec![],
            vec![],
            None,
            true,
        ),
        // Python: pytest decorator
        (
            "python",
            "some_check",
            "tests/test_auth.py",
            SymbolKind::Function,
            vec![s("pytest.mark.parametrize")],
            vec![],
            None,
            true,
        ),
        // Python: setUp lifecycle method
        (
            "python",
            "setUp",
            "tests/test_foo.py",
            SymbolKind::Method,
            vec![],
            vec![],
            None,
            true,
        ),
        // Python: regular function
        (
            "python",
            "login",
            "src/auth.py",
            SymbolKind::Function,
            vec![],
            vec![],
            None,
            false,
        ),
        // --- Java: @Test decorator ---
        (
            "java",
            "shouldReturnTrue",
            "src/test/java/FooTest.java",
            SymbolKind::Method,
            vec![s("Test")],
            vec![],
            None,
            true,
        ),
        // Java: @BeforeEach lifecycle
        (
            "java",
            "init",
            "src/test/java/FooTest.java",
            SymbolKind::Method,
            vec![s("BeforeEach")],
            vec![],
            None,
            true,
        ),
        // Java: no annotation → false
        (
            "java",
            "processOrder",
            "src/main/java/Order.java",
            SymbolKind::Method,
            vec![],
            vec![],
            None,
            false,
        ),
        // --- Kotlin: shares java/kotlin detector ---
        (
            "kotlin",
            "shouldReturnUser",
            "src/test/kotlin/UserTest.kt",
            SymbolKind::Method,
            vec![s("Test")],
            vec![],
            None,
            true,
        ),
        (
            "kotlin",
            "fetchUser",
            "src/main/kotlin/User.kt",
            SymbolKind::Method,
            vec![],
            vec![],
            None,
            false,
        ),
        // --- Scala: JUnit @Test OR test path OR test name prefix ---
        (
            "scala",
            "shouldCompute",
            "src/test/scala/MathSpec.scala",
            SymbolKind::Method,
            vec![s("Test")],
            vec![],
            None,
            true,
        ),
        // Scala: in test path (no annotation needed)
        (
            "scala",
            "compute",
            "src/test/scala/MathSpec.scala",
            SymbolKind::Method,
            vec![],
            vec![],
            None,
            true,
        ),
        // Scala: testX prefix (MUnit convention)
        (
            "scala",
            "testComputation",
            "src/main/scala/Math.scala",
            SymbolKind::Function,
            vec![],
            vec![],
            None,
            true,
        ),
        // Scala: regular function outside test path, no test prefix
        (
            "scala",
            "compute",
            "src/main/scala/Math.scala",
            SymbolKind::Function,
            vec![],
            vec![],
            None,
            false,
        ),
        // --- Elixir: test_ prefix, test path, or "test " prefix ---
        (
            "elixir",
            "test_addition",
            "test/math_test.exs",
            SymbolKind::Function,
            vec![],
            vec![],
            None,
            true,
        ),
        // Elixir: in test/ directory (any function)
        (
            "elixir",
            "setup_context",
            "test/support/helpers.exs",
            SymbolKind::Function,
            vec![],
            vec![],
            None,
            true,
        ),
        // Elixir: "test " prefix (ExUnit macro naming)
        (
            "elixir",
            "test greets the world",
            "lib/my_app.ex",
            SymbolKind::Function,
            vec![],
            vec![],
            None,
            true,
        ),
        // Elixir: regular function outside test path
        (
            "elixir",
            "add",
            "lib/math.ex",
            SymbolKind::Function,
            vec![],
            vec![],
            None,
            false,
        ),
        // --- C#: attribute-driven ---
        (
            "csharp",
            "ShouldWork",
            "Tests/MyTest.cs",
            SymbolKind::Method,
            vec![],
            vec![s("Fact")],
            None,
            true,
        ),
        (
            "csharp",
            "ShouldAlsoWork",
            "Tests/MyTest.cs",
            SymbolKind::Method,
            vec![],
            vec![s("Theory")],
            None,
            true,
        ),
        // C#: bracketed attribute format
        (
            "csharp",
            "ShouldValidate",
            "Tests/MyTest.cs",
            SymbolKind::Method,
            vec![],
            vec![s("[Fact]")],
            None,
            true,
        ),
        // C#: no test attribute → false
        (
            "csharp",
            "ProcessOrder",
            "MyProject/OrderService.cs",
            SymbolKind::Method,
            vec![],
            vec![],
            None,
            false,
        ),
        // --- Razor: routes to C# detector ---
        (
            "razor",
            "ShouldRender",
            "MyProject.Tests/ButtonTests.cshtml",
            SymbolKind::Method,
            vec![],
            vec![s("Fact")],
            None,
            true,
        ),
        (
            "razor",
            "OnGet",
            "MyProject/Pages/Index.cshtml",
            SymbolKind::Method,
            vec![],
            vec![],
            None,
            false,
        ),
        // --- Go: Test/Fuzz/Example prefix AND _test.go file ---
        (
            "go",
            "TestParseInput",
            "parser/parser_test.go",
            SymbolKind::Function,
            vec![],
            vec![],
            None,
            true,
        ),
        (
            "go",
            "FuzzParse",
            "parser/parser_test.go",
            SymbolKind::Function,
            vec![],
            vec![],
            None,
            true,
        ),
        (
            "go",
            "ExampleParse",
            "parser/parser_test.go",
            SymbolKind::Function,
            vec![],
            vec![],
            None,
            true,
        ),
        // Go: Test prefix but NOT _test.go → false
        (
            "go",
            "TestParseInput",
            "parser/parser.go",
            SymbolKind::Function,
            vec![],
            vec![],
            None,
            false,
        ),
        // Go: _test.go but no recognized prefix → false
        (
            "go",
            "helperSetup",
            "parser/parser_test.go",
            SymbolKind::Function,
            vec![],
            vec![],
            None,
            false,
        ),
        // --- JavaScript: test/it/describe in test file ---
        (
            "javascript",
            "test",
            "src/__tests__/auth.test.js",
            SymbolKind::Function,
            vec![],
            vec![],
            None,
            true,
        ),
        (
            "javascript",
            "it",
            "tests/payment.test.js",
            SymbolKind::Function,
            vec![],
            vec![],
            None,
            true,
        ),
        (
            "javascript",
            "describe",
            "tests/payment.test.js",
            SymbolKind::Function,
            vec![],
            vec![],
            None,
            true,
        ),
        // JS: test function NOT in test file → false
        (
            "javascript",
            "test",
            "src/utils.js",
            SymbolKind::Function,
            vec![],
            vec![],
            None,
            false,
        ),
        // --- TypeScript: same rules as JS ---
        (
            "typescript",
            "describe",
            "src/payment.spec.ts",
            SymbolKind::Function,
            vec![],
            vec![],
            None,
            true,
        ),
        (
            "typescript",
            "test",
            "src/utils.ts",
            SymbolKind::Function,
            vec![],
            vec![],
            None,
            false,
        ),
        // --- PHP: @test doc or test prefix in test path ---
        (
            "php",
            "itShouldProcess",
            "tests/PaymentTest.php",
            SymbolKind::Method,
            vec![],
            vec![],
            Some("/** @test */"),
            true,
        ),
        (
            "php",
            "testProcessPayment",
            "tests/PaymentTest.php",
            SymbolKind::Method,
            vec![],
            vec![],
            None,
            true,
        ),
        // PHP: @test works even outside test path
        (
            "php",
            "someMethod",
            "src/Service.php",
            SymbolKind::Method,
            vec![],
            vec![],
            Some("/** @test */"),
            true,
        ),
        // PHP: test prefix in prod code → false (path guard)
        (
            "php",
            "testConnection",
            "src/database.php",
            SymbolKind::Method,
            vec![],
            vec![],
            None,
            false,
        ),
        // --- Ruby: test_ prefix AND test/spec path ---
        (
            "ruby",
            "test_process_payment",
            "spec/payment_spec.rb",
            SymbolKind::Method,
            vec![],
            vec![],
            None,
            true,
        ),
        (
            "ruby",
            "test_login",
            "test/auth_test.rb",
            SymbolKind::Method,
            vec![],
            vec![],
            None,
            true,
        ),
        // Ruby: test_ prefix but NOT in test path → false
        (
            "ruby",
            "test_connection",
            "lib/database.rb",
            SymbolKind::Method,
            vec![],
            vec![],
            None,
            false,
        ),
        // --- Swift: test prefix + test path ---
        (
            "swift",
            "testLogin",
            "Tests/AuthTests.swift",
            SymbolKind::Method,
            vec![],
            vec![],
            None,
            true,
        ),
        // Swift: setUp lifecycle in test path
        (
            "swift",
            "setUp",
            "Tests/AuthTests.swift",
            SymbolKind::Method,
            vec![],
            vec![],
            None,
            true,
        ),
        // Swift: test prefix NOT in test path → false
        (
            "swift",
            "testConnection",
            "Sources/App/DB.swift",
            SymbolKind::Method,
            vec![],
            vec![],
            None,
            false,
        ),
        // --- Dart: @isTest decorator or test prefix in test path ---
        (
            "dart",
            "myTest",
            "test/widget_test.dart",
            SymbolKind::Function,
            vec![s("isTest")],
            vec![],
            None,
            true,
        ),
        (
            "dart",
            "testWidgetRendering",
            "test/widget_test.dart",
            SymbolKind::Function,
            vec![],
            vec![],
            None,
            true,
        ),
        // Dart: test prefix in prod code → false (path guard)
        (
            "dart",
            "testWidgetRendering",
            "lib/widgets.dart",
            SymbolKind::Function,
            vec![],
            vec![],
            None,
            false,
        ),
        // --- Generic fallback: test_/Test prefix + test path ---
        (
            "lua",
            "test_something",
            "tests/test_util.lua",
            SymbolKind::Function,
            vec![],
            vec![],
            None,
            true,
        ),
        (
            "zig",
            "TestAllocator",
            "tests/allocator_test.zig",
            SymbolKind::Function,
            vec![],
            vec![],
            None,
            true,
        ),
        // Generic: test_ prefix but NOT in test path → false
        (
            "lua",
            "test_helper",
            "src/utils.lua",
            SymbolKind::Function,
            vec![],
            vec![],
            None,
            false,
        ),
        // Unknown language falls through to generic
        (
            "brainfuck",
            "test_something",
            "tests/test.bf",
            SymbolKind::Function,
            vec![],
            vec![],
            None,
            true,
        ),
        (
            "brainfuck",
            "run_program",
            "src/main.bf",
            SymbolKind::Function,
            vec![],
            vec![],
            None,
            false,
        ),
        // --- is_callable gate: non-callable kinds always return false ---
        (
            "rust",
            "TestFixture",
            "src/tests/foo.rs",
            SymbolKind::Struct,
            vec![],
            vec![s("test")],
            None,
            false,
        ),
        (
            "python",
            "TestPaymentProcessor",
            "tests/test_payment.py",
            SymbolKind::Class,
            vec![],
            vec![],
            None,
            false,
        ),
        (
            "java",
            "TestStatus",
            "src/test/java/Status.java",
            SymbolKind::Enum,
            vec![s("Test")],
            vec![],
            None,
            false,
        ),
        (
            "csharp",
            "ITestService",
            "Tests/ITestService.cs",
            SymbolKind::Interface,
            vec![],
            vec![s("Fact")],
            None,
            false,
        ),
        (
            "javascript",
            "test",
            "src/__tests__/payment.test.js",
            SymbolKind::Variable,
            vec![],
            vec![],
            None,
            false,
        ),
        // Constructor IS callable
        (
            "csharp",
            "TestSetup",
            "MyProject.Tests/Setup.cs",
            SymbolKind::Constructor,
            vec![],
            vec![s("TestMethod")],
            None,
            true,
        ),
    ];

    for (i, (lang, name, path, kind, decorators, attributes, doc, expected)) in
        cases.iter().enumerate()
    {
        let result = is_test_symbol(lang, name, path, kind, decorators, attributes, *doc);
        assert_eq!(
            result, *expected,
            "Case {} FAILED: is_test_symbol({:?}, {:?}, {:?}, {:?}) = {} but expected {}",
            i, lang, name, path, kind, result, expected,
        );
    }
}

#[test]
fn integration_zig_test_block_detected() {
    let code = r#"
const std = @import("std");

test "basic addition" {
    try std.testing.expectEqual(@as(u32, 4), 2 + 2);
}

pub fn add(a: u32, b: u32) u32 {
    return a + b;
}
"#;
    let symbols = extract_symbols_for("zig", "tests/math_test.zig", code);

    let test_block = symbols.iter().find(|s| s.name == "basic addition");
    assert!(
        test_block.is_some(),
        "Should extract test block 'basic addition'"
    );
    assert!(
        has_is_test(test_block.unwrap()),
        "Zig test block should have is_test=true"
    );

    let add_fn = symbols.iter().find(|s| s.name == "add");
    assert!(add_fn.is_some(), "Should extract add function");
    assert!(
        !has_is_test(add_fn.unwrap()),
        "add function should NOT have is_test"
    );
}
