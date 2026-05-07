use super::parsing::VueSection;
use super::script::create_symbol_manual;
use crate::base::{BaseExtractor, Symbol, SymbolKind};
use serde_json::Value;
use std::collections::HashMap;

pub(super) fn extract_template_symbols(base: &BaseExtractor, section: &VueSection) -> Vec<Symbol> {
    let section_offset = section_content_offset(&base.content, section.start_line);

    let mut symbols = Vec::new();
    for (line_start, line) in template_lines(&section.content) {
        extract_attribute_symbol(
            base,
            section_offset + line_start,
            line,
            "ref",
            SymbolKind::Variable,
            &mut symbols,
        );
        extract_attribute_symbol(
            base,
            section_offset + line_start,
            line,
            "v-model",
            SymbolKind::Property,
            &mut symbols,
        );
        if line.contains("<slot") {
            extract_attribute_symbol(
                base,
                section_offset + line_start,
                line,
                "name",
                SymbolKind::Event,
                &mut symbols,
            );
        }
    }
    symbols
}

fn template_lines(content: &str) -> impl Iterator<Item = (usize, &str)> {
    content.lines().scan(0usize, |offset, line| {
        let current = *offset;
        *offset += line.len() + 1;
        Some((current, line))
    })
}

fn section_content_offset(content: &str, start_line: usize) -> usize {
    content
        .split_inclusive('\n')
        .take(start_line)
        .map(str::len)
        .sum()
}

fn extract_attribute_symbol(
    base: &BaseExtractor,
    absolute_line_start: usize,
    line: &str,
    attribute: &str,
    kind: SymbolKind,
    symbols: &mut Vec<Symbol>,
) {
    let pattern = format!("{}=\"", attribute);
    let mut search_start = 0usize;

    while let Some(relative) = line[search_start..].find(&pattern) {
        let value_start = search_start + relative + pattern.len();
        let Some(value_end_relative) = line[value_start..].find('"') else {
            break;
        };
        let value_end = value_start + value_end_relative;
        let name = &line[value_start..value_end];
        if !name.is_empty() {
            let start_byte = absolute_line_start + value_start;
            let end_byte = absolute_line_start + value_end;
            if let (Some((start_line, start_col)), Some((end_line, end_col))) = (
                line_column_for_byte(&base.content, start_byte),
                line_column_for_byte(&base.content, end_byte),
            ) {
                let mut metadata = HashMap::new();
                metadata.insert(
                    "type".to_string(),
                    Value::String(format!("template-{}", attribute)),
                );
                symbols.push(create_symbol_manual(
                    base,
                    name,
                    kind.clone(),
                    start_line,
                    start_col,
                    end_line,
                    end_col,
                    Some(format!("{}=\"{}\"", attribute, name)),
                    None,
                    Some(metadata),
                ));
            }
        }
        search_start = value_end.saturating_add(1);
    }
}

fn line_column_for_byte(content: &str, target: usize) -> Option<(usize, usize)> {
    let mut line = 1usize;
    let mut line_start = 0usize;
    for (idx, byte) in content.bytes().enumerate() {
        if idx == target {
            return Some((line, idx - line_start + 1));
        }
        if byte == b'\n' {
            line += 1;
            line_start = idx + 1;
        }
    }
    (target == content.len()).then_some((line, target - line_start + 1))
}
