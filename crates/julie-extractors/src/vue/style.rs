// Vue style section symbol extraction
//
// Responsible for extracting CSS selectors and custom properties from the <style> section
// Supports: class selectors (.name), ID selectors (#name), CSS custom properties (--name)

use super::parsing::VueSection;
use crate::base::BaseExtractor;
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
    for symbol in &mut symbols {
        symbol.language = base.language.clone();
        symbol.start_line += section.start_line as u32;
        symbol.end_line += section.start_line as u32;
        symbol.start_byte += byte_offset;
        symbol.end_byte += byte_offset;
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
