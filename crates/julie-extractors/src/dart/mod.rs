// Dart Extractor - Implementation of dart-extractor.ts (TDD GREEN phase)
//
// Direct Implementation of Dart extractor logic to idiomatic Rust.
// Preserves proven extraction strategy while leveraging Rust's safety and performance.
//
// Test parity: All test cases must pass

mod functions;
mod helpers;
mod identifiers;
mod imports;
mod members;
mod pending_calls;
mod relationships;
mod signatures;
mod types;

use crate::base::{
    BaseExtractor, Identifier, PendingRelationship, Relationship, Symbol, SymbolKind,
    SymbolOptions, Visibility,
};
use helpers::{find_child_by_type, get_node_text};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;
use tree_sitter::{Node, Tree};

// Static regex compiled once for performance
static TYPE_SIGNATURE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\w+)\s+\w+").unwrap());

/// Dart language extractor that handles Dart-specific constructs including Flutter
pub struct DartExtractor {
    pub(crate) base: BaseExtractor,
    pending_relationships: Vec<PendingRelationship>,
    same_file_calls: Vec<(String, String, u32)>,
}

impl DartExtractor {
    pub fn new(
        language: String,
        file_path: String,
        content: String,
        workspace_root: &std::path::Path,
    ) -> Self {
        Self {
            base: BaseExtractor::new(language, file_path, content, workspace_root),
            pending_relationships: Vec::new(),
            same_file_calls: Vec::new(),
        }
    }

    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        // WORKAROUND: Set global content cache for get_node_text() helper
        helpers::set_dart_content_cache(&self.base.content);

        let mut symbols = Vec::new();
        self.visit_node(tree.root_node(), &mut symbols, None);
        symbols
    }

    fn visit_node(&mut self, node: Node, symbols: &mut Vec<Symbol>, parent_id: Option<&str>) {
        if node.kind().is_empty() {
            return;
        }

        let mut symbol: Option<Symbol> = None;
        let current_parent_id = parent_id.map(|id| id.to_string());

        match node.kind() {
            "class_definition" => {
                symbol =
                    functions::extract_class(&mut self.base, &node, current_parent_id.as_deref());
            }
            "function_declaration" | "lambda_expression" => {
                symbol = functions::extract_function(
                    &mut self.base,
                    &node,
                    current_parent_id.as_deref(),
                );
            }
            "function_signature" => {
                if let Some(parent) = node.parent() {
                    if parent.kind() != "method_signature" {
                        symbol = if current_parent_id.is_some() {
                            functions::extract_method(
                                &mut self.base,
                                &node,
                                current_parent_id.as_deref(),
                            )
                        } else {
                            functions::extract_function(
                                &mut self.base,
                                &node,
                                current_parent_id.as_deref(),
                            )
                        };
                    }
                }
            }
            "method_signature" | "method_declaration" => {
                symbol =
                    functions::extract_method(&mut self.base, &node, current_parent_id.as_deref());
            }
            "enum_declaration" => {
                symbol = types::extract_enum(&mut self.base, &node, current_parent_id.as_deref());
            }
            "enum_constant" => {
                symbol = types::extract_enum_constant(
                    &mut self.base,
                    &node,
                    current_parent_id.as_deref(),
                );
            }
            "mixin_declaration" => {
                symbol = types::extract_mixin(&mut self.base, &node, current_parent_id.as_deref());
            }
            "extension_declaration" => {
                symbol =
                    types::extract_extension(&mut self.base, &node, current_parent_id.as_deref());
            }
            "constructor_signature"
            | "factory_constructor_signature"
            | "constant_constructor_signature" => {
                symbol = functions::extract_constructor(
                    &mut self.base,
                    &node,
                    current_parent_id.as_deref(),
                );
            }
            "getter_signature" => {
                symbol =
                    members::extract_getter(&mut self.base, &node, current_parent_id.as_deref());
            }
            "setter_signature" => {
                symbol =
                    members::extract_setter(&mut self.base, &node, current_parent_id.as_deref());
            }
            "declaration" => {
                symbol =
                    members::extract_field(&mut self.base, &node, current_parent_id.as_deref());
            }
            "top_level_variable_declaration" | "initialized_variable_definition" => {
                symbol = functions::extract_variable(
                    &mut self.base,
                    &node,
                    current_parent_id.as_deref(),
                );
            }
            "type_alias" => {
                symbol =
                    types::extract_typedef(&mut self.base, &node, current_parent_id.as_deref());
            }
            "import_or_export" => {
                symbol = imports::extract_import_or_export(
                    &mut self.base,
                    &node,
                    current_parent_id.as_deref(),
                );
            }
            "ERROR" | "expression_statement" => {
                // harper-tree-sitter-dart misparsees enhanced enums: the body after the first
                // enum_constant spills into ERROR and expression_statement siblings at
                // program level. Recover symbols generically by detecting enum context.
                if node.parent().map_or(false, |p| p.kind() == "program") {
                    if let Some(enum_id) = find_enum_context_parent(&node, symbols) {
                        recover_enum_symbols_from_error(
                            &mut self.base,
                            &node,
                            Some(&enum_id),
                            symbols,
                        );
                    }
                }
            }
            _ => {}
        }

        // Add symbol if extracted successfully
        let next_parent_id = if let Some(ref sym) = symbol {
            symbols.push(sym.clone());
            Some(sym.id.as_str())
        } else {
            current_parent_id.as_deref()
        };

        // Recursively visit children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit_node(child, symbols, next_parent_id);
        }
    }

    pub fn extract_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship> {
        let mut rels =
            relationships::extract_relationships(&mut self.base, tree.root_node(), symbols);
        self.same_file_calls.clear();
        self.extract_pending_relationships(tree, symbols);

        for (caller_id, callee_id, line_number) in self.same_file_calls.drain(..) {
            rels.push(crate::base::Relationship {
                id: format!(
                    "{}_{}_{:?}_{}",
                    caller_id,
                    callee_id,
                    crate::base::RelationshipKind::Calls,
                    line_number
                ),
                from_symbol_id: caller_id,
                to_symbol_id: callee_id,
                kind: crate::base::RelationshipKind::Calls,
                file_path: self.base.file_path.clone(),
                line_number,
                confidence: 0.9,
                metadata: None,
            });
        }

        rels
    }

    fn extract_pending_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) {
        let symbol_map: HashMap<String, &Symbol> =
            symbols.iter().map(|s| (s.name.clone(), s)).collect();
        self.walk_for_pending_calls(tree.root_node(), &symbol_map);
    }

    pub fn infer_types(&self, symbols: &[Symbol]) -> HashMap<String, String> {
        let mut types = HashMap::new();
        for symbol in symbols {
            if let Some(signature) = &symbol.signature {
                if let Some(captures) = TYPE_SIGNATURE_RE.captures(signature) {
                    if let Some(type_match) = captures.get(1) {
                        types.insert(symbol.name.clone(), type_match.as_str().to_string());
                    }
                }
            }
            if let Some(is_final) = symbol.metadata.as_ref().and_then(|m| m.get("isFinal")) {
                if is_final.as_bool() == Some(true) {
                    types
                        .entry(symbol.name.clone())
                        .or_insert_with(|| "final".to_string());
                }
            }
            if let Some(is_const) = symbol.metadata.as_ref().and_then(|m| m.get("isConst")) {
                if is_const.as_bool() == Some(true) {
                    types
                        .entry(symbol.name.clone())
                        .or_insert_with(|| "const".to_string());
                }
            }
        }
        types
    }

    pub fn extract_identifiers(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Identifier> {
        let symbol_map: HashMap<String, &Symbol> =
            symbols.iter().map(|s| (s.id.clone(), s)).collect();
        identifiers::walk_tree_for_identifiers(&mut self.base, tree.root_node(), &symbol_map);
        self.base.identifiers.clone()
    }

    pub fn get_pending_relationships(&self) -> Vec<PendingRelationship> {
        self.pending_relationships.clone()
    }

    pub fn add_pending_relationship(&mut self, pending: PendingRelationship) {
        self.pending_relationships.push(pending);
    }
}

// === Generic Enhanced Enum Error Recovery ===

/// Walk backward through previous siblings to find an enum_declaration.
/// harper-tree-sitter-dart splits enhanced enum bodies across sibling nodes at
/// the program level, so ERROR / expression_statement nodes that immediately
/// follow an enum_declaration likely contain the rest of that enum's body.
fn find_enum_context_parent(node: &Node, symbols: &[Symbol]) -> Option<String> {
    let mut prev = node.prev_sibling();
    while let Some(sib) = prev {
        match sib.kind() {
            "enum_declaration" => {
                // Find the enum symbol's id
                if let Some(name_node) = find_child_by_type(&sib, "identifier") {
                    let name = get_node_text(&name_node);
                    if let Some(sym) = symbols.iter().find(|s| s.name == name && s.kind == SymbolKind::Enum) {
                        return Some(sym.id.clone());
                    }
                }
                return None;
            }
            // Another ERROR or expression_statement is fine -- keep walking back
            "ERROR" | "expression_statement" | "local_variable_declaration" => {
                prev = sib.prev_sibling();
            }
            // Hit a different top-level declaration -- no enum context
            _ => return None,
        }
    }
    None
}

/// Entry point for recovering symbols from an ERROR / expression_statement node
/// that belongs to an enhanced enum whose body was misparsed.
fn recover_enum_symbols_from_error(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
    symbols: &mut Vec<Symbol>,
) {
    // Collect names already extracted so we skip duplicates
    let already_extracted: HashSet<String> = symbols.iter().map(|s| s.name.clone()).collect();
    recover_from_node_recursive(base, node, parent_id, symbols, &already_extracted);
}

/// Recursively walk a subtree recovering enum members and constructors.
///
/// In the misparsed tree the relevant patterns are:
///   - `member_access` containing an `identifier` child  -->  EnumMember
///   - `const_object_expression` with a `type_identifier` child  -->  Constructor
///   - plain `identifier` that is lowercase  -->  EnumMember (fallback)
///
/// We skip noise nodes like `parenthesized_expression`, `string_literal`,
/// `argument_part`, and `arguments` to avoid extracting garbage.
fn recover_from_node_recursive(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
    symbols: &mut Vec<Symbol>,
    already_extracted: &HashSet<String>,
) {
    match node.kind() {
        "member_access" => {
            // e.g. green('Green') parses as member_access with identifier "green"
            if let Some(id_node) = find_child_by_type(node, "identifier") {
                let name = get_node_text(&id_node);
                if !already_extracted.contains(&name) && looks_like_enum_value(&name) {
                    let sym = base.create_symbol(
                        &id_node,
                        name,
                        SymbolKind::EnumMember,
                        SymbolOptions {
                            visibility: Some(Visibility::Public),
                            parent_id: parent_id.map(|id| id.to_string()),
                            metadata: Some(HashMap::new()),
                            ..Default::default()
                        },
                    );
                    symbols.push(sym);
                }
            }
            return; // Don't recurse into member_access children
        }
        "const_object_expression" => {
            // e.g. `const Color(this.displayName)` parses as const_object_expression
            if let Some(type_node) = find_child_by_type(node, "type_identifier") {
                let name = get_node_text(&type_node);
                // Only create if we don't already have a Constructor with this name
                let has_constructor = symbols
                    .iter()
                    .any(|s| s.name == name && s.kind == SymbolKind::Constructor);
                if !has_constructor {
                    let sym = base.create_symbol(
                        &type_node,
                        name.clone(),
                        SymbolKind::Constructor,
                        SymbolOptions {
                            signature: Some(format!("const {}", name)),
                            visibility: Some(Visibility::Public),
                            parent_id: parent_id.map(|id| id.to_string()),
                            metadata: Some(HashMap::new()),
                            ..Default::default()
                        },
                    );
                    symbols.push(sym);
                }
            }
            return; // Don't recurse into const_object_expression children
        }
        // Skip noise nodes entirely
        "parenthesized_expression" | "string_literal" | "argument_part" | "arguments" => {
            return;
        }
        _ => {}
    }

    // Recurse into children for other node types
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        recover_from_node_recursive(base, &child, parent_id, symbols, already_extracted);
    }
}

/// Check whether an identifier plausibly looks like a Dart enum value.
///
/// Filters out noise that can leak through ERROR recovery:
/// - Private identifiers (start with `_`) are never enum values
/// - Dart keywords/built-ins that appear as identifiers in misparsed trees
/// - Single-character identifiers (loop variables, type params)
fn looks_like_enum_value(name: &str) -> bool {
    // Private identifiers are never enum values
    if name.starts_with('_') {
        return false;
    }

    // Single characters are noise (loop vars, type params)
    if name.len() <= 1 {
        return false;
    }

    // Dart keywords and built-in names that can appear as identifiers in ERROR nodes
    !matches!(
        name,
        "return" | "switch" | "case" | "default" | "throw" | "break" | "continue"
            | "this" | "super" | "if" | "else" | "for" | "while" | "do" | "try" | "catch"
            | "finally" | "new" | "const" | "var" | "final" | "static" | "void" | "null"
            | "true" | "false" | "async" | "await" | "yield" | "get" | "set"
            | "String" | "int" | "double" | "bool" | "List" | "Map" | "Set"
            | "Future" | "Stream" | "dynamic" | "Object" | "num"
    )
}
