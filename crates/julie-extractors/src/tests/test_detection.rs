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
