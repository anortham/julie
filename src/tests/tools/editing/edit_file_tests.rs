//! Golden master tests for the edit_file tool.

use crate::tools::editing::edit_file::apply_edit;
use std::fs;
use std::path::PathBuf;

fn fixture_source(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/editing/sources")
        .join(name)
}

fn fixture_control(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/editing/controls/edit-file")
        .join(name)
}

fn load(path: &PathBuf) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("Failed to read {}: {}", path.display(), e))
}

#[test]
fn test_exact_replace() {
    let source = load(&fixture_source("dmp_rust_module.rs"));
    let expected = load(&fixture_control("rust_exact_replace.rs"));

    let result = apply_edit(
        &source,
        "pub fn get_user(&self, id: u64) -> Option<&String> {",
        "pub fn get_user(&self, id: u64) -> Result<&String, NotFoundError> {",
        "first",
    )
    .expect("Edit should succeed");

    assert_eq!(result, expected, "Output should match golden master (exact replace)");
}

#[test]
fn test_replace_all_occurrences() {
    let source = load(&fixture_source("dmp_rust_module.rs"));
    let expected = load(&fixture_control("rust_replace_all.rs"));

    let result = apply_edit(&source, "(&self", "(&mut self", "all")
        .expect("Edit should succeed");

    assert_eq!(result, expected, "Output should match golden master (replace all)");
}

#[test]
fn test_markdown_edit() {
    let source = load(&fixture_source("dmp_markdown_doc.md"));
    let expected = load(&fixture_control("markdown_edit.md"));

    let old_text = "Add advanced features and testing.\n\n- Task C: Integration tests\n- Task D: Performance tuning";
    let new_text = "Redesigned to focus on security hardening.\n\n- Task C: Security audit\n- Task D: Penetration testing\n- Task E: Fix vulnerabilities";

    let result = apply_edit(&source, old_text, new_text, "first")
        .expect("Edit should succeed");

    assert_eq!(result, expected, "Output should match golden master (markdown edit)");
}

#[test]
fn test_no_match_returns_error() {
    let source = "fn main() {}\n";
    let result = apply_edit(source, "fn nonexistent()", "fn replacement()", "first");
    assert!(result.is_err(), "Should return error when no match found");
}

#[test]
fn test_empty_old_text_returns_error() {
    let result = apply_edit("some content", "", "replacement", "first");
    assert!(result.is_err(), "Should return error for empty old_text");
}

#[test]
fn test_replace_last_occurrence() {
    let source = "aaa bbb aaa bbb aaa";
    let result = apply_edit(source, "aaa", "ccc", "last").unwrap();
    assert_eq!(result, "aaa bbb aaa bbb ccc");
}

#[test]
fn test_invalid_occurrence_returns_error() {
    let result = apply_edit("content", "con", "new", "invalid");
    assert!(result.is_err());
}
