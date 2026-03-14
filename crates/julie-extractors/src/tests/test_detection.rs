// Tests for test_detection module — is_test_symbol() function
//
// Covers all 31 languages via language-specific rules + generic fallback.

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
    is_test_symbol(language, name, file_path, kind, decorators, attributes, doc_comment)
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
