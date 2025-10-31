// Vue style section symbol extraction
//
// Responsible for extracting CSS class names from the <style> section

use super::helpers::CSS_CLASS_RE;
use super::parsing::VueSection;
use super::script::{create_symbol_manual, find_doc_comment_before};
use crate::extractors::base::BaseExtractor;
use crate::extractors::base::SymbolKind;

/// Extract symbols from style section (CSS class names, etc.)
/// Implementation of extractStyleSymbols logic
pub(super) fn extract_style_symbols(
    base: &BaseExtractor,
    section: &VueSection,
) -> Vec<crate::extractors::base::Symbol> {
    let mut symbols = Vec::new();
    let lines: Vec<&str> = section.content.lines().collect();

    for (i, line) in lines.iter().enumerate() {
        let actual_line = section.start_line + i;

        // Extract doc comment for this line (look backward from current line)
        let doc_comment = find_doc_comment_before(&lines, i);

        // Extract CSS class names - following pattern
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
    }

    symbols
}
