// R Language Extractor Implementation
// R is a statistical computing and graphics language
// Tree-sitter-r parser provides AST nodes for R syntax

mod identifiers;
mod idioms;
mod relationships;
mod text_args;

use crate::base::{BaseExtractor, Identifier, PendingRelationship, Relationship, Symbol};
use crate::base::{SymbolKind, SymbolOptions};
use crate::test_detection::is_test_symbol;
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

/// Common R functions that use dots in their names but are NOT S3 methods.
/// These should be classified as plain functions, not method dispatches.
/// Only includes dot-containing names (non-dot names are already excluded by classify_s3).
const NON_S3_DOT_FUNCTIONS: &[&str] = &[
    // Data structures
    "data.frame",
    "data.table",
    // I/O
    "read.csv",
    "read.table",
    "read.delim",
    "read.fwf",
    "write.csv",
    "write.table",
    // Type checking (is.*)
    "is.na",
    "is.null",
    "is.numeric",
    "is.character",
    "is.logical",
    "is.integer",
    "is.double",
    "is.complex",
    "is.list",
    "is.vector",
    "is.matrix",
    "is.array",
    "is.factor",
    "is.ordered",
    "is.data.frame",
    "is.function",
    "is.environment",
    "is.recursive",
    "is.atomic",
    "is.finite",
    "is.infinite",
    "is.nan",
    "is.element",
    "is.loaded",
    "is.pairlist",
    "is.primitive",
    "is.R",
    // Type coercion (as.*)
    "as.character",
    "as.numeric",
    "as.integer",
    "as.double",
    "as.logical",
    "as.complex",
    "as.factor",
    "as.data.frame",
    "as.matrix",
    "as.list",
    "as.vector",
    "as.Date",
    "as.POSIXct",
    "as.POSIXlt",
    // System/control
    "on.exit",
    "do.call",
    "set.seed",
    // System info (sys.*, Sys.*)
    "sys.call",
    "sys.function",
    "sys.frame",
    "sys.nframe",
    "sys.on.exit",
    "sys.parents",
    "sys.status",
    "Sys.time",
    "Sys.Date",
    "Sys.sleep",
    "Sys.getenv",
    "Sys.setenv",
    "Sys.timezone",
    "Sys.glob",
    // File system
    "file.path",
    "file.exists",
    "file.create",
    "file.remove",
    "file.rename",
    "file.copy",
    "file.info",
    "file.size",
    "file.access",
    "file.choose",
    "file.show",
    "dir.create",
    "dir.exists",
    "list.files",
    "list.dirs",
    // Timing
    "proc.time",
    "system.time",
    "system.file",
    // Environment/scope
    "parent.frame",
    "parent.env",
    "new.env",
    // Utility
    "all.equal",
    "all.names",
    "all.vars",
    "which.min",
    "which.max",
    "seq.int",
    "seq.along",
    "make.names",
    "make.unique",
    "attr.all.equal",
    "match.arg",
    "match.call",
    "match.fun",
    // Modeling
    "model.frame",
    "model.matrix",
    "model.response",
    "drop.terms",
    // Misc
    "base.url",
    "try.catch",
    "with.default",
    "within.data.frame",
    "within.list",
    "body.function",
    "close.connection",
    "open.connection",
];

pub struct RExtractor {
    base: BaseExtractor,
    symbols: Vec<Symbol>,
}

impl RExtractor {
    pub fn new(
        language: String,
        file_path: String,
        content: String,
        workspace_root: &std::path::Path,
    ) -> Self {
        Self {
            base: BaseExtractor::new(language, file_path, content, workspace_root),
            symbols: Vec::new(),
        }
    }

    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let root_node = tree.root_node();
        self.symbols.clear();

        // Build exclusion set once for S3 checking
        let non_s3: std::collections::HashSet<&str> =
            NON_S3_DOT_FUNCTIONS.iter().copied().collect();

        self.traverse_node(root_node, None, &non_s3);

        self.symbols.clone()
    }

    /// Recursively traverse the R AST and extract symbols
    fn traverse_node(
        &mut self,
        node: Node,
        parent_id: Option<String>,
        non_s3: &std::collections::HashSet<&str>,
    ) {
        let current_symbol: Option<Symbol> = match node.kind() {
            "binary_operator" => self.extract_from_binary_op(node, &parent_id, non_s3),
            "call" => self.extract_from_call(node, &parent_id),
            _ => None,
        };

        // Recursively traverse children
        let next_parent_id = current_symbol.as_ref().map(|s| s.id.clone()).or(parent_id);
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.traverse_node(child, next_parent_id.clone(), non_s3);
        }
    }

    /// Handle binary_operator nodes (assignments)
    fn extract_from_binary_op(
        &mut self,
        node: Node,
        parent_id: &Option<String>,
        non_s3: &std::collections::HashSet<&str>,
    ) -> Option<Symbol> {
        let operator = node.child(1)?;
        let op_text = self.base.get_node_text(&operator);

        match op_text.as_str() {
            // Left-to-right assignment: x <- value, x = value, x <<- value
            "<-" | "=" | "<<-" => {
                let left = node.child(0)?;
                let name = idioms::assignment_name(self, left)?;
                if name.contains('(') {
                    return None;
                }
                let right = node.child(2)?;

                if let Some(symbol) =
                    idioms::extract_assignment_class_factory(self, node, &name, right, parent_id)
                {
                    Some(symbol)
                } else if right.kind() == "function_definition" {
                    Some(self.extract_function_assignment(node, name, right, parent_id, non_s3))
                } else if idioms::is_container_assignment(self, left, right) {
                    None
                } else {
                    let mut metadata = idioms::member_metadata(self, node, parent_id);
                    let options = SymbolOptions {
                        parent_id: parent_id.clone(),
                        doc_comment: self.base.find_doc_comment(&node),
                        metadata: if metadata.is_empty() {
                            None
                        } else {
                            Some(std::mem::take(&mut metadata))
                        },
                        ..Default::default()
                    };
                    let kind = if idioms::member_metadata(self, node, parent_id)
                        .get("member_visibility")
                        .is_some()
                    {
                        SymbolKind::Field
                    } else {
                        SymbolKind::Variable
                    };
                    let symbol = self.base.create_symbol(&node, name, kind, options);
                    self.symbols.push(symbol.clone());
                    Some(symbol)
                }
            }
            // Right-to-left assignment: value -> x, value ->> x
            "->" | "->>" => {
                let right = node.child(2)?;
                if right.kind() != "identifier" {
                    return None;
                }
                let name = self.base.get_node_text(&right);
                let options = SymbolOptions {
                    parent_id: parent_id.clone(),
                    doc_comment: self.base.find_doc_comment(&node),
                    ..Default::default()
                };
                let symbol = self
                    .base
                    .create_symbol(&node, name, SymbolKind::Variable, options);
                self.symbols.push(symbol.clone());
                Some(symbol)
            }
            _ => None,
        }
    }

    /// Extract a function assignment with proper signature, S3 detection, and UseMethod detection
    fn extract_function_assignment(
        &mut self,
        node: Node,
        name: String,
        func_def: Node,
        parent_id: &Option<String>,
        non_s3: &std::collections::HashSet<&str>,
    ) -> Symbol {
        let signature = self.build_function_signature(&name, func_def);
        let mut metadata: HashMap<String, serde_json::Value> = HashMap::new();

        // Detect S3 method pattern: method.class (but not common dot-functions)
        let (kind, s3_detected) = self.classify_s3(&name, non_s3);

        if s3_detected {
            if let Some(dot_pos) = name.find('.') {
                let method_name = &name[..dot_pos];
                let class_name = &name[dot_pos + 1..];
                metadata.insert(
                    "s3_method".to_string(),
                    serde_json::Value::String(method_name.to_string()),
                );
                metadata.insert(
                    "s3_class".to_string(),
                    serde_json::Value::String(class_name.to_string()),
                );
            }
        }

        // Check for UseMethod() in body -> mark as S3 generic
        if self.body_contains_usemethod(func_def) {
            metadata.insert("s3_generic".to_string(), serde_json::Value::Bool(true));
        }

        metadata.extend(idioms::member_metadata(self, node, parent_id));

        // Test detection
        if is_test_symbol("r", &name, &self.base.file_path, &kind, &[], None) {
            metadata.insert("is_test".to_string(), serde_json::Value::Bool(true));
        }

        let options = SymbolOptions {
            parent_id: parent_id.clone(),
            signature: Some(signature),
            metadata: if metadata.is_empty() {
                None
            } else {
                Some(metadata)
            },
            doc_comment: self.base.find_doc_comment(&node),
            ..Default::default()
        };
        let symbol = self.base.create_symbol(&node, name, kind, options);
        self.symbols.push(symbol.clone());
        symbol
    }

    /// Classify whether a function name is an S3 method or a plain function
    fn classify_s3(
        &self,
        name: &str,
        non_s3: &std::collections::HashSet<&str>,
    ) -> (SymbolKind, bool) {
        if !name.contains('.') || non_s3.contains(name) {
            return (SymbolKind::Function, false);
        }
        // Has a dot and is not in the exclusion list -> S3 method
        (SymbolKind::Method, true)
    }

    /// Build a function signature like `name <- function(x, y = 0)`
    fn build_function_signature(&self, name: &str, func_def: Node) -> String {
        let params = self.extract_parameters(func_def);
        format!("{} <- function({})", name, params)
    }

    /// Extract parameter list from a function_definition node
    fn extract_parameters(&self, func_def: Node) -> String {
        // The parameters node is a named field "parameters" on function_definition
        let params_node = match func_def.child_by_field_name("parameters") {
            Some(n) => n,
            None => {
                // Fall back: walk children looking for "formal_parameters"
                let mut found = None;
                let mut cursor = func_def.walk();
                for child in func_def.children(&mut cursor) {
                    if child.kind() == "formal_parameters" || child.kind() == "parameters" {
                        found = Some(child);
                        break;
                    }
                }
                match found {
                    Some(n) => n,
                    None => return String::new(),
                }
            }
        };

        let mut params = Vec::new();
        let mut cursor = params_node.walk();
        for child in params_node.children(&mut cursor) {
            match child.kind() {
                "parameter" | "default_parameter" => {
                    let param_text = self.format_parameter(child);
                    if !param_text.is_empty() {
                        params.push(param_text);
                    }
                }
                "dots" | "..." => {
                    params.push("...".to_string());
                }
                "identifier" => {
                    // Bare identifier as parameter (tree-sitter-r sometimes uses this)
                    let text = self.base.get_node_text(&child);
                    if !text.is_empty() {
                        params.push(text);
                    }
                }
                _ => {}
            }
        }
        params.join(", ")
    }

    /// Format a single parameter, truncating long defaults
    fn format_parameter(&self, param_node: Node) -> String {
        let full_text = self.base.get_node_text(&param_node);

        // Check if there's a default value (contains '=')
        if let Some(eq_pos) = full_text.find('=') {
            let param_name = full_text[..eq_pos].trim();
            let default_val = full_text[eq_pos + 1..].trim();

            if default_val.len() > 30 {
                format!("{} = {}...", param_name, &default_val[..30])
            } else {
                full_text.to_string()
            }
        } else {
            full_text.to_string()
        }
    }

    /// Check if a function body contains UseMethod("...")
    fn body_contains_usemethod(&self, func_def: Node) -> bool {
        let body = match func_def.child_by_field_name("body") {
            Some(b) => b,
            None => {
                // Fall back: last child is usually the body
                let count = func_def.child_count();
                if count == 0 {
                    return false;
                }
                match func_def.child((count - 1) as u32) {
                    Some(b) => b,
                    None => return false,
                }
            }
        };
        let body_text = self.base.get_node_text(&body);
        body_text.contains("UseMethod(")
    }

    fn extract_from_call(&mut self, node: Node, parent_id: &Option<String>) -> Option<Symbol> {
        idioms::extract_s4_call(self, node, parent_id)
            .or_else(|| idioms::extract_import_call(self, node, parent_id))
    }

    pub fn extract_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship> {
        relationships::extract_relationships(self, tree, symbols)
    }

    pub fn extract_identifiers(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Identifier> {
        identifiers::extract_identifiers(self, tree, symbols)
    }

    // ========================================================================
    // Pending Relationship Management
    // ========================================================================

    pub(crate) fn add_structured_pending_relationship(
        &mut self,
        pending: crate::base::StructuredPendingRelationship,
    ) {
        self.base.add_structured_pending_relationship(pending);
    }

    /// Get all pending relationships collected during extraction
    pub fn get_pending_relationships(&self) -> Vec<PendingRelationship> {
        self.base.get_pending_relationships()
    }

    pub fn get_structured_pending_relationships(
        &self,
    ) -> Vec<crate::base::StructuredPendingRelationship> {
        self.base.get_structured_pending_relationships()
    }
}
