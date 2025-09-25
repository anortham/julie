// Base Extractor Types and Traits for Julie
//
// This module is a precise port of Miller's base-extractor.ts (404 lines).
// Every method, utility, and algorithm has been carefully ported to maintain exact functionality.
//
// CRITICAL: This represents months of Miller development work. Any changes must maintain
// 100% functional parity with Miller's extractors and pass all Miller's tests.

use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use tree_sitter::Node;
use tracing::{warn, debug};
use md5;

/// A code symbol (function, class, variable, etc.) extracted from source code
///
/// Direct port of Miller's Symbol interface - exact field mapping maintained
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Symbol {
    /// Unique identifier for this symbol (MD5 hash like Miller)
    pub id: String,
    /// Symbol name as it appears in code
    pub name: String,
    /// Kind of symbol (function, class, etc.)
    pub kind: SymbolKind,
    /// Programming language this symbol is from
    pub language: String,
    /// File path where this symbol is defined
    pub file_path: String,
    /// Start line number (1-based, exactly like Miller)
    pub start_line: u32,
    /// Start column number (0-based, exactly like Miller)
    pub start_column: u32,
    /// End line number (1-based, exactly like Miller)
    pub end_line: u32,
    /// End column number (0-based, exactly like Miller)
    pub end_column: u32,
    /// Start byte offset in file
    pub start_byte: u32,
    /// End byte offset in file
    pub end_byte: u32,
    /// Function/method signature
    pub signature: Option<String>,
    /// Documentation comment (using Miller's extraction algorithm)
    pub doc_comment: Option<String>,
    /// Visibility (public, private, protected)
    pub visibility: Option<Visibility>,
    /// Parent symbol ID (for methods in classes, etc.)
    pub parent_id: Option<String>,
    /// Additional language-specific metadata
    pub metadata: Option<HashMap<String, serde_json::Value>>,
    /// Semantic group for cross-language linking
    pub semantic_group: Option<String>,
    /// Confidence score for symbol extraction (0.0 to 1.0)
    pub confidence: Option<f32>,
}

/// Symbol kinds - direct port of Miller's SymbolKind enum
///
/// CRITICAL: Order and values must match Miller exactly for test compatibility
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
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
    #[serde(rename = "enum_member")]
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

impl SymbolKind {
    /// Convert from string representation (for database deserialization)
    pub fn from_string(s: &str) -> Self {
        match s {
            "class" => SymbolKind::Class,
            "interface" => SymbolKind::Interface,
            "function" => SymbolKind::Function,
            "method" => SymbolKind::Method,
            "variable" => SymbolKind::Variable,
            "constant" => SymbolKind::Constant,
            "property" => SymbolKind::Property,
            "enum" => SymbolKind::Enum,
            "enum_member" => SymbolKind::EnumMember,
            "module" => SymbolKind::Module,
            "namespace" => SymbolKind::Namespace,
            "type" => SymbolKind::Type,
            "trait" => SymbolKind::Trait,
            "struct" => SymbolKind::Struct,
            "union" => SymbolKind::Union,
            "field" => SymbolKind::Field,
            "constructor" => SymbolKind::Constructor,
            "destructor" => SymbolKind::Destructor,
            "operator" => SymbolKind::Operator,
            "import" => SymbolKind::Import,
            "export" => SymbolKind::Export,
            "event" => SymbolKind::Event,
            "delegate" => SymbolKind::Delegate,
            _ => SymbolKind::Variable, // Default fallback
        }
    }

    /// Convert to string representation (for database serialization)
    pub fn to_string(&self) -> String {
        match self {
            SymbolKind::Class => "class",
            SymbolKind::Interface => "interface",
            SymbolKind::Function => "function",
            SymbolKind::Method => "method",
            SymbolKind::Variable => "variable",
            SymbolKind::Constant => "constant",
            SymbolKind::Property => "property",
            SymbolKind::Enum => "enum",
            SymbolKind::EnumMember => "enum_member",
            SymbolKind::Module => "module",
            SymbolKind::Namespace => "namespace",
            SymbolKind::Type => "type",
            SymbolKind::Trait => "trait",
            SymbolKind::Struct => "struct",
            SymbolKind::Union => "union",
            SymbolKind::Field => "field",
            SymbolKind::Constructor => "constructor",
            SymbolKind::Destructor => "destructor",
            SymbolKind::Operator => "operator",
            SymbolKind::Import => "import",
            SymbolKind::Export => "export",
            SymbolKind::Event => "event",
            SymbolKind::Delegate => "delegate",
        }.to_string()
    }
}

/// Visibility levels for symbols - direct port from Miller
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    Public,
    Private,
    Protected,
}

/// Relationship between two symbols - direct port from Miller
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Relationship {
    /// Unique identifier for this relationship
    pub id: String,
    /// Source symbol ID
    #[serde(rename = "fromSymbolId")]
    pub from_symbol_id: String,
    /// Target symbol ID
    #[serde(rename = "toSymbolId")]
    pub to_symbol_id: String,
    /// Type of relationship
    pub kind: RelationshipKind,
    /// File where this relationship occurs
    #[serde(rename = "filePath")]
    pub file_path: String,
    /// Line number where relationship occurs (1-based like Miller)
    #[serde(rename = "lineNumber")]
    pub line_number: u32,
    /// Confidence level (0.0 to 1.0)
    pub confidence: f32,
    /// Additional metadata
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

/// Relationship kinds - direct port from Miller's RelationshipKind enum
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
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
    Composition,
}

impl std::fmt::Display for RelationshipKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RelationshipKind::Calls => write!(f, "calls"),
            RelationshipKind::Extends => write!(f, "extends"),
            RelationshipKind::Implements => write!(f, "implements"),
            RelationshipKind::Uses => write!(f, "uses"),
            RelationshipKind::Returns => write!(f, "returns"),
            RelationshipKind::Parameter => write!(f, "parameter"),
            RelationshipKind::Imports => write!(f, "imports"),
            RelationshipKind::Instantiates => write!(f, "instantiates"),
            RelationshipKind::References => write!(f, "references"),
            RelationshipKind::Defines => write!(f, "defines"),
            RelationshipKind::Overrides => write!(f, "overrides"),
            RelationshipKind::Contains => write!(f, "contains"),
            RelationshipKind::Joins => write!(f, "joins"),
            RelationshipKind::Composition => write!(f, "composition"),
        }
    }
}

impl RelationshipKind {
    /// Convert from string representation (for database deserialization)
    pub fn from_string(s: &str) -> Self {
        match s {
            "calls" => RelationshipKind::Calls,
            "extends" => RelationshipKind::Extends,
            "implements" => RelationshipKind::Implements,
            "uses" => RelationshipKind::Uses,
            "returns" => RelationshipKind::Returns,
            "parameter" => RelationshipKind::Parameter,
            "imports" => RelationshipKind::Imports,
            "instantiates" => RelationshipKind::Instantiates,
            "references" => RelationshipKind::References,
            "defines" => RelationshipKind::Defines,
            "overrides" => RelationshipKind::Overrides,
            "contains" => RelationshipKind::Contains,
            "joins" => RelationshipKind::Joins,
            _ => RelationshipKind::Uses, // Default fallback
        }
    }

    /// Convert to string representation (for database serialization)
    pub fn to_string(&self) -> String {
        match self {
            RelationshipKind::Calls => "calls",
            RelationshipKind::Extends => "extends",
            RelationshipKind::Implements => "implements",
            RelationshipKind::Uses => "uses",
            RelationshipKind::Returns => "returns",
            RelationshipKind::Parameter => "parameter",
            RelationshipKind::Imports => "imports",
            RelationshipKind::Instantiates => "instantiates",
            RelationshipKind::References => "references",
            RelationshipKind::Defines => "defines",
            RelationshipKind::Overrides => "overrides",
            RelationshipKind::Contains => "contains",
            RelationshipKind::Joins => "joins",
            RelationshipKind::Composition => "composition",
        }.to_string()
    }
}

/// Type information for a symbol - direct port from Miller
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TypeInfo {
    /// Symbol this type info belongs to
    #[serde(rename = "symbolId")]
    pub symbol_id: String,
    /// Resolved type name
    #[serde(rename = "resolvedType")]
    pub resolved_type: String,
    /// Generic type parameters
    #[serde(rename = "genericParams")]
    pub generic_params: Option<Vec<String>>,
    /// Type constraints
    pub constraints: Option<Vec<String>>,
    /// Whether type was inferred or explicit
    #[serde(rename = "isInferred")]
    pub is_inferred: bool,
    /// Programming language
    pub language: String,
    /// Additional type metadata
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

/// Base implementation for language extractors
///
/// Direct port of Miller's BaseExtractor class with all utility methods
pub struct BaseExtractor {
    pub language: String,
    pub file_path: String,
    pub content: String,
    pub symbol_map: HashMap<String, Symbol>,
    pub relationships: Vec<Relationship>,
    pub type_info: HashMap<String, TypeInfo>,
}

impl BaseExtractor {
    /// Create new abstract extractor - port of Miller's constructor
    pub fn new(language: String, file_path: String, content: String) -> Self {
        Self {
            language,
            file_path,
            content,
            symbol_map: HashMap::new(),
            relationships: Vec::new(),
            type_info: HashMap::new(),
        }
    }

    /// Get text from a tree-sitter node - exact port of Miller's getNodeText
    pub fn get_node_text(&self, node: &Node) -> String {
        let start_byte = node.start_byte();
        let end_byte = node.end_byte();

        // Use byte slice but handle UTF-8 boundaries properly
        let content_bytes = self.content.as_bytes();
        if start_byte < content_bytes.len() && end_byte <= content_bytes.len() {
            String::from_utf8_lossy(&content_bytes[start_byte..end_byte]).to_string()
        } else {
            String::new()
        }
    }

    /// Find documentation comment for a node - exact port of Miller's findDocComment
    pub fn find_doc_comment(&self, node: &Node) -> Option<String> {
        // Look for comment nodes preceding the current node
        if let Some(prev_sibling) = node.prev_named_sibling() {
            if prev_sibling.kind().contains("comment") {
                return Some(self.get_node_text(&prev_sibling));
            }
        }

        // Look for JSDoc-style comments above the node
        if let Some(parent) = node.parent() {
            let node_row = node.start_position().row;

            for i in 0..parent.child_count() {
                if let Some(child) = parent.child(i) {
                    if child.start_position().row < node_row && child.kind().contains("comment") {
                        let comment_text = self.get_node_text(&child);
                        // Check if it's a documentation comment (starts with /** or ///)
                        if comment_text.starts_with("/**") || comment_text.starts_with("///") {
                            return Some(comment_text);
                        }
                    }
                }
            }
        }

        None
    }

    /// Generate ID for a symbol - exact port of Miller's generateId (MD5 hash)
    pub fn generate_id(&self, name: &str, line: u32, column: u32) -> String {
        let input = format!("{}:{}:{}:{}", self.file_path, name, line, column);
        let digest = md5::compute(input.as_bytes());
        format!("{:x}", digest)
    }

    /// Create a symbol - exact port of Miller's createSymbol method
    pub fn create_symbol(
        &mut self,
        node: &Node,
        name: String,
        kind: SymbolKind,
        options: SymbolOptions,
    ) -> Symbol {
        let start_pos = node.start_position();
        let end_pos = node.end_position();

        let id = self.generate_id(&name, start_pos.row as u32, start_pos.column as u32);

        let symbol = Symbol {
            id: id.clone(),
            name,
            kind,
            language: self.language.clone(),
            file_path: self.file_path.clone(),
            start_line: (start_pos.row + 1) as u32, // Miller uses 1-based line numbers
            start_column: start_pos.column as u32, // Miller uses 0-based column numbers
            end_line: (end_pos.row + 1) as u32,
            end_column: end_pos.column as u32,
            start_byte: node.start_byte() as u32,
            end_byte: node.end_byte() as u32,
            signature: options.signature,
            doc_comment: options.doc_comment.or_else(|| self.find_doc_comment(node)),
            visibility: options.visibility,
            parent_id: options.parent_id,
            metadata: Some(options.metadata.unwrap_or_default()),
            semantic_group: None, // Will be populated during cross-language analysis
            confidence: None, // Will be calculated based on parsing context
        };

        self.symbol_map.insert(id, symbol.clone());
        symbol
    }

    /// Create a relationship - exact port of Miller's createRelationship
    pub fn create_relationship(
        &self,
        from_symbol_id: String,
        to_symbol_id: String,
        kind: RelationshipKind,
        node: &Node,
        confidence: Option<f32>,
        metadata: Option<HashMap<String, serde_json::Value>>,
    ) -> Relationship {
        Relationship {
            id: format!("{}_{}_{:?}_{}", from_symbol_id, to_symbol_id, kind, node.start_position().row),
            from_symbol_id,
            to_symbol_id,
            kind,
            file_path: self.file_path.clone(),
            line_number: (node.start_position().row + 1) as u32, // 1-based like Miller
            confidence: confidence.unwrap_or(1.0),
            metadata,
        }
    }

    /// Find containing symbol - exact port of Miller's findContainingSymbol
    pub fn find_containing_symbol<'a>(&self, node: &Node, symbols: &'a [Symbol]) -> Option<&'a Symbol> {
        let position = node.start_position();

        // Find symbols that contain this position
        let mut containing_symbols: Vec<&Symbol> = symbols.iter().filter(|s| {
            let pos_line = (position.row + 1) as u32;
            let pos_column = position.column as u32;

            let line_contains = s.start_line <= pos_line && s.end_line >= pos_line;

            // For column containment, handle multi-line spans exactly like Miller
            let col_contains = if pos_line == s.start_line && pos_line == s.end_line {
                // Single line span
                s.start_column <= pos_column && s.end_column >= pos_column
            } else if pos_line == s.start_line {
                // First line of multi-line span
                s.start_column <= pos_column
            } else if pos_line == s.end_line {
                // Last line of multi-line span
                s.end_column >= pos_column
            } else {
                // Middle line of multi-line span
                true
            };

            line_contains && col_contains
        }).collect();

        if containing_symbols.is_empty() {
            return None;
        }

        // Priority order - exact copy from Miller
        let get_priority = |kind: &SymbolKind| -> u32 {
            match kind {
                SymbolKind::Function | SymbolKind::Method | SymbolKind::Constructor => 1,
                SymbolKind::Class | SymbolKind::Interface => 2,
                SymbolKind::Namespace => 3,
                SymbolKind::Variable | SymbolKind::Constant | SymbolKind::Property => 10,
                _ => 5,
            }
        };

        containing_symbols.sort_by(|a, b| {
            // First, sort by priority (functions first)
            let priority_a = get_priority(&a.kind);
            let priority_b = get_priority(&b.kind);
            if priority_a != priority_b {
                return priority_a.cmp(&priority_b);
            }

            // Then by size (smaller first) - exact Miller calculation
            let size_a = (a.end_line - a.start_line) * 1000 + (a.end_column - a.start_column);
            let size_b = (b.end_line - b.start_line) * 1000 + (b.end_column - b.start_column);
            size_a.cmp(&size_b)
        });

        Some(containing_symbols[0])
    }

    /// Extract visibility - exact port of Miller's extractVisibility
    pub fn extract_visibility(&self, node: &Node) -> Option<Visibility> {
        // Look for visibility modifiers in child nodes
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                match child.kind() {
                    "public" => return Some(Visibility::Public),
                    "private" => return Some(Visibility::Private),
                    "protected" => return Some(Visibility::Protected),
                    _ => continue,
                }
            }
        }

        // Check for language-specific patterns in text
        let text = self.get_node_text(node);
        if text.contains("public ") {
            Some(Visibility::Public)
        } else if text.contains("private ") {
            Some(Visibility::Private)
        } else if text.contains("protected ") {
            Some(Visibility::Protected)
        } else {
            None
        }
    }

    /// Extract identifier name - exact port of Miller's extractIdentifierName
    pub fn extract_identifier_name(&self, node: &Node) -> String {
        // Try to find the identifier node using field name
        if let Some(name_node) = node.child_by_field_name("name") {
            if name_node.kind() == "identifier" {
                return self.get_node_text(&name_node);
            }
        }

        // Try first child
        if let Some(first_child) = node.child(0) {
            if first_child.kind() == "identifier" {
                return self.get_node_text(&first_child);
            }
        }

        // Fallback: extract from the node text using regex (exact Miller pattern)
        let node_text = self.get_node_text(node);
        let text = node_text.trim();
        if let Some(captures) = regex::Regex::new(r"^[a-zA-Z_$][a-zA-Z0-9_$]*").unwrap().find(text) {
            captures.as_str().to_string()
        } else {
            "Anonymous".to_string()
        }
    }

    /// Walk tree with visitor - exact port of Miller's walkTree
    pub fn walk_tree<F>(&self, node: &Node, visitor: &mut F, depth: u32)
    where
        F: FnMut(&Node, u32),
    {
        visitor(node, depth);

        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                self.walk_tree(&child, visitor, depth + 1);
            }
        }
    }

    /// Find nodes by type - exact port of Miller's findNodesByType
    pub fn find_nodes_by_type<'a>(&self, node: &Node<'a>, node_type: &str) -> Vec<Node<'a>> {
        let mut nodes = Vec::new();
        self.find_nodes_by_type_recursive(node, node_type, &mut nodes);
        nodes
    }

    fn find_nodes_by_type_recursive<'a>(&self, node: &Node<'a>, node_type: &str, nodes: &mut Vec<Node<'a>>) {
        if node.kind() == node_type {
            nodes.push(*node);
        }

        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                self.find_nodes_by_type_recursive(&child, node_type, nodes);
            }
        }
    }

    /// Find parent of type - exact port of Miller's findParentOfType
    pub fn find_parent_of_type<'a>(&self, node: &Node<'a>, parent_type: &str) -> Option<Node<'a>> {
        let mut current = node.parent();
        while let Some(parent) = current {
            if parent.kind() == parent_type {
                return Some(parent);
            }
            current = parent.parent();
        }
        None
    }

    /// Check if node has error - exact port of Miller's hasError
    pub fn has_error(&self, node: &Node) -> bool {
        node.has_error() || node.kind() == "ERROR"
    }

    /// Get children of type - exact port of Miller's getChildrenOfType
    pub fn get_children_of_type<'a>(&self, node: &Node<'a>, child_type: &str) -> Vec<Node<'a>> {
        let mut children = Vec::new();
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                if child.kind() == child_type {
                    children.push(child);
                }
            }
        }
        children
    }

    /// Get field text safely - exact port of Miller's getFieldText
    pub fn get_field_text(&self, node: &Node, field_name: &str) -> Option<String> {
        node.child_by_field_name(field_name)
            .map(|field_node| self.get_node_text(&field_node))
    }

    /// Reset extractor state - exact port of Miller's reset
    pub fn reset(&mut self) {
        self.symbol_map.clear();
        self.relationships.clear();
        self.type_info.clear();
    }

    /// Traverse tree with error handling - exact port of Miller's traverseTree
    pub fn traverse_tree<F>(&self, node: &Node, callback: &mut F)
    where
        F: FnMut(&Node),
    {
        // Try to process current node
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback(node))) {
            Ok(_) => {},
            Err(_) => {
                warn!("Error processing node {}", node.kind());
                return;
            }
        }

        // Recursively traverse children with error handling
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    self.traverse_tree(&child, callback)
                })) {
                    Ok(_) => {},
                    Err(_) => {
                        debug!("Skipping problematic child node");
                        continue;
                    }
                }
            }
        }
    }

    /// Find first child by type - exact port of Miller's findChildByType
    pub fn find_child_by_type<'a>(&self, node: &Node<'a>, child_type: &str) -> Option<Node<'a>> {
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                if child.kind() == child_type {
                    return Some(child);
                }
            }
        }
        None
    }

    /// Find children by type - exact port of Miller's findChildrenByType
    pub fn find_children_by_type<'a>(&self, node: &Node<'a>, child_type: &str) -> Vec<Node<'a>> {
        let mut results = Vec::new();
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                if child.kind() == child_type {
                    results.push(child);
                }
            }
        }
        results
    }

    /// Find child by multiple types - exact port of Miller's findChildByTypes
    pub fn find_child_by_types<'a>(&self, node: &Node<'a>, types: &[&str]) -> Option<Node<'a>> {
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                if types.contains(&child.kind()) {
                    return Some(child);
                }
            }
        }
        None
    }

    /// Extract documentation - alias for find_doc_comment (Miller consistency)
    pub fn extract_documentation(&self, node: &Node) -> Option<String> {
        self.find_doc_comment(node)
    }

    /// Get extraction results - exact port of Miller's getResults
    pub fn get_results(&self) -> ExtractionResults {
        ExtractionResults {
            symbols: self.symbol_map.values().cloned().collect(),
            relationships: self.relationships.clone(),
            types: self.type_info.clone(),
        }
    }
}

/// Options for creating symbols - matches Miller's createSymbol options
#[derive(Debug, Clone, Default)]
pub struct SymbolOptions {
    pub signature: Option<String>,
    pub visibility: Option<Visibility>,
    pub parent_id: Option<String>,
    pub metadata: Option<HashMap<String, serde_json::Value>>,
    pub doc_comment: Option<String>,
}

/// Extraction results - matches Miller's getResults return type
#[derive(Debug, Clone)]
pub struct ExtractionResults {
    pub symbols: Vec<Symbol>,
    pub relationships: Vec<Relationship>,
    pub types: HashMap<String, TypeInfo>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_creation() {
        let extractor = BaseExtractor::new(
            "javascript".to_string(),
            "test.js".to_string(),
            "function test() {}".to_string(),
        );

        // This will be tested with actual tree-sitter nodes in integration tests
        // For now, just test that the basic structure works
        assert_eq!(extractor.language, "javascript");
        assert_eq!(extractor.file_path, "test.js");
        assert!(!extractor.content.is_empty());
    }

    #[test]
    fn test_id_generation() {
        let extractor = BaseExtractor::new(
            "rust".to_string(),
            "src/lib.rs".to_string(),
            "fn test() {}".to_string(),
        );

        let id1 = extractor.generate_id("test", 1, 0);
        let id2 = extractor.generate_id("test", 1, 0);
        let id3 = extractor.generate_id("test", 2, 0);

        assert_eq!(id1, id2); // Same inputs should give same ID
        assert_ne!(id1, id3); // Different inputs should give different IDs
        assert_eq!(id1.len(), 32); // MD5 hash is 32 chars
    }
}