//! C.3 — enriched Tantivy schema tests.
//!
//! Covers:
//! - `classify_role` for vendor / generated / test / docs / source path
//!   patterns across the 8 project layouts called out in CLAUDE.md.
//! - `test_subrole` for unit / integration / smoke.
//! - Schema contains the three new field names.

use crate::search::scoring::{classify_role, test_subrole};

// ----- classify_role -----

#[test]
fn test_classify_role_rust_src() {
    assert_eq!(classify_role("src/main.rs", "rust"), "source");
    assert_eq!(classify_role("src/foo/mod.rs", "rust"), "source");
}

#[test]
fn test_classify_role_rust_tests() {
    assert_eq!(classify_role("src/tests/foo.rs", "rust"), "test");
    assert_eq!(classify_role("tests/integration.rs", "rust"), "test");
}

#[test]
fn test_classify_role_csharp_tests_suffix() {
    // C# .Tests convention.
    assert_eq!(
        classify_role("MyProject.Tests/UserTests.cs", "csharp"),
        "test"
    );
}

#[test]
fn test_classify_role_python_test_dir() {
    assert_eq!(classify_role("tests/test_auth.py", "python"), "test");
}

#[test]
fn test_classify_role_js_co_located_test_file() {
    assert_eq!(classify_role("src/auth.test.ts", "typescript"), "test");
    assert_eq!(classify_role("src/auth.spec.tsx", "typescript"), "test");
}

#[test]
fn test_classify_role_go_test_file() {
    assert_eq!(classify_role("pkg/auth/auth_test.go", "go"), "test");
}

#[test]
fn test_classify_role_docs_by_path() {
    assert_eq!(classify_role("docs/plan.md", "markdown"), "docs");
    assert_eq!(classify_role("documentation/api.rst", "rust"), "docs");
}

#[test]
fn test_classify_role_docs_by_language() {
    // Even a non-docs directory: a markdown file is docs-role.
    assert_eq!(classify_role("src/README.md", "markdown"), "docs");
    assert_eq!(classify_role("config/app.json", "json"), "docs");
    assert_eq!(classify_role("Cargo.toml", "toml"), "docs");
}

#[test]
fn test_classify_role_vendor_node_modules() {
    assert_eq!(
        classify_role("node_modules/lodash/lodash.js", "javascript"),
        "vendor"
    );
}

#[test]
fn test_classify_role_vendor_third_party() {
    assert_eq!(
        classify_role("third_party/protobuf/parser.cc", "cpp"),
        "vendor"
    );
}

#[test]
fn test_classify_role_generated_target_dir() {
    assert_eq!(
        classify_role("target/debug/build/foo.rs", "rust"),
        "generated"
    );
}

#[test]
fn test_classify_role_generated_dist() {
    assert_eq!(classify_role("dist/bundle.js", "javascript"), "generated");
}

#[test]
fn test_classify_role_vendor_takes_priority_over_test() {
    // A test inside node_modules should be "vendor", not "test", so it
    // gets de-emphasized as third-party rather than promoted as a test
    // helper.
    assert_eq!(
        classify_role("node_modules/foo/test/index.test.js", "javascript"),
        "vendor"
    );
}

#[test]
fn test_classify_role_generated_takes_priority_over_test() {
    assert_eq!(
        classify_role("target/debug/build/tests/generated.rs", "rust"),
        "generated"
    );
}

// ----- test_subrole -----

#[test]
fn test_test_subrole_empty_for_non_test_path() {
    assert_eq!(test_subrole("src/main.rs"), "");
    assert_eq!(test_subrole("docs/plan.md"), "");
}

#[test]
fn test_test_subrole_integration() {
    assert_eq!(test_subrole("src/tests/integration/foo.rs"), "integration");
    assert_eq!(
        test_subrole("tests/integration_tests/foo.rs"),
        "integration"
    );
}

#[test]
fn test_test_subrole_smoke() {
    assert_eq!(test_subrole("src/tests/smoke/health.rs"), "smoke");
}

#[test]
fn test_test_subrole_unit() {
    assert_eq!(test_subrole("src/tests/unit/parse.rs"), "unit");
}

#[test]
fn test_test_subrole_unclassified_test_path() {
    // Test path but no sub-role segment — empty string is allowed.
    assert_eq!(test_subrole("src/tests/foo.rs"), "");
    assert_eq!(test_subrole("tests/random.rs"), "");
}

// ----- Schema field presence -----

#[test]
fn test_schema_contains_c3_enriched_fields() {
    let schema = crate::search::create_schema();
    assert!(
        schema.get_field("role").is_ok(),
        "schema must define `role` field"
    );
    assert!(
        schema.get_field("test_role").is_ok(),
        "schema must define `test_role` field"
    );
}

#[test]
fn test_schema_fields_struct_resolves_c3_fields() {
    // SchemaFields::new() panics if any field is missing. Constructing
    // it is a proof the three new fields are wired through.
    let schema = crate::search::create_schema();
    let _fields = crate::search::SchemaFields::new(&schema);
}
