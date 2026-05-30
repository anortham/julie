//! Java test-role detection signals (Miller bridge test-roles).
//!
//! EXTRACTOR-level assertions; the role classifier lives in the `julie` crate.
//! Java covers:
//! - **JUnit 4/5**: `@Test` / `@ParameterizedTest` / `@BeforeEach` … captured as
//!   annotation markers on methods (already wired) + the method `is_test` flag.
//! - **JUnit 3 / TestNG legacy**: a `class … extends TestCase` is a test
//!   container with no annotation. The extractor records the superclass +
//!   interfaces under the canonical `base_types` key; the classifier's base-type
//!   rule + `test_base_types = ["TestCase"]` config light it up.
//! - **`@Nested`**: a JUnit 5 nested container class — the class extractor now
//!   captures class-level annotations so the classifier sees `nested`.

use crate::base::SymbolKind;
use crate::java::JavaExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn symbols(code: &str, file: &str) -> Vec<crate::base::Symbol> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_java::LANGUAGE.into())
        .expect("load Java grammar");
    let tree = parser.parse(code, None).expect("parse Java");
    let mut ext = JavaExtractor::new(
        "java".to_string(),
        file.to_string(),
        code.to_string(),
        &PathBuf::from("/test/workspace"),
    );
    ext.extract_symbols(&tree)
}

fn base_types(symbol: &crate::base::Symbol) -> Vec<String> {
    symbol
        .metadata
        .as_ref()
        .and_then(|m| m.get("base_types"))
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn annotation_keys(symbol: &crate::base::Symbol) -> Vec<String> {
    symbol
        .annotations
        .iter()
        .map(|a| a.annotation_key.clone())
        .collect()
}

fn is_test(symbol: &crate::base::Symbol) -> bool {
    symbol
        .metadata
        .as_ref()
        .and_then(|m| m.get("is_test"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

#[test]
fn junit3_testcase_subclass_emits_base_types() {
    // `class CalcTest extends TestCase` — JUnit 3 has no annotation; detection
    // hinges on the superclass. The class symbol must record base_types
    // containing "TestCase". Path-independent.
    let code = r#"
import junit.framework.TestCase;

public class CalcTest extends TestCase {
    public void testAdd() {
        assertEquals(4, 2 + 2);
    }
}
"#;
    let syms = symbols(code, "src/test/java/CalcTest.java");
    let class_sym = syms
        .iter()
        .find(|s| s.name == "CalcTest" && s.kind == SymbolKind::Class)
        .unwrap_or_else(|| panic!("expected CalcTest class, got {syms:?}"));
    assert!(
        base_types(class_sym).iter().any(|b| b == "TestCase"),
        "class must record TestCase in base_types metadata, got {:?}",
        base_types(class_sym)
    );
}

#[test]
fn superclass_and_interfaces_both_recorded_in_base_types() {
    // `class FooTest extends TestCase implements Serializable` — base_types must
    // include BOTH the superclass and the implemented interface(s).
    let code = r#"
public class FooTest extends TestCase implements Serializable {
    public void testThing() {}
}
"#;
    let syms = symbols(code, "src/test/java/FooTest.java");
    let class_sym = syms.iter().find(|s| s.name == "FooTest").unwrap();
    let bt = base_types(class_sym);
    assert!(bt.iter().any(|b| b == "TestCase"), "superclass, got {bt:?}");
    assert!(
        bt.iter().any(|b| b == "Serializable"),
        "interface, got {bt:?}"
    );
}

#[test]
fn nested_class_annotation_captured() {
    // `@Nested class WhenEmpty` — the class extractor previously dropped
    // class-level annotations. It must now expose annotation_key "nested" so the
    // classifier can mark it a TestContainer via `test_container = ["nested"]`.
    let code = r#"
class StackTest {
    @Nested
    class WhenEmpty {
        @Test
        void isEmpty() {}
    }
}
"#;
    let syms = symbols(code, "src/test/java/StackTest.java");
    let nested = syms
        .iter()
        .find(|s| s.name == "WhenEmpty")
        .unwrap_or_else(|| panic!("expected WhenEmpty class, got {syms:?}"));
    assert!(
        annotation_keys(nested).iter().any(|k| k == "nested"),
        "@Nested must yield annotation_key 'nested', got {:?}",
        annotation_keys(nested)
    );
}

#[test]
fn junit5_test_method_has_annotation_and_is_test() {
    // `@Test void shouldAdd()` — JUnit 5: the method carries annotation_key "test"
    // AND the `is_test` flag (annotation-driven, path-independent).
    let code = r#"
class CalculatorTest {
    @Test
    void shouldAdd() {
        assertEquals(4, 2 + 2);
    }
}
"#;
    let syms = symbols(code, "src/main/java/CalculatorTest.java");
    let method = syms
        .iter()
        .find(|s| s.name == "shouldAdd")
        .unwrap_or_else(|| panic!("expected shouldAdd method, got {syms:?}"));
    assert!(
        annotation_keys(method).iter().any(|k| k == "test"),
        "@Test must yield annotation_key 'test', got {:?}",
        annotation_keys(method)
    );
    assert!(is_test(method), "@Test method must be flagged is_test");
}
