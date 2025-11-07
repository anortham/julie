// Base Extractor Types for Julie
//
// All data structures for symbol extraction, identifiers, relationships, and types.
// Lines 15-394 from original base.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for code context extraction
#[derive(Debug, Clone)]
pub struct ContextConfig {
    /// Number of lines to show before the symbol
    pub lines_before: usize,
    /// Number of lines to show after the symbol
    pub lines_after: usize,
    /// Maximum line length to display (longer lines get truncated)
    pub max_line_length: usize,
    /// Whether to show line numbers in context
    pub show_line_numbers: bool,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            lines_before: 3,
            lines_after: 3,
            max_line_length: 120,
            show_line_numbers: true,
        }
    }
}

/// A code symbol (function, class, variable, etc.) extracted from source code
///
/// Direct Implementation of Symbol interface - exact field mapping maintained
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Symbol {
    /// Unique identifier for this symbol (MD5 hash standard format)
    pub id: String,
    /// Symbol name as it appears in code
    pub name: String,
    /// Kind of symbol (function, class, etc.)
    pub kind: SymbolKind,
    /// Programming language this symbol is from
    pub language: String,
    /// File path where this symbol is defined
    pub file_path: String,
    /// Start line number (1-based, exactly standard format)
    pub start_line: u32,
    /// Start column number (0-based, exactly standard format)
    pub start_column: u32,
    /// End line number (1-based, exactly standard format)
    pub end_line: u32,
    /// End column number (0-based, exactly standard format)
    pub end_column: u32,
    /// Start byte offset in file
    pub start_byte: u32,
    /// End byte offset in file
    pub end_byte: u32,
    /// Function/method signature
    pub signature: Option<String>,
    /// Documentation comment (using extraction algorithm)
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
    /// Code context lines around the symbol (3 lines before + match + 3 lines after)
    pub code_context: Option<String>,
    /// Content type to distinguish documentation from code
    /// None = code (default), Some("documentation") = markdown docs
    pub content_type: Option<String>,
}

/// An identifier (reference/usage) extracted from source code
///
/// Unlike Symbols (definitions), Identifiers represent usage sites like function calls,
/// variable references, type usages, etc. They are extracted unresolved (target_symbol_id is None)
/// and resolved on-demand during queries for optimal incremental update performance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Identifier {
    /// Unique identifier for this reference (MD5 hash)
    pub id: String,
    /// Identifier name as it appears in code
    pub name: String,
    /// Kind of identifier (call, variable_ref, type_usage, member_access)
    pub kind: IdentifierKind,
    /// Programming language this identifier is from
    pub language: String,
    /// File path where this identifier appears
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
    /// ID of the symbol that contains this identifier (e.g., which function uses this variable)
    pub containing_symbol_id: Option<String>,
    /// ID of the symbol this identifier refers to (None until resolved on-demand)
    pub target_symbol_id: Option<String>,
    /// Confidence score for identifier extraction (0.0 to 1.0)
    pub confidence: f32,
    /// Code context around the identifier
    pub code_context: Option<String>,
}

/// Identifier kinds - types of references/usages in code
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum IdentifierKind {
    /// Function/method call
    Call,
    /// Variable reference (reading a variable)
    VariableRef,
    /// Type usage (in type annotations, casts, etc.)
    TypeUsage,
    /// Member access (object.property, object.method)
    MemberAccess,
    /// Import/use statement
    Import,
}

impl std::fmt::Display for IdentifierKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IdentifierKind::Call => write!(f, "call"),
            IdentifierKind::VariableRef => write!(f, "variable_ref"),
            IdentifierKind::TypeUsage => write!(f, "type_usage"),
            IdentifierKind::MemberAccess => write!(f, "member_access"),
            IdentifierKind::Import => write!(f, "import"),
        }
    }
}

impl IdentifierKind {
    /// Convert from string representation (for database deserialization)
    pub fn from_string(s: &str) -> Self {
        match s {
            "call" => IdentifierKind::Call,
            "variable_ref" => IdentifierKind::VariableRef,
            "type_usage" => IdentifierKind::TypeUsage,
            "member_access" => IdentifierKind::MemberAccess,
            "import" => IdentifierKind::Import,
            _ => IdentifierKind::VariableRef, // Default fallback
        }
    }
}

/// Symbol kinds - Implementation of SymbolKind enum
///
/// CRITICAL: Order and values must maintain test compatibility
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
    #[allow(dead_code)] // TODO: Used for database deserialization
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

    // Note: to_string() is provided by Display trait implementation below
}

impl std::fmt::Display for SymbolKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SymbolKind::Class => write!(f, "class"),
            SymbolKind::Interface => write!(f, "interface"),
            SymbolKind::Function => write!(f, "function"),
            SymbolKind::Method => write!(f, "method"),
            SymbolKind::Variable => write!(f, "variable"),
            SymbolKind::Constant => write!(f, "constant"),
            SymbolKind::Property => write!(f, "property"),
            SymbolKind::Enum => write!(f, "enum"),
            SymbolKind::EnumMember => write!(f, "enum_member"),
            SymbolKind::Module => write!(f, "module"),
            SymbolKind::Namespace => write!(f, "namespace"),
            SymbolKind::Type => write!(f, "type"),
            SymbolKind::Trait => write!(f, "trait"),
            SymbolKind::Struct => write!(f, "struct"),
            SymbolKind::Union => write!(f, "union"),
            SymbolKind::Field => write!(f, "field"),
            SymbolKind::Constructor => write!(f, "constructor"),
            SymbolKind::Destructor => write!(f, "destructor"),
            SymbolKind::Operator => write!(f, "operator"),
            SymbolKind::Import => write!(f, "import"),
            SymbolKind::Export => write!(f, "export"),
            SymbolKind::Event => write!(f, "event"),
            SymbolKind::Delegate => write!(f, "delegate"),
        }
    }
}

/// Visibility levels for symbols - reference implementation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    Public,
    Private,
    Protected,
}

impl std::fmt::Display for Visibility {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Visibility::Public => write!(f, "Public"),
            Visibility::Private => write!(f, "Private"),
            Visibility::Protected => write!(f, "Protected"),
        }
    }
}

/// Relationship between two symbols - reference implementation
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
    /// Line number where relationship occurs (1-based standard format)
    #[serde(rename = "lineNumber")]
    pub line_number: u32,
    /// Confidence level (0.0 to 1.0)
    pub confidence: f32,
    /// Additional metadata
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

/// Relationship kinds - direct port from RelationshipKind enum
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
    #[allow(dead_code)] // TODO: Used for database deserialization
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

    // Note: to_string() is provided automatically by the Display trait implementation above
    // No need for an inherent method that shadows it
}

/// Type information for a symbol - reference implementation
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

/// Options for creating symbols - matches createSymbol options
#[derive(Debug, Clone, Default)]
pub struct SymbolOptions {
    pub signature: Option<String>,
    pub visibility: Option<Visibility>,
    pub parent_id: Option<String>,
    pub metadata: Option<HashMap<String, serde_json::Value>>,
    pub doc_comment: Option<String>,
}

/// Extraction results - matches getResults return type
#[derive(Debug, Clone)]
pub struct ExtractionResults {
    pub symbols: Vec<Symbol>,
    pub relationships: Vec<Relationship>,
    pub types: HashMap<String, TypeInfo>,
    pub identifiers: Vec<Identifier>, // Include identifiers for LSP-quality tools
}
