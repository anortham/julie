// Vue template section symbol extraction
//
// Responsible for extracting component usage and directives from the <template> section

use super::helpers::{COMPONENT_USAGE_RE, DIRECTIVE_USAGE_RE};
use super::parsing::VueSection;
use super::script::{create_symbol_manual, find_doc_comment_before};
use crate::extractors::base::BaseExtractor;
use crate::extractors::base::SymbolKind;

/// Extract symbols from template section (component usage, directives, etc.)
/// Implementation of extractTemplateSymbols logic
pub(super) fn extract_template_symbols(
    base: &BaseExtractor,
    section: &VueSection,
) -> Vec<crate::extractors::base::Symbol> {
    let mut symbols = Vec::new();
    let lines: Vec<&str> = section.content.lines().collect();

    for (i, line) in lines.iter().enumerate() {
        let actual_line = section.start_line + i;

        // Extract doc comment for this line (look backward from current line)
        let doc_comment = find_doc_comment_before(&lines, i);

        // Extract component usage - following pattern
        for captures in COMPONENT_USAGE_RE.captures_iter(line) {
            if let Some(component_name) = captures.get(1) {
                let name = component_name.as_str();
                let start_col = component_name.start() + 1;
                symbols.push(create_symbol_manual(
                    base,
                    name,
                    SymbolKind::Class,
                    actual_line,
                    start_col,
                    actual_line,
                    start_col + name.len(),
                    Some(format!("<{}>", name)),
                    doc_comment.clone(),
                    None,
                ));
            }
        }

        // Extract directives - following pattern
        for captures in DIRECTIVE_USAGE_RE.captures_iter(line) {
            if let Some(directive_name) = captures.get(1) {
                let name = directive_name.as_str();
                let start_col = directive_name.start() + 1;
                symbols.push(create_symbol_manual(
                    base,
                    name,
                    SymbolKind::Property,
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
