// Vue <script setup> symbol extraction using tree-sitter
//
// Handles Vue 3 Composition API patterns in <script setup> blocks:
// - ref(), reactive(), computed() variable declarations
// - Function declarations and arrow functions
// - Import statements
// - defineProps(), defineEmits(), defineExpose() macros

use super::parsing::VueSection;
use super::script::create_symbol_manual;
use crate::base::{BaseExtractor, Symbol, SymbolKind};
use serde_json::Value;
use std::collections::HashMap;
use tree_sitter::{Node, Parser};

/// Extract symbols from a <script setup> section using tree-sitter
pub(super) fn extract_script_setup_symbols(
    base: &BaseExtractor,
    section: &VueSection,
) -> Vec<Symbol> {
    let tree = match parse_script_section(section) {
        Some(t) => t,
        None => return Vec::new(),
    };

    let mut symbols = Vec::new();
    let root = tree.root_node();

    walk_for_symbols(base, root, section, &mut symbols);

    symbols
}

/// Parse the script section content with the appropriate tree-sitter parser
fn parse_script_section(section: &VueSection) -> Option<tree_sitter::Tree> {
    let mut parser = Parser::new();

    let lang = section.lang.as_deref().unwrap_or("js");
    let ts_lang = if lang == "ts" || lang == "typescript" {
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
    } else {
        tree_sitter_javascript::LANGUAGE.into()
    };

    parser.set_language(&ts_lang).ok()?;
    parser.parse(&section.content, None)
}

/// Recursively walk the AST and extract symbols
fn walk_for_symbols(
    base: &BaseExtractor,
    node: Node,
    section: &VueSection,
    symbols: &mut Vec<Symbol>,
) {
    match node.kind() {
        "function_declaration" => {
            if let Some(sym) = extract_function_declaration(base, node, section) {
                symbols.push(sym);
            }
        }
        "lexical_declaration" => {
            // const x = ref(0), const fn = () => {}, const props = defineProps(...)
            extract_lexical_declaration(base, node, section, symbols);
        }
        "import_statement" => {
            extract_import_statement(base, node, section, symbols);
        }
        "expression_statement" => {
            // Standalone defineExpose() calls
            extract_standalone_call(base, node, section, symbols);
        }
        _ => {}
    }

    // Recurse into children, but skip nodes we've already fully handled
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            // Don't recurse into things we handle at the top level
            "function_declaration" | "lexical_declaration" | "import_statement"
            | "expression_statement" => {
                // Only recurse at top level (program node)
                if node.kind() == "program" {
                    walk_for_symbols(base, child, section, symbols);
                }
            }
            _ => {
                walk_for_symbols(base, child, section, symbols);
            }
        }
    }
}

/// Extract a function declaration: `function handleClick() { ... }`
fn extract_function_declaration(
    base: &BaseExtractor,
    node: Node,
    section: &VueSection,
) -> Option<Symbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = get_node_text(&name_node, &section.content);

    let params_node = node.child_by_field_name("parameters");
    let params_text = params_node
        .map(|p| get_node_text(&p, &section.content))
        .unwrap_or_else(|| "()".to_string());

    let start_line = section.start_line + name_node.start_position().row;
    let start_col = name_node.start_position().column + 1;
    let end_line = section.start_line + node.end_position().row;
    let end_col = node.end_position().column + 1;

    let mut metadata = HashMap::new();
    metadata.insert("type".to_string(), Value::String("function".to_string()));

    Some(create_symbol_manual(
        base,
        &name,
        SymbolKind::Function,
        start_line,
        start_col,
        end_line,
        end_col,
        Some(format!("function {}{}", name, params_text)),
        None,
        Some(metadata),
    ))
}

/// Extract symbols from a lexical declaration: `const x = ref(0)`, `const fn = () => {}`
fn extract_lexical_declaration(
    base: &BaseExtractor,
    node: Node,
    section: &VueSection,
    symbols: &mut Vec<Symbol>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "variable_declarator" {
            if let Some(sym) = extract_variable_declarator(base, child, section) {
                symbols.push(sym);
            }
        }
    }
}

/// Extract a symbol from a variable declarator node
fn extract_variable_declarator(
    base: &BaseExtractor,
    node: Node,
    section: &VueSection,
) -> Option<Symbol> {
    let name_node = node.child_by_field_name("name")?;
    let name = get_node_text(&name_node, &section.content);
    let value_node = node.child_by_field_name("value")?;

    let start_line = section.start_line + name_node.start_position().row;
    let start_col = name_node.start_position().column + 1;
    let end_line = section.start_line + node.end_position().row;
    let end_col = node.end_position().column + 1;

    // Check if the value is an arrow function or function expression
    let is_function = matches!(
        value_node.kind(),
        "arrow_function" | "function_expression" | "function"
    );

    if is_function {
        // Arrow function or function expression assigned to a const
        let params = value_node
            .child_by_field_name("parameters")
            .or_else(|| value_node.child_by_field_name("parameter"))
            .map(|p| get_node_text(&p, &section.content))
            .unwrap_or_else(|| "()".to_string());

        let mut metadata = HashMap::new();
        metadata.insert("type".to_string(), Value::String("function".to_string()));
        metadata.insert(
            "isArrowFunction".to_string(),
            Value::Bool(value_node.kind() == "arrow_function"),
        );

        Some(create_symbol_manual(
            base,
            &name,
            SymbolKind::Function,
            start_line,
            start_col,
            end_line,
            end_col,
            Some(format!("const {} = {}", name, params)),
            None,
            Some(metadata),
        ))
    } else {
        // Variable declaration (ref, reactive, computed, defineProps, etc.)
        let mut metadata = HashMap::new();

        // Check if the value is a call_expression to classify it
        if value_node.kind() == "call_expression" {
            if let Some(callee) = value_node.child_by_field_name("function") {
                let callee_name = get_node_text(&callee, &section.content);
                metadata.insert(
                    "compositionApi".to_string(),
                    Value::String(callee_name.clone()),
                );

                // Classify based on the call
                let var_type = match callee_name.as_str() {
                    "ref" => "ref",
                    "reactive" => "reactive",
                    "computed" => "computed",
                    "defineProps" => "props",
                    "defineEmits" => "emits",
                    _ => "variable",
                };
                metadata.insert("type".to_string(), Value::String(var_type.to_string()));
            }
        }

        Some(create_symbol_manual(
            base,
            &name,
            SymbolKind::Variable,
            start_line,
            start_col,
            end_line,
            end_col,
            Some(format!("const {}", name)),
            None,
            Some(metadata),
        ))
    }
}

/// Extract import statements: `import { ref, computed } from 'vue'`
fn extract_import_statement(
    base: &BaseExtractor,
    node: Node,
    section: &VueSection,
    symbols: &mut Vec<Symbol>,
) {
    let source_node = node.child_by_field_name("source");
    let source = source_node
        .map(|s| get_node_text(&s, &section.content))
        .unwrap_or_default();

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "import_clause" => {
                // Walk into import clause for named imports, default imports, etc.
                let mut clause_cursor = child.walk();
                for clause_child in child.children(&mut clause_cursor) {
                    match clause_child.kind() {
                        "identifier" => {
                            // Default import: `import MyComponent from './MyComponent.vue'`
                            let name = get_node_text(&clause_child, &section.content);
                            let start_line =
                                section.start_line + clause_child.start_position().row;
                            let start_col = clause_child.start_position().column + 1;

                            let mut metadata = HashMap::new();
                            metadata.insert(
                                "source".to_string(),
                                Value::String(source.clone()),
                            );
                            metadata.insert(
                                "importKind".to_string(),
                                Value::String("default".to_string()),
                            );

                            symbols.push(create_symbol_manual(
                                base,
                                &name,
                                SymbolKind::Import,
                                start_line,
                                start_col,
                                start_line,
                                start_col + name.len(),
                                Some(format!("import {} from {}", name, source)),
                                None,
                                Some(metadata),
                            ));
                        }
                        "named_imports" => {
                            extract_named_imports(
                                base,
                                clause_child,
                                section,
                                &source,
                                symbols,
                            );
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
}

/// Extract individual named imports from `{ ref, computed, watch }`
fn extract_named_imports(
    base: &BaseExtractor,
    node: Node,
    section: &VueSection,
    source: &str,
    symbols: &mut Vec<Symbol>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "import_specifier" {
            // Named import: could be `ref` or `ref as myRef`
            let name_node = child
                .child_by_field_name("alias")
                .or_else(|| child.child_by_field_name("name"));

            if let Some(name_node) = name_node {
                let name = get_node_text(&name_node, &section.content);
                let start_line = section.start_line + name_node.start_position().row;
                let start_col = name_node.start_position().column + 1;

                let mut metadata = HashMap::new();
                metadata.insert("source".to_string(), Value::String(source.to_string()));
                metadata.insert(
                    "importKind".to_string(),
                    Value::String("named".to_string()),
                );

                symbols.push(create_symbol_manual(
                    base,
                    &name,
                    SymbolKind::Import,
                    start_line,
                    start_col,
                    start_line,
                    start_col + name.len(),
                    Some(format!("import {{ {} }} from {}", name, source)),
                    None,
                    Some(metadata),
                ));
            }
        }
    }
}

/// Extract standalone calls like `defineExpose({ ... })`
fn extract_standalone_call(
    base: &BaseExtractor,
    node: Node,
    section: &VueSection,
    symbols: &mut Vec<Symbol>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "call_expression" {
            if let Some(callee) = child.child_by_field_name("function") {
                let callee_name = get_node_text(&callee, &section.content);
                // Only extract Vue compiler macros as standalone symbols
                if matches!(
                    callee_name.as_str(),
                    "defineExpose" | "defineProps" | "defineEmits" | "defineOptions"
                ) {
                    let start_line = section.start_line + callee.start_position().row;
                    let start_col = callee.start_position().column + 1;
                    let end_line = section.start_line + child.end_position().row;
                    let end_col = child.end_position().column + 1;

                    let mut metadata = HashMap::new();
                    metadata.insert(
                        "type".to_string(),
                        Value::String("vue-macro".to_string()),
                    );

                    symbols.push(create_symbol_manual(
                        base,
                        &callee_name,
                        SymbolKind::Function,
                        start_line,
                        start_col,
                        end_line,
                        end_col,
                        Some(format!("{}()", callee_name)),
                        None,
                        Some(metadata),
                    ));
                }
            }
        }
    }
}

/// Get text content from a tree-sitter node using the section content
fn get_node_text(node: &Node, content: &str) -> String {
    let start = node.start_byte();
    let end = node.end_byte();
    if end <= content.len() {
        content[start..end].to_string()
    } else {
        String::new()
    }
}
