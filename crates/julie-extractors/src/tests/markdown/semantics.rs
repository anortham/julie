use crate::base::{Symbol, SymbolKind};
use crate::markdown::MarkdownExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn extract_symbols(code: &str) -> Vec<Symbol> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_md::LANGUAGE.into())
        .expect("Error loading Markdown grammar");
    let tree = parser.parse(code, None).expect("Failed to parse code");
    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = MarkdownExtractor::new(
        "markdown".to_string(),
        "docs/guide.md".to_string(),
        code.to_string(),
        &workspace_root,
    );
    extractor.extract_symbols(&tree)
}

fn metadata_str<'a>(symbol: &'a Symbol, key: &str) -> Option<&'a str> {
    symbol
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get(key))
        .and_then(|value| value.as_str())
}

fn metadata_u64(symbol: &Symbol, key: &str) -> Option<u64> {
    symbol
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get(key))
        .and_then(|value| value.as_u64())
}

#[test]
fn test_markdown_heading_level_is_preserved_in_metadata() {
    let symbols = extract_symbols(
        r#"# Overview

### Details
"#,
    );

    let overview = symbols
        .iter()
        .find(|symbol| symbol.name == "Overview")
        .expect("Overview heading should be extracted");
    let details = symbols
        .iter()
        .find(|symbol| symbol.name == "Details")
        .expect("Details heading should be extracted");

    assert_eq!(metadata_u64(overview, "heading_level"), Some(1));
    assert_eq!(metadata_u64(details, "heading_level"), Some(3));
}

#[test]
fn test_markdown_heading_fallback_preserves_csharp_heading_text() {
    let symbols = extract_symbols("# C# Programming\n\nText.\n");
    let heading = symbols
        .iter()
        .find(|symbol| symbol.kind == SymbolKind::Module)
        .expect("heading should be extracted");

    assert_eq!(heading.name, "C# Programming");
}

#[test]
fn test_markdown_links_footnotes_and_code_blocks_are_extracted() {
    let symbols = extract_symbols(
        r#"# Resources

See [Julie](https://example.com) and the [Guide][guide].
Remember the setup note[^setup].

[guide]: https://guide.example "Guide"
[^setup]: Install dependencies first.

```rust
fn main() {}
```
"#,
    );

    let inline_link = symbols
        .iter()
        .find(|symbol| {
            symbol.kind == SymbolKind::Import
                && metadata_str(symbol, "markdown_kind") == Some("inline_link")
                && symbol.name == "Julie"
        })
        .expect("inline link should be extracted");
    assert_eq!(
        metadata_str(inline_link, "destination"),
        Some("https://example.com")
    );

    let reference_definition = symbols
        .iter()
        .find(|symbol| {
            symbol.kind == SymbolKind::Import
                && metadata_str(symbol, "markdown_kind") == Some("link_reference_definition")
                && symbol.name == "guide"
        })
        .expect("reference definition should be extracted");
    assert_eq!(
        metadata_str(reference_definition, "destination"),
        Some("https://guide.example")
    );

    let footnote = symbols
        .iter()
        .find(|symbol| {
            symbol.kind == SymbolKind::Property
                && metadata_str(symbol, "markdown_kind") == Some("footnote_definition")
                && symbol.name == "setup"
        })
        .expect("footnote definition should be extracted");
    assert!(
        footnote
            .doc_comment
            .as_deref()
            .is_some_and(|doc| doc.contains("Install dependencies first"))
    );

    let code_block = symbols
        .iter()
        .find(|symbol| {
            symbol.kind == SymbolKind::Property
                && metadata_str(symbol, "markdown_kind") == Some("code_block")
        })
        .expect("fenced code block should be extracted");
    assert_eq!(metadata_str(code_block, "language"), Some("rust"));
    assert!(
        code_block
            .doc_comment
            .as_deref()
            .is_some_and(|doc| doc.contains("fn main()"))
    );
}
