//! Documentation excerpt normalization coverage.

use super::*;

#[test]
fn test_extract_doc_excerpt_multi_line() {
    let doc = "/// Record a completed tool call.\n/// Bumps in-memory atomics synchronously, then spawns async task\n/// for source_bytes lookup + SQLite write.";
    let excerpt = extract_doc_excerpt(doc);
    assert!(
        excerpt.contains("Record a completed tool call"),
        "First sentence should be present: {excerpt}"
    );
    assert!(
        excerpt.contains("SQLite write"),
        "Later sentences should be present: {excerpt}"
    );
}

#[test]
fn test_extract_doc_excerpt_strips_rust_prefixes() {
    let doc = "/// First line.\n/// Second line.";
    let excerpt = extract_doc_excerpt(doc);
    assert!(
        !excerpt.contains("///"),
        "Should strip /// prefix: {excerpt}"
    );
    assert!(excerpt.contains("First line."));
    assert!(excerpt.contains("Second line."));
}

#[test]
fn test_extract_doc_excerpt_strips_csharp_xml_tags() {
    let doc = "/// <summary>\n/// Handles authentication.\n/// </summary>\n/// <param name=\"token\">The auth token.</param>";
    let excerpt = extract_doc_excerpt(doc);
    assert!(
        !excerpt.contains("<summary>"),
        "Should strip XML tags: {excerpt}"
    );
    assert!(
        !excerpt.contains("<param"),
        "Should strip param tags: {excerpt}"
    );
    assert!(
        excerpt.contains("Handles authentication"),
        "Content should survive: {excerpt}"
    );
}

#[test]
fn test_extract_doc_excerpt_handles_python_docstring() {
    let doc = "# Process the input data.\n# Returns the transformed result.";
    let excerpt = extract_doc_excerpt(doc);
    assert!(
        excerpt.contains("Process the input data"),
        "Should strip # prefix: {excerpt}"
    );
    assert!(
        excerpt.contains("Returns the transformed result"),
        "Second line should be present: {excerpt}"
    );
}

#[test]
fn test_extract_doc_excerpt_truncates_at_budget() {
    // Create a doc longer than MAX_DOC_EXCERPT_CHARS (300)
    let long_line = "/// ".to_string() + &"word ".repeat(80); // ~400 chars of content
    let excerpt = extract_doc_excerpt(&long_line);
    assert!(
        excerpt.len() <= 300,
        "Should truncate to 300 bytes: len={}, excerpt: {excerpt}",
        excerpt.len()
    );
}

#[test]
fn test_extract_doc_excerpt_empty_doc() {
    assert_eq!(extract_doc_excerpt(""), "");
    assert_eq!(extract_doc_excerpt("///"), "");
    assert_eq!(extract_doc_excerpt("/// \n/// "), "");
}

#[test]
fn test_extract_doc_excerpt_jsdoc_block() {
    let doc =
        "/**\n * Initialize the connection pool.\n * Validates config before connecting.\n */";
    let excerpt = extract_doc_excerpt(doc);
    assert!(
        excerpt.contains("Initialize the connection pool"),
        "Should handle JSDoc: {excerpt}"
    );
    assert!(
        excerpt.contains("Validates config"),
        "Second line: {excerpt}"
    );
}
