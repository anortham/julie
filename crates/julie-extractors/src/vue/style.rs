// Vue style section symbol extraction
//
// Responsible for extracting CSS selectors and custom properties from the <style> section
// Supports: class selectors (.name), ID selectors (#name), CSS custom properties (--name)

use super::parsing::VueSection;
use crate::base::{BaseExtractor, EmbeddedSpanOffset, NormalizedSpan};
use crate::css::CSSExtractor;
use tree_sitter::Parser;

/// Extract symbols from style section (CSS class names, etc.)
/// Implementation of extractStyleSymbols logic
pub(super) fn extract_style_symbols(
    base: &BaseExtractor,
    section: &VueSection,
) -> Vec<crate::base::Symbol> {
    let mut parser = Parser::new();
    if parser
        .set_language(&tree_sitter_css::LANGUAGE.into())
        .is_err()
    {
        return Vec::new();
    }
    let Some(tree) = parser.parse(&section.content, None) else {
        return Vec::new();
    };

    let mut extractor = CSSExtractor::new(
        "css".to_string(),
        base.file_path.clone(),
        section.content.clone(),
        std::path::Path::new(""),
    );
    let mut symbols = extractor.extract_symbols(&tree);
    let byte_offset = section_byte_offset(&base.content, section.start_line);
    let Some(offset) = EmbeddedSpanOffset::from_host_byte(&base.content, byte_offset as usize)
    else {
        return Vec::new();
    };
    for symbol in &mut symbols {
        symbol.language = base.language.clone();
        let span = NormalizedSpan {
            start_line: symbol.start_line,
            start_column: symbol.start_column,
            end_line: symbol.end_line,
            end_column: symbol.end_column,
            start_byte: symbol.start_byte,
            end_byte: symbol.end_byte,
        };
        symbol.apply_normalized_span(offset.apply(span));
        symbol.refresh_id();
    }
    symbols
}

fn section_byte_offset(content: &str, start_line: usize) -> u32 {
    content
        .split_inclusive('\n')
        .take(start_line)
        .map(str::len)
        .sum::<usize>() as u32
}
