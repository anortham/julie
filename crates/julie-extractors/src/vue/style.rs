// Vue style section symbol extraction
//
// Responsible for extracting CSS selectors and custom properties from the <style> section
// Supports: class selectors (.name), ID selectors (#name), CSS custom properties (--name)

use super::helpers::{CSS_CLASS_RE, CSS_CUSTOM_PROP_RE, CSS_ID_RE};
use super::parsing::VueSection;
use super::script::{create_symbol_manual, find_doc_comment_before};
use crate::base::BaseExtractor;
use crate::base::SymbolKind;

/// Extract symbols from style section (CSS class names, etc.)
/// Implementation of extractStyleSymbols logic
pub(super) fn extract_style_symbols(
    base: &BaseExtractor,
    section: &VueSection,
) -> Vec<crate::base::Symbol> {
    let mut symbols = Vec::new();
    let lines: Vec<&str> = section.content.lines().collect();

    for (i, line) in lines.iter().enumerate() {
        let actual_line = section.start_line + i;

        // Extract doc comment for this line (look backward from current line)
        let doc_comment = find_doc_comment_before(&lines, i);

        // Extract CSS class selectors (.class-name { })
        for captures in CSS_CLASS_RE.captures_iter(line) {
            if let Some(class_name) = captures.get(1) {
                let name = class_name.as_str();
                let start_col = class_name.start() + 1;
                symbols.push(create_symbol_manual(
                    base,
                    name,
                    SymbolKind::Property,
                    actual_line,
                    start_col,
                    actual_line,
                    start_col + name.len(),
                    Some(format!(".{}", name)),
                    doc_comment.clone(),
                    None,
                ));
            }
        }

        // Extract CSS ID selectors (#id-name { })
        for captures in CSS_ID_RE.captures_iter(line) {
            if let Some(id_name) = captures.get(1) {
                let name = id_name.as_str();
                let start_col = id_name.start();
                symbols.push(create_symbol_manual(
                    base,
                    name,
                    SymbolKind::Property,
                    actual_line,
                    start_col,
                    actual_line,
                    start_col + name.len(),
                    Some(format!("#{}", name)),
                    doc_comment.clone(),
                    None,
                ));
            }
        }

        // Extract CSS custom properties (--var-name: value)
        for captures in CSS_CUSTOM_PROP_RE.captures_iter(line) {
            if let Some(prop_name) = captures.get(1) {
                let name = prop_name.as_str();
                let start_col = prop_name.start();
                symbols.push(create_symbol_manual(
                    base,
                    name,
                    SymbolKind::Variable,
                    actual_line,
                    start_col,
                    actual_line,
                    start_col + name.len(),
                    Some(name.to_string()),
                    doc_comment.clone(),
                    None,
                ));
            }
        }
    }

    symbols
}
