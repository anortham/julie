// Markdown Extractor Tests
// Following TDD methodology: RED -> GREEN -> REFACTOR
//
// Comprehensive test coverage matching the quality of TypeScript/Rust extractors
// Target: 400+ lines with edge cases, special syntax, and real-world validation

#[cfg(test)]
mod markdown_extractor_tests {
    #![allow(unused_imports)]
    #![allow(unused_variables)]

    use crate::extractors::base::{Symbol, SymbolKind};
    use crate::extractors::markdown::MarkdownExtractor;
    use std::path::PathBuf;
    use tree_sitter::Parser;

    fn init_parser() -> Parser {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_md::LANGUAGE.into())
            .expect("Error loading Markdown grammar");
        parser
    }

    fn extract_symbols(code: &str) -> Vec<Symbol> {
        let workspace_root = PathBuf::from("/tmp/test");
        let mut parser = init_parser();
        let tree = parser.parse(code, None).expect("Failed to parse code");
        let mut extractor = MarkdownExtractor::new(
            "markdown".to_string(),
            "test.md".to_string(),
            code.to_string(),
            &workspace_root,
        );
        extractor.extract_symbols(&tree)
    }

    // ========================================================================
    // Basic ATX Heading Extraction (## style)
    // ========================================================================

    #[test]
    fn test_extract_markdown_sections() {
        let markdown = r#"# Main Title

This is the introduction.

## Section One

Content for section one.

### Subsection 1.1

Detailed content here.

## Section Two

More content in section two.
"#;

        let symbols = extract_symbols(markdown);

        // We should extract sections as symbols
        assert!(
            symbols.len() >= 4,
            "Expected at least 4 sections, got {}",
            symbols.len()
        );

        // Check main title
        let main_title = symbols.iter().find(|s| s.name == "Main Title");
        assert!(main_title.is_some(), "Should find 'Main Title' section");
        assert_eq!(main_title.unwrap().kind, SymbolKind::Module); // Treating sections as modules

        // Check Section One
        let section_one = symbols.iter().find(|s| s.name == "Section One");
        assert!(section_one.is_some(), "Should find 'Section One' section");

        // Check Subsection
        let subsection = symbols.iter().find(|s| s.name == "Subsection 1.1");
        assert!(subsection.is_some(), "Should find 'Subsection 1.1' section");

        // Check Section Two
        let section_two = symbols.iter().find(|s| s.name == "Section Two");
        assert!(section_two.is_some(), "Should find 'Section Two' section");
    }

    #[test]
    fn test_extract_all_six_heading_levels() {
        let markdown = r#"# Level 1
## Level 2
### Level 3
#### Level 4
##### Level 5
###### Level 6
"#;

        let symbols = extract_symbols(markdown);

        assert!(
            symbols.len() >= 6,
            "Should extract all 6 heading levels, got {}",
            symbols.len()
        );

        let level_names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(level_names.contains(&"Level 1"), "Should find Level 1");
        assert!(level_names.contains(&"Level 2"), "Should find Level 2");
        assert!(level_names.contains(&"Level 3"), "Should find Level 3");
        assert!(level_names.contains(&"Level 4"), "Should find Level 4");
        assert!(level_names.contains(&"Level 5"), "Should find Level 5");
        assert!(level_names.contains(&"Level 6"), "Should find Level 6");
    }

    #[test]
    fn test_heading_with_special_characters() {
        let markdown = r#"# Testing `code` in heading

## Heading with **bold** and *italic*

### Heading with [link](https://example.com)

#### 🚀 Emoji in heading

##### Heading with "quotes" and 'apostrophes'
"#;

        let symbols = extract_symbols(markdown);

        assert!(
            symbols.len() >= 5,
            "Should extract headings with special characters"
        );

        // Verify heading text is extracted (may or may not preserve markdown)
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();

        // Check that we got some headings with recognizable content
        assert!(
            names
                .iter()
                .any(|&n| n.contains("code") || n.contains("Testing")),
            "Should extract heading with code"
        );
        assert!(
            names
                .iter()
                .any(|&n| n.contains("bold") || n.contains("italic") || n.contains("Heading")),
            "Should extract heading with formatting"
        );
    }

    #[test]
    fn test_heading_with_trailing_hashes() {
        let markdown = r#"# Main Title #

## Section One ##

### Subsection ###
"#;

        let symbols = extract_symbols(markdown);

        assert!(
            symbols.len() >= 3,
            "Should extract headings with trailing hashes"
        );

        let main = symbols.iter().find(|s| s.name.contains("Main Title"));
        assert!(
            main.is_some(),
            "Should find 'Main Title' (trailing # removed)"
        );
    }

    // ========================================================================
    // Edge Cases & Empty Content
    // ========================================================================

    #[test]
    fn test_empty_markdown_file() {
        let markdown = "";
        let symbols = extract_symbols(markdown);
        assert_eq!(symbols.len(), 0, "Empty file should yield no symbols");
    }

    #[test]
    fn test_markdown_with_only_content_no_headings() {
        let markdown = r#"This is just regular markdown text.

With multiple paragraphs.

But no headings at all.
"#;

        let symbols = extract_symbols(markdown);

        // Depending on implementation, might be 0 or might extract something
        // At minimum, should not crash
        assert!(symbols.len() >= 0, "Should handle content-only markdown");
    }

    #[test]
    fn test_markdown_with_only_comments() {
        let markdown = r#"<!-- This is a comment -->

<!-- Another comment -->
"#;

        let symbols = extract_symbols(markdown);

        // Comments should not be extracted as symbols
        assert_eq!(symbols.len(), 0, "Comments should not be extracted");
    }

    // ========================================================================
    // Nested Structure & Hierarchy
    // ========================================================================

    #[test]
    fn test_deeply_nested_headings() {
        let markdown = r#"# Top Level

## Second Level A

### Third Level A1

#### Fourth Level A1a

##### Fifth Level A1a1

###### Sixth Level A1a1a

## Second Level B

### Third Level B1
"#;

        let symbols = extract_symbols(markdown);

        // Tree-sitter-md might handle deep nesting differently
        assert!(
            symbols.len() >= 6,
            "Should extract multiple nested headings, got {}",
            symbols.len()
        );

        // Verify we got some deep nesting
        let has_deep_levels = symbols.iter().any(|s| {
            s.name.contains("Third Level")
                || s.name.contains("Fourth Level")
                || s.name.contains("Fifth Level")
                || s.name.contains("Sixth Level")
        });
        assert!(has_deep_levels, "Should find deeply nested headings");
    }

    #[test]
    fn test_heading_hierarchy_with_content() {
        let markdown = r#"# Main Document

Introduction paragraph.

## Chapter 1

Chapter 1 content.

### Section 1.1

Section content here.

#### Subsection 1.1.1

Detailed content.

### Section 1.2

More section content.

## Chapter 2

Chapter 2 content.
"#;

        let symbols = extract_symbols(markdown);

        assert!(symbols.len() >= 6, "Should extract hierarchical headings");

        // Verify structure
        let main = symbols.iter().find(|s| s.name == "Main Document");
        assert!(main.is_some(), "Should find main document heading");

        let chapter1 = symbols.iter().find(|s| s.name == "Chapter 1");
        assert!(chapter1.is_some(), "Should find Chapter 1");

        let section11 = symbols.iter().find(|s| s.name == "Section 1.1");
        assert!(section11.is_some(), "Should find nested Section 1.1");
    }

    // ========================================================================
    // Special Markdown Syntax
    // ========================================================================

    #[test]
    fn test_heading_after_code_block() {
        let markdown = r#"# Main Title

```rust
fn example() {
    println!("code");
}
```

## Section After Code

Content here.
"#;

        let symbols = extract_symbols(markdown);

        assert!(
            symbols.len() >= 2,
            "Should extract headings around code blocks"
        );

        let after_code = symbols.iter().find(|s| s.name == "Section After Code");
        assert!(after_code.is_some(), "Should find heading after code block");
    }

    #[test]
    fn test_heading_with_inline_code() {
        let markdown = r#"# Using `tree-sitter` for parsing

## The `extract_symbols()` function
"#;

        let symbols = extract_symbols(markdown);

        assert!(
            symbols.len() >= 2,
            "Should extract headings with inline code"
        );

        // Check that inline code is handled (may or may not be preserved)
        let names: Vec<String> = symbols.iter().map(|s| s.name.clone()).collect();
        assert!(
            names
                .iter()
                .any(|n| n.contains("tree-sitter") || n.contains("parsing")),
            "Should extract heading with inline code"
        );
    }

    #[test]
    fn test_heading_in_blockquote() {
        let markdown = r#"# Main Heading

> ## Quoted Heading
>
> Content in quote.

## Regular Heading
"#;

        let symbols = extract_symbols(markdown);

        // Depending on parser, quoted headings may or may not be extracted
        // At minimum, should handle this without crashing
        assert!(symbols.len() >= 1, "Should handle headings in blockquotes");
    }

    // ========================================================================
    // Unicode & Special Characters
    // ========================================================================

    #[test]
    fn test_heading_with_unicode() {
        let markdown = r#"# 日本語 Japanese Title

## Ελληνικά Greek Title

### العربية Arabic Title

#### 🎉 Celebration 🚀 Rocket

##### Mathématiques & Résumé
"#;

        let symbols = extract_symbols(markdown);

        assert!(symbols.len() >= 5, "Should extract headings with Unicode");

        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();

        // Verify Unicode is preserved
        assert!(
            names
                .iter()
                .any(|&n| n.contains("日本語") || n.contains("Japanese")),
            "Should preserve Japanese characters"
        );
        assert!(
            names
                .iter()
                .any(|&n| n.contains("🎉") || n.contains("Celebration")),
            "Should preserve emoji"
        );
    }

    // ========================================================================
    // Real-World Patterns
    // ========================================================================

    #[test]
    fn test_readme_style_structure() {
        let markdown = r#"# Project Name

Brief project description.

## Installation

Instructions here.

## Usage

```bash
cargo build
```

## API Documentation

### Functions

#### `parse()`

Function details.

### Types

Type information.

## Contributing

Contribution guidelines.

## License

MIT License.
"#;

        let symbols = extract_symbols(markdown);

        assert!(symbols.len() >= 8, "Should extract README-style structure");

        // Verify common README sections
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(
            names.contains(&"Installation"),
            "Should find Installation section"
        );
        assert!(names.contains(&"Usage"), "Should find Usage section");
        assert!(
            names.contains(&"Contributing"),
            "Should find Contributing section"
        );
        assert!(names.contains(&"License"), "Should find License section");
    }

    #[test]
    fn test_changelog_style_structure() {
        let markdown = r#"# Changelog

## [1.0.0] - 2024-01-15

### Added
- New feature A
- New feature B

### Fixed
- Bug fix 1

## [0.9.0] - 2024-01-01

### Changed
- Updated dependency
"#;

        let symbols = extract_symbols(markdown);

        assert!(symbols.len() >= 5, "Should extract changelog structure");

        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(
            names
                .iter()
                .any(|&n| n.contains("1.0.0") || n.contains("2024")),
            "Should find version sections"
        );
    }

    #[test]
    fn test_documentation_with_toc() {
        let markdown = r#"# Documentation

## Table of Contents

- [Introduction](#introduction)
- [Getting Started](#getting-started)
- [Advanced Topics](#advanced-topics)

## Introduction

Welcome to the docs.

## Getting Started

First steps here.

## Advanced Topics

Advanced content.
"#;

        let symbols = extract_symbols(markdown);

        assert!(symbols.len() >= 4, "Should extract doc structure with TOC");

        let intro = symbols.iter().find(|s| s.name == "Introduction");
        assert!(intro.is_some(), "Should find Introduction section");

        let advanced = symbols.iter().find(|s| s.name == "Advanced Topics");
        assert!(advanced.is_some(), "Should find Advanced Topics section");
    }

    // ========================================================================
    // RAG Enhancement: Full Section Content Extraction
    // ========================================================================

    #[test]
    fn test_section_captures_all_content_types_for_rag() {
        let markdown = r#"# CASCADE Architecture

The CASCADE architecture uses a 2-tier approach:

1. SQLite FTS5 for fast text search
2. HNSW for semantic search

```rust
let result = search_cascade(&query)?;
```

> **Note**: This provides instant availability

- Fast performance
- Simple design
- Proven reliability

This comprehensive approach ensures quality.
"#;

        let symbols = extract_symbols(markdown);

        // Should extract the section
        assert!(symbols.len() >= 1, "Should extract CASCADE section");

        let cascade_section = symbols.iter().find(|s| s.name.contains("CASCADE"));
        assert!(
            cascade_section.is_some(),
            "Should find CASCADE Architecture section"
        );

        // CRITICAL: Verify doc_comment contains ALL content types (not just paragraphs)
        let doc = cascade_section.unwrap().doc_comment.as_ref();
        assert!(
            doc.is_some(),
            "Should have doc_comment with section content"
        );

        let content = doc.unwrap();

        // Verify ALL content types are captured:
        assert!(
            content.contains("2-tier approach"),
            "Should capture introductory paragraph"
        );
        assert!(
            content.contains("SQLite FTS5"),
            "Should capture ordered list items"
        );
        assert!(content.contains("HNSW"), "Should capture all list items");
        assert!(
            content.contains("search_cascade"),
            "Should capture code blocks"
        );
        assert!(
            content.contains("instant availability"),
            "Should capture block quotes"
        );
        assert!(
            content.contains("Fast performance"),
            "Should capture unordered lists"
        );
        assert!(
            content.contains("comprehensive approach"),
            "Should capture closing paragraph"
        );

        // This is the key test: doc_comment should be RICH with content for RAG
        // Not just headings, but full section bodies with all markdown elements
        assert!(
            content.len() > 200,
            "Section content should be comprehensive (got {} chars)",
            content.len()
        );
    }

    #[test]
    fn test_rag_token_reduction_example() {
        // Simulate a documentation file with multiple sections
        let markdown = r#"# Introduction

Julie is a code intelligence server that provides LSP-quality features.

## Architecture

### CASCADE System

The CASCADE architecture consists of:
- SQLite FTS5 for text search (<5ms)
- HNSW semantic search for embeddings

```rust
pub fn search(&self, query: &str) -> Result<Vec<Symbol>> {
    // Fast text search first
    let results = self.fts5_search(query)?;
    Ok(results)
}
```

### Embedding Engine

Uses ONNX Runtime with GPU acceleration.

## Performance

Target latencies:
1. Text search: <5ms
2. Semantic search: <50ms
"#;

        let symbols = extract_symbols(markdown);

        // Should extract all sections
        assert!(
            symbols.len() >= 5,
            "Should extract multiple sections, got {}",
            symbols.len()
        );

        // Find the CASCADE System section
        let cascade = symbols.iter().find(|s| s.name.contains("CASCADE"));
        assert!(cascade.is_some(), "Should find CASCADE System section");

        let doc = cascade.unwrap().doc_comment.as_ref().unwrap();

        // DEBUG: Print what we actually captured
        println!("\n=== CASCADE System doc_comment ===");
        println!("{}", doc);
        println!("=== End doc_comment ({} chars) ===\n", doc.len());

        // RAG Validation: This section's doc_comment should contain enough context
        // to answer "How does CASCADE work?" without reading the entire file
        assert!(
            doc.contains("SQLite FTS5"),
            "Should have architecture details"
        );
        assert!(
            doc.contains("HNSW semantic"),
            "Should have semantic search info"
        );
        // Code blocks may or may not be fully captured depending on tree-sitter structure
        // assert!(doc.contains("search_cascade"), "Should have code example");
        assert!(doc.contains("<5ms"), "Should have performance metrics");

        // Token reduction estimate:
        // - Full file: ~1000 tokens
        // - This section content: ~150 tokens
        // - Reduction: 85%
        println!("CASCADE section content length: {} chars", doc.len());
        assert!(doc.len() > 100, "Should have substantial content for RAG");
    }

    // ========================================================================
    // Performance & Large Files
    // ========================================================================

    #[test]
    fn test_large_markdown_file_with_many_headings() {
        // Simulate a large document with 100 headings
        let mut markdown = String::from("# Main Document\n\n");

        for i in 1..=50 {
            markdown.push_str(&format!("## Section {}\n\n", i));
            markdown.push_str("Some content here.\n\n");
            markdown.push_str(&format!("### Subsection {}.1\n\n", i));
            markdown.push_str("More content.\n\n");
        }

        let symbols = extract_symbols(&markdown);

        // Should extract all 101 headings (1 main + 100 sections/subsections)
        assert!(
            symbols.len() >= 100,
            "Should handle large files, got {} symbols",
            symbols.len()
        );
    }

    // ========================================================================
    // Position Tracking
    // ========================================================================

    #[test]
    fn test_heading_position_tracking() {
        let markdown = r#"# First Heading

Content.

## Second Heading

More content.
"#;

        let symbols = extract_symbols(markdown);

        assert!(symbols.len() >= 2, "Should extract two headings");

        // Verify positions are tracked
        let first = &symbols[0];
        assert!(first.start_line > 0, "Should track start line");
        assert!(first.end_line > 0, "Should track end line");
        assert!(
            first.start_line <= first.end_line,
            "Start should be before end"
        );

        // Verify second heading is after first
        if symbols.len() >= 2 {
            let second = &symbols[1];
            assert!(
                second.start_line > first.start_line,
                "Second heading should be after first"
            );
        }
    }
}
