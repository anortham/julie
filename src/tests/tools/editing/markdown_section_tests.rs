//! Tests for markdown section line ranges covering full content, not just headings.

use crate::extractors::base::Symbol;
use crate::extractors::manager::ExtractorManager;
use std::path::PathBuf;

fn extract_markdown_symbols(source: &str) -> Vec<Symbol> {
    let workspace_root = PathBuf::from("/tmp");
    let manager = ExtractorManager::new();
    manager
        .extract_symbols("/tmp/test.md", source, &workspace_root)
        .expect("Failed to extract markdown symbols")
}

#[test]
fn test_section_line_range_covers_content() {
    let markdown = "# Title\n\nFirst paragraph.\n\n## Section A\n\nContent of section A.\n\nMore content.\n\n## Section B\n\nContent of section B.\n";

    let symbols = extract_markdown_symbols(markdown);

    let section_a = symbols
        .iter()
        .find(|s| s.name == "Section A")
        .expect("Should find Section A");

    assert!(
        section_a.end_line > section_a.start_line + 1,
        "Section A end_line ({}) should extend well beyond start_line ({}) to cover content",
        section_a.end_line,
        section_a.start_line
    );

    let section_b = symbols
        .iter()
        .find(|s| s.name == "Section B")
        .expect("Should find Section B");

    assert!(
        section_b.start_line >= section_a.end_line,
        "Section B start ({}) should be at or after Section A end ({})",
        section_b.start_line,
        section_a.end_line
    );
}

#[test]
fn test_section_content_accessible_via_byte_range() {
    let markdown = "# Doc\n\n## Quick Reference\n\n```bash\ncargo build\ncargo test\n```\n\nSome notes here.\n\n## Next Section\n\nOther stuff.\n";

    let symbols = extract_markdown_symbols(markdown);

    let quick_ref = symbols
        .iter()
        .find(|s| s.name == "Quick Reference")
        .expect("Should find Quick Reference");

    let content = &markdown[quick_ref.start_byte as usize..quick_ref.end_byte as usize];

    assert!(
        content.contains("cargo build"),
        "Section byte range should include code block content. Got: {}",
        content
    );
    assert!(
        content.contains("Some notes here"),
        "Section byte range should include paragraph content. Got: {}",
        content
    );
}
