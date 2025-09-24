// Base Extractor Types and Traits for Julie
//
// This module defines the core types and traits that all language extractors must implement.
// It's based on Miller's proven extractor architecture but built with Rust's type system.

use serde::{Serialize, Deserialize};
use uuid::Uuid;
use std::collections::HashMap;

/// A code symbol (function, class, variable, etc.) extracted from source code
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Symbol {
    /// Unique identifier for this symbol
    pub id: String,
    /// Symbol name as it appears in code
    pub name: String,
    /// Kind of symbol (function, class, etc.)
    pub kind: SymbolKind,
    /// Programming language this symbol is from
    pub language: String,
    /// File path where this symbol is defined
    pub file_path: String,
    /// Start line number (1-based)
    pub start_line: u32,
    /// Start column number (0-based)
    pub start_column: u32,
    /// End line number (1-based)
    pub end_line: u32,
    /// End column number (0-based)
    pub end_column: u32,
    /// Start byte offset in file
    pub start_byte: u32,
    /// End byte offset in file
    pub end_byte: u32,
    /// Function/method signature
    pub signature: Option<String>,
    /// Documentation comment
    pub doc_comment: Option<String>,
    /// Visibility (public, private, protected)
    pub visibility: Option<Visibility>,
    /// Parent symbol ID (for methods in classes, etc.)
    pub parent_id: Option<String>,
    /// Additional language-specific metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Symbol {
    /// Create a new symbol with generated ID
    pub fn new(
        name: String,
        kind: SymbolKind,
        language: String,
        file_path: String,
        start_line: u32,
        start_column: u32,
        end_line: u32,
        end_column: u32,
        start_byte: u32,
        end_byte: u32,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            kind,
            language,
            file_path,
            start_line,
            start_column,
            end_line,
            end_column,
            start_byte,
            end_byte,
            signature: None,
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: HashMap::new(),
        }
    }
}

/// Types of symbols that can be extracted from source code
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Class,
    Interface,
    Function,
    Method,
    Variable,
    Constant,
    Property,
    Enum,
    EnumMember,
    Module,
    Namespace,
    Type,
    Trait,
    Struct,
    Union,
    Field,
    Constructor,
    Destructor,
    Operator,
    Import,
    Export,
    Event,
    Delegate,
}

/// Visibility levels for symbols
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Public,
    Private,
    Protected,
    Internal,
    Package,
}

/// Relationship between two symbols
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Relationship {
    /// Source symbol ID
    pub from_symbol_id: String,
    /// Target symbol ID
    pub to_symbol_id: String,
    /// Type of relationship
    pub kind: RelationshipKind,
    /// File where this relationship occurs
    pub file_path: String,
    /// Line number where relationship occurs
    pub line_number: u32,
    /// Confidence level (0.0 to 1.0)
    pub confidence: f32,
    /// Additional metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Types of relationships between symbols
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipKind {
    Calls,
    Extends,
    Implements,
    Uses,
    Returns,
    Parameter,
    Imports,
    Instantiates,
    References,
    Defines,
    Overrides,
    Contains,
    Joins,
}

/// Type information for a symbol
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TypeInfo {
    /// Symbol this type info belongs to
    pub symbol_id: String,
    /// Resolved type name
    pub resolved_type: String,
    /// Generic type parameters
    pub generic_params: Option<Vec<String>>,
    /// Type constraints
    pub constraints: Option<Vec<String>>,
    /// Whether type was inferred or explicit
    pub is_inferred: bool,
    /// Programming language
    pub language: String,
    /// Additional type metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Base trait that all language extractors must implement
pub trait BaseExtractor {
    /// Extract symbols from a tree-sitter parse tree
    fn extract_symbols(&mut self, tree: &tree_sitter::Tree) -> Vec<Symbol>;

    /// Extract relationships between symbols
    fn extract_relationships(&mut self, tree: &tree_sitter::Tree, symbols: &[Symbol]) -> Vec<Relationship>;

    /// Infer type information for symbols
    fn infer_types(&mut self, symbols: &[Symbol]) -> HashMap<String, TypeInfo>;

    /// Get the language this extractor handles
    fn language(&self) -> &str;

    /// Get the file path being processed
    fn file_path(&self) -> &str;

    /// Get the source content being processed
    fn content(&self) -> &str;
}

/// Abstract base implementation for language extractors
pub struct AbstractExtractor {
    pub language: String,
    pub file_path: String,
    pub content: String,
    pub symbols: HashMap<String, Symbol>,
    pub relationships: Vec<Relationship>,
    pub type_info: HashMap<String, TypeInfo>,
}

impl AbstractExtractor {
    pub fn new(language: String, file_path: String, content: String) -> Self {
        Self {
            language,
            file_path,
            content,
            symbols: HashMap::new(),
            relationships: Vec::new(),
            type_info: HashMap::new(),
        }
    }

    /// Helper to create a hash-based ID for symbols
    pub fn create_symbol_id(&self, name: &str, start_line: u32, start_column: u32) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        self.file_path.hash(&mut hasher);
        name.hash(&mut hasher);
        start_line.hash(&mut hasher);
        start_column.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }

    /// Helper to get text at a specific tree-sitter node
    pub fn get_node_text(&self, node: &tree_sitter::Node) -> String {
        let start_byte = node.start_byte();
        let end_byte = node.end_byte();
        self.content.get(start_byte..end_byte)
            .unwrap_or("")
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_creation() {
        let symbol = Symbol::new(
            "testFunction".to_string(),
            SymbolKind::Function,
            "javascript".to_string(),
            "test.js".to_string(),
            10, 0, 15, 1,
            100, 200,
        );

        assert_eq!(symbol.name, "testFunction");
        assert_eq!(symbol.kind, SymbolKind::Function);
        assert_eq!(symbol.language, "javascript");
        assert_eq!(symbol.start_line, 10);
        assert!(!symbol.id.is_empty());
    }

    #[test]
    fn test_symbol_id_generation() {
        let extractor = AbstractExtractor::new(
            "rust".to_string(),
            "src/lib.rs".to_string(),
            "fn test() {}".to_string(),
        );

        let id1 = extractor.create_symbol_id("test", 1, 0);
        let id2 = extractor.create_symbol_id("test", 1, 0);
        let id3 = extractor.create_symbol_id("test", 2, 0);

        assert_eq!(id1, id2); // Same inputs should give same ID
        assert_ne!(id1, id3); // Different inputs should give different IDs
    }
}