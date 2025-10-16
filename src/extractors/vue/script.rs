// Vue script section symbol extraction
//
// Responsible for extracting Vue component options from the <script> section
// Handles data(), methods, computed, props, and function definitions

use crate::extractors::base::{BaseExtractor, Symbol, SymbolKind};
use serde_json::Value;
use std::collections::HashMap;
use super::helpers::{DATA_FUNCTION_RE, METHODS_OBJECT_RE, COMPUTED_OBJECT_RE, PROPS_OBJECT_RE, FUNCTION_DEF_RE};
use super::parsing::VueSection;

/// Extract symbols from script section
pub(super) fn extract_script_symbols(base: &BaseExtractor, section: &VueSection) -> Vec<Symbol> {
    let mut symbols = Vec::new();
    let lines: Vec<&str> = section.content.lines().collect();

    for (i, line) in lines.iter().enumerate() {
        let actual_line = section.start_line + i;

        // Extract Vue component options - following Miller's patterns
        if DATA_FUNCTION_RE.is_match(line) {
            symbols.push(create_symbol_manual(
                base,
                "data",
                SymbolKind::Function,
                actual_line,
                1,
                actual_line,
                5,
                Some("data()".to_string()),
                Some("Vue component data".to_string()),
                None,
            ));
        }

        if METHODS_OBJECT_RE.is_match(line) {
            symbols.push(create_symbol_manual(
                base,
                "methods",
                SymbolKind::Property,
                actual_line,
                1,
                actual_line,
                8,
                Some("methods: {}".to_string()),
                Some("Vue component methods".to_string()),
                None,
            ));
        }

        if COMPUTED_OBJECT_RE.is_match(line) {
            symbols.push(create_symbol_manual(
                base,
                "computed",
                SymbolKind::Property,
                actual_line,
                1,
                actual_line,
                9,
                Some("computed: {}".to_string()),
                Some("Vue computed properties".to_string()),
                None,
            ));
        }

        if PROPS_OBJECT_RE.is_match(line) {
            symbols.push(create_symbol_manual(
                base,
                "props",
                SymbolKind::Property,
                actual_line,
                1,
                actual_line,
                6,
                Some("props: {}".to_string()),
                Some("Vue component props".to_string()),
                None,
            ));
        }

        // Extract function definitions - following Miller's pattern
        if let Some(captures) = FUNCTION_DEF_RE.captures(line) {
            if let Some(func_name) = captures.get(1) {
                let name = func_name.as_str();
                let start_col = line.find(name).unwrap_or(0) + 1;
                symbols.push(create_symbol_manual(
                    base,
                    name,
                    SymbolKind::Method,
                    actual_line,
                    start_col,
                    actual_line,
                    start_col + name.len(),
                    Some(format!("{}()", name)),
                    Some("Vue component method".to_string()),
                    None,
                ));
            }
        }
    }

    symbols
}

/// Helper to create symbols manually (without Parser.SyntaxNode)
/// Port of Miller's createSymbolManual logic
#[allow(clippy::too_many_arguments)] // Matches Miller's API for compatibility
pub(super) fn create_symbol_manual(
    base: &BaseExtractor,
    name: &str,
    kind: SymbolKind,
    start_line: usize,
    start_column: usize,
    end_line: usize,
    end_column: usize,
    signature: Option<String>,
    documentation: Option<String>,
    metadata: Option<HashMap<String, Value>>,
) -> Symbol {
    use crate::extractors::base::{SymbolOptions, Visibility};

    let options = SymbolOptions {
        signature,
        doc_comment: documentation,
        visibility: Some(Visibility::Public),
        parent_id: None,
        metadata,
    };

    // Generate ID similar to Miller's approach
    let id = format!("{}:{}:{}", name, start_line, start_column);

    Symbol {
        id,
        name: name.to_string(),
        kind,
        language: base.language.clone(),
        file_path: base.file_path.clone(),
        start_line: start_line as u32,
        start_column: start_column as u32,
        end_line: end_line as u32,
        end_column: end_column as u32,
        start_byte: 0, // Not available without tree-sitter node
        end_byte: 0,   // Not available without tree-sitter node
        signature: options.signature,
        doc_comment: options.doc_comment,
        visibility: options.visibility,
        parent_id: options.parent_id,
        metadata: Some(options.metadata.unwrap_or_default()),
        semantic_group: None, // Vue components don't have cross-language groups yet
        confidence: None,     // Will be set during validation
        code_context: None,   // Will be populated during context extraction
    }
}
