pub(crate) mod assignments;
/// Python extractor for extracting symbols and relationships from Python source code
/// Implementation of Python extractor with comprehensive Python feature support
///
/// This module is organized into focused sub-modules:
/// - helpers: Shared utility functions
/// - types: Class, enum, dataclass extraction
/// - functions: Function and method extraction
/// - signatures: Parameter and type hint extraction
/// - decorators: Decorator extraction and handling
/// - imports: Import statement handling
/// - assignments: Variable and constant assignment extraction
/// - relationships: Inheritance and call relationship extraction
/// - identifiers: LSP identifier tracking for references
pub(crate) mod decorators;
pub(crate) mod functions;
pub(crate) mod helpers;
pub(crate) mod identifiers;
pub(crate) mod imports;
pub(crate) mod relationships;
pub(crate) mod signatures;
pub(crate) mod types;

use crate::base::{
    BaseExtractor, Identifier, Relationship, StructuredPendingRelationship, Symbol, SymbolKind,
};
use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;
use tree_sitter::{Node, Tree};

/// Matches return type hint at end of signature: `: ReturnType`
static RETURN_HINT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":\s*([^=\s]+)\s*$").unwrap());

/// Matches type annotation with default value: `: Type =`
static VAR_ANNOTATION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r":\s*([^=]+)\s*=").unwrap());

// All public API is through PythonExtractor methods
// Internal functions are used via module paths within the parent module

/// Python extractor for extracting symbols and relationships from Python source code
pub struct PythonExtractor {
    base: BaseExtractor,
}

impl PythonExtractor {
    pub fn new(file_path: String, content: String, workspace_root: &std::path::Path) -> Self {
        Self {
            base: BaseExtractor::new("python".to_string(), file_path, content, workspace_root),
        }
    }

    /// Extract all symbols from Python source code
    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        self.traverse_tree(tree.root_node(), &mut symbols);
        symbols
    }

    fn traverse_tree(&mut self, node: Node, symbols: &mut Vec<Symbol>) {
        match node.kind() {
            "class_definition" => {
                if let Some(symbol) = types::extract_class(self, node) {
                    symbols.push(symbol);
                }
            }
            "function_definition" => {
                if let Some(symbol) = functions::extract_function(self, node) {
                    symbols.push(symbol);
                }
            }
            "async_function_definition" => {
                if let Some(symbol) = functions::extract_async_function(self, node) {
                    symbols.push(symbol);
                }
            }
            "assignment" => {
                // Can produce multiple symbols for tuple unpacking (a, b = 1, 2)
                let assignment_symbols = assignments::extract_assignment(self, node);
                symbols.extend(assignment_symbols);
            }
            "import_statement" | "import_from_statement" => {
                let import_symbols = imports::extract_imports(self, node);
                symbols.extend(import_symbols);
            }
            "lambda" => {
                let symbol = functions::extract_lambda(self, node);
                symbols.push(symbol);
            }
            _ => {}
        }

        // Recursively traverse children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.traverse_tree(child, symbols);
        }
    }

    /// Extract relationships from Python code
    pub fn extract_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship> {
        relationships::extract_relationships(self, tree, symbols)
    }

    /// Infer types from Python type annotations and assignments
    pub fn infer_types(&self, symbols: &[Symbol]) -> HashMap<String, String> {
        let mut type_map = HashMap::new();

        for symbol in symbols {
            if matches!(symbol.kind, SymbolKind::Function | SymbolKind::Method) {
                if let Some(return_type) = metadata_return_type(symbol) {
                    type_map.insert(symbol.id.clone(), return_type);
                    continue;
                }
            }

            // Infer types from Python-specific patterns
            if let Some(ref signature) = symbol.signature {
                if let Some(inferred_type) = self.infer_type_from_signature(signature, &symbol.kind)
                {
                    type_map.insert(symbol.id.clone(), inferred_type);
                }
            }
        }

        type_map
    }

    fn infer_type_from_signature(&self, signature: &str, kind: &SymbolKind) -> Option<String> {
        match kind {
            SymbolKind::Function | SymbolKind::Method => {
                // Extract type hints from function signatures
                if let Some(captures) = RETURN_HINT_RE.captures(signature) {
                    return Some(captures[1].to_string());
                }
            }
            SymbolKind::Variable | SymbolKind::Property => {
                // Extract type from variable annotations
                if let Some(captures) = VAR_ANNOTATION_RE.captures(signature) {
                    return Some(captures[1].trim().to_string());
                }
            }
            _ => {}
        }

        None
    }

    /// Extract all identifier usages (function calls, member access, etc.)
    /// Following the Rust extractor reference implementation pattern
    pub fn extract_identifiers(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Identifier> {
        identifiers::extract_identifiers(self, tree, symbols)
    }

    // ========================================================================
    // Accessors for sub-modules
    // ========================================================================

    pub(crate) fn base(&self) -> &BaseExtractor {
        &self.base
    }

    pub(crate) fn base_mut(&mut self) -> &mut BaseExtractor {
        &mut self.base
    }

    // ========================================================================
    // Pending Relationship Management
    // ========================================================================

    pub(crate) fn add_structured_pending_relationship(
        &mut self,
        pending: StructuredPendingRelationship,
    ) {
        self.base.add_structured_pending_relationship(pending);
    }

    /// Get all pending relationships collected during extraction
    pub fn get_pending_relationships(&self) -> Vec<crate::base::PendingRelationship> {
        self.base.get_pending_relationships()
    }

    pub fn get_structured_pending_relationships(&self) -> Vec<StructuredPendingRelationship> {
        self.base.get_structured_pending_relationships()
    }
}

fn metadata_return_type(symbol: &Symbol) -> Option<String> {
    symbol
        .metadata
        .as_ref()?
        .get("returnType")?
        .as_str()
        .and_then(normalize_python_return_hint)
}

fn normalize_python_return_hint(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let without_colon = trimmed.strip_prefix(':').unwrap_or(trimmed).trim();
    if without_colon.is_empty() {
        None
    } else {
        Some(without_colon.to_string())
    }
}
