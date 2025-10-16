//! TypeScript/JavaScript symbol extractor with modular architecture
//!
//! This module provides comprehensive symbol extraction for TypeScript, JavaScript, and TSX/JSX files.
//! The architecture is organized into specialized modules for clarity and maintainability:
//!
//! - **symbols**: Core symbol extraction logic for classes, functions, interfaces, etc.
//! - **functions**: Function and method extraction with signature building
//! - **classes**: Class extraction with inheritance and modifiers
//! - **interfaces**: Interface and type alias extraction
//! - **imports_exports**: Import/export statement extraction
//! - **relationships**: Function call and inheritance relationship tracking
//! - **inference**: Type inference from assignments and return statements
//! - **identifiers**: Identifier usage extraction (calls, member access, etc.)
//! - **helpers**: Utility functions for tree traversal and text extraction

mod symbols;
mod functions;
mod classes;
mod interfaces;
mod imports_exports;
mod relationships;
mod inference;
mod identifiers;
mod helpers;

use crate::extractors::base::{BaseExtractor, Identifier, Relationship, Symbol};
use std::collections::HashMap;
use tree_sitter::Tree;

/// Main TypeScript extractor that orchestrates modular extraction components
pub struct TypeScriptExtractor {
    base: BaseExtractor,
}

impl TypeScriptExtractor {
    /// Create a new TypeScript extractor
    pub fn new(language: String, file_path: String, content: String) -> Self {
        Self {
            base: BaseExtractor::new(language, file_path, content),
        }
    }

    /// Extract all symbols from the syntax tree
    pub fn extract_symbols(&mut self, tree: &Tree) -> Vec<Symbol> {
        symbols::extract_symbols(self, tree)
    }

    /// Extract all relationships (calls, inheritance, etc.)
    pub fn extract_relationships(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Relationship> {
        relationships::extract_relationships(self, tree, symbols)
    }

    /// Extract all identifiers (function calls, member access, etc.)
    pub fn extract_identifiers(&mut self, tree: &Tree, symbols: &[Symbol]) -> Vec<Identifier> {
        identifiers::extract_identifiers(self, tree, symbols)
    }

    /// Infer types from variable assignments and function returns
    pub fn infer_types(&self, symbols: &[Symbol]) -> HashMap<String, String> {
        inference::infer_types(self, symbols)
    }

    // ========================================================================
    // Public access to base for sub-modules (pub(super) scoped internal access)
    // ========================================================================

    /// Get mutable reference to base extractor (for sub-modules)
    pub(crate) fn base_mut(&mut self) -> &mut BaseExtractor {
        &mut self.base
    }

    /// Get immutable reference to base extractor (for sub-modules)
    pub(crate) fn base(&self) -> &BaseExtractor {
        &self.base
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extractors::base::{SymbolKind, BaseExtractor};

    #[test]
    fn test_extract_function_declarations() {
        let code = "function getUserData() { return data; }";
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_javascript::LANGUAGE.into()).unwrap();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor =
            TypeScriptExtractor::new("typescript".to_string(), "test.ts".to_string(), code.to_string());
        let symbols = extractor.extract_symbols(&tree);

        assert!(!symbols.is_empty());
        assert!(symbols.iter().any(|s| s.name == "getUserData"));
    }

    #[test]
    fn test_extract_class_declarations() {
        let code = r#"
        class User {
            name: string;
            constructor(name: string) {
                this.name = name;
            }
        }
        "#;
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_javascript::LANGUAGE.into()).unwrap();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor =
            TypeScriptExtractor::new("typescript".to_string(), "test.ts".to_string(), code.to_string());
        let symbols = extractor.extract_symbols(&tree);

        assert!(symbols.iter().any(|s| s.name == "User" && s.kind == SymbolKind::Class));
    }

    #[test]
    fn test_extract_variable_and_property_declarations() {
        let code = r#"
        const myVar = 42;
        const myArrowFunc = () => "hello";
        "#;
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_javascript::LANGUAGE.into()).unwrap();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor =
            TypeScriptExtractor::new("typescript".to_string(), "test.ts".to_string(), code.to_string());
        let symbols = extractor.extract_symbols(&tree);

        assert!(symbols.iter().any(|s| s.name == "myVar"));
        assert!(symbols.iter().any(|s| s.name == "myArrowFunc" && s.kind == SymbolKind::Function));
    }

    #[test]
    fn test_extract_function_call_relationships() {
        let code = r#"
        function caller() {
            callee();
        }
        function callee() {}
        "#;
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_javascript::LANGUAGE.into()).unwrap();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor =
            TypeScriptExtractor::new("typescript".to_string(), "test.ts".to_string(), code.to_string());
        let symbols = extractor.extract_symbols(&tree);
        let relationships = extractor.extract_relationships(&tree, &symbols);

        assert!(!relationships.is_empty());
    }

    #[test]
    fn test_track_accurate_symbol_positions() {
        let code = r#"
        function foo() {}
        function bar() {}
        "#;
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_javascript::LANGUAGE.into()).unwrap();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor =
            TypeScriptExtractor::new("typescript".to_string(), "test.ts".to_string(), code.to_string());
        let symbols = extractor.extract_symbols(&tree);

        let foo_symbol = symbols.iter().find(|s| s.name == "foo").unwrap();
        let bar_symbol = symbols.iter().find(|s| s.name == "bar").unwrap();

        assert!(foo_symbol.start_line < bar_symbol.start_line);
    }

    #[test]
    fn test_extract_inheritance_relationships() {
        let code = r#"
        class Animal {}
        class Dog extends Animal {}
        "#;
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_javascript::LANGUAGE.into()).unwrap();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor =
            TypeScriptExtractor::new("typescript".to_string(), "test.ts".to_string(), code.to_string());
        let symbols = extractor.extract_symbols(&tree);
        let relationships = extractor.extract_relationships(&tree, &symbols);

        assert!(!relationships.is_empty());
    }

    #[test]
    fn test_infer_basic_types() {
        let code = r#"
        const str = "hello";
        const num = 42;
        "#;
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_javascript::LANGUAGE.into()).unwrap();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor =
            TypeScriptExtractor::new("typescript".to_string(), "test.ts".to_string(), code.to_string());
        let symbols = extractor.extract_symbols(&tree);
        let types = extractor.infer_types(&symbols);

        assert!(!types.is_empty());
    }

    #[test]
    fn test_handle_function_return_types() {
        let code = r#"
        function getString(): string {
            return "hello";
        }
        "#;
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_javascript::LANGUAGE.into()).unwrap();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor =
            TypeScriptExtractor::new("typescript".to_string(), "test.ts".to_string(), code.to_string());
        let symbols = extractor.extract_symbols(&tree);

        let func_symbol = symbols.iter().find(|s| s.name == "getString").unwrap();
        assert!(func_symbol.metadata.is_some());
    }
}
