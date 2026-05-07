// Base Extractor Types for Julie
//
// All data structures for symbol extraction, identifiers, relationships, and types.
// Lines 15-394 from original base.rs

use md5;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::relationship_resolution::StructuredPendingRelationship;
use super::span::NormalizedSpan;

/// Tree-sitter parse recovery diagnostic kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParseDiagnosticKind {
    Error,
    Missing,
}

/// Span for syntax recovery produced by tree-sitter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParseDiagnostic {
    pub kind: ParseDiagnosticKind,
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
    pub start_byte: u32,
    pub end_byte: u32,
}

/// Role classification for test-related symbols.
///
/// Distinguishes test cases (scorable for quality) from fixtures and containers
/// (not scorable), preventing false "stub" classifications on setup/teardown methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestRole {
    TestCase,
    ParameterizedTest,
    FixtureSetup,
    FixtureTeardown,
    TestContainer,
}

impl TestRole {
    /// Returns true for roles where quality scoring (assert density, stub detection) applies.
    pub fn is_scorable(&self) -> bool {
        matches!(self, TestRole::TestCase | TestRole::ParameterizedTest)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            TestRole::TestCase => "test_case",
            TestRole::ParameterizedTest => "parameterized_test",
            TestRole::FixtureSetup => "fixture_setup",
            TestRole::FixtureTeardown => "fixture_teardown",
            TestRole::TestContainer => "test_container",
        }
    }
}

/// Canonical annotation marker with display, match, and source text forms.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnnotationMarker {
    pub annotation: String,
    pub annotation_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub carrier: Option<String>,
}

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// Documentation comment (using extraction algorithm)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc_comment: Option<String>,
    /// Visibility (public, private, protected)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<Visibility>,
    /// Parent symbol ID (for methods in classes, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// Additional language-specific metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
    /// Canonical annotation markers.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub annotations: Vec<AnnotationMarker>,
    /// Semantic group for cross-language linking
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic_group: Option<String>,
    /// Confidence score for symbol extraction (0.0 to 1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    /// Code context lines around the symbol (3 lines before + match + 3 lines after)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_context: Option<String>,
    /// Content type to distinguish documentation from code
    /// None = code (default), Some("documentation") = markdown docs
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
}

impl Symbol {
    pub fn apply_normalized_span(&mut self, span: NormalizedSpan) {
        self.start_line = span.start_line;
        self.start_column = span.start_column;
        self.end_line = span.end_line;
        self.end_column = span.end_column;
        self.start_byte = span.start_byte;
        self.end_byte = span.end_byte;
    }

    pub fn refresh_id(&mut self) -> String {
        let previous_id = self.id.clone();
        self.id = stable_location_id(self.file_path.as_str(), self.name.as_str(), self.span());
        previous_id
    }

    fn span(&self) -> NormalizedSpan {
        NormalizedSpan {
            start_line: self.start_line,
            start_column: self.start_column,
            end_line: self.end_line,
            end_column: self.end_column,
            start_byte: self.start_byte,
            end_byte: self.end_byte,
        }
    }
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

impl Identifier {
    pub fn apply_normalized_span(&mut self, span: NormalizedSpan) {
        self.start_line = span.start_line;
        self.start_column = span.start_column;
        self.end_line = span.end_line;
        self.end_column = span.end_column;
        self.start_byte = span.start_byte;
        self.end_byte = span.end_byte;
    }

    pub fn refresh_id(&mut self) {
        self.id = stable_location_id(self.file_path.as_str(), self.name.as_str(), self.span());
    }

    fn span(&self) -> NormalizedSpan {
        NormalizedSpan {
            start_line: self.start_line,
            start_column: self.start_column,
            end_line: self.end_line,
            end_column: self.end_column,
            start_byte: self.start_byte,
            end_byte: self.end_byte,
        }
    }
}

pub(crate) fn stable_location_id(file_path: &str, name: &str, span: NormalizedSpan) -> String {
    let input = format!(
        "{}:{}:{}:{}:{}:{}:{}:{}",
        file_path,
        name,
        span.start_line,
        span.start_column,
        span.end_line,
        span.end_column,
        span.start_byte,
        span.end_byte
    );
    format!("{:x}", md5::compute(input.as_bytes()))
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
}

impl std::fmt::Display for IdentifierKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IdentifierKind::Call => write!(f, "call"),
            IdentifierKind::VariableRef => write!(f, "variable_ref"),
            IdentifierKind::TypeUsage => write!(f, "type_usage"),
            IdentifierKind::MemberAccess => write!(f, "member_access"),
        }
    }
}

impl IdentifierKind {
    /// Convert from string representation (for database deserialization)
    pub fn try_from_string(s: &str) -> Option<Self> {
        match s {
            "call" => Some(IdentifierKind::Call),
            "variable_ref" => Some(IdentifierKind::VariableRef),
            "type_usage" => Some(IdentifierKind::TypeUsage),
            "member_access" => Some(IdentifierKind::MemberAccess),
            _ => None,
        }
    }

    /// Convert from string representation (for database deserialization)
    pub fn from_string(s: &str) -> Self {
        Self::try_from_string(s).unwrap_or_else(|| panic!("unknown identifier kind: {s}"))
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
    pub fn try_from_string(s: &str) -> Option<Self> {
        match s {
            "class" => Some(SymbolKind::Class),
            "interface" => Some(SymbolKind::Interface),
            "function" => Some(SymbolKind::Function),
            "method" => Some(SymbolKind::Method),
            "variable" => Some(SymbolKind::Variable),
            "constant" => Some(SymbolKind::Constant),
            "property" => Some(SymbolKind::Property),
            "enum" => Some(SymbolKind::Enum),
            "enum_member" => Some(SymbolKind::EnumMember),
            "module" => Some(SymbolKind::Module),
            "namespace" => Some(SymbolKind::Namespace),
            "type" => Some(SymbolKind::Type),
            "trait" => Some(SymbolKind::Trait),
            "struct" => Some(SymbolKind::Struct),
            "union" => Some(SymbolKind::Union),
            "field" => Some(SymbolKind::Field),
            "constructor" => Some(SymbolKind::Constructor),
            "destructor" => Some(SymbolKind::Destructor),
            "operator" => Some(SymbolKind::Operator),
            "import" => Some(SymbolKind::Import),
            "export" => Some(SymbolKind::Export),
            "event" => Some(SymbolKind::Event),
            "delegate" => Some(SymbolKind::Delegate),
            _ => None,
        }
    }

    /// Convert from string representation (for database deserialization)
    #[allow(dead_code)] // TODO: Used for database deserialization
    pub fn from_string(s: &str) -> Self {
        Self::try_from_string(s).unwrap_or_else(|| panic!("unknown symbol kind: {s}"))
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
    Internal,
    FilePrivate,
    Open,
}

impl Visibility {
    pub fn as_storage_str(&self) -> &'static str {
        match self {
            Visibility::Public => "public",
            Visibility::Private => "private",
            Visibility::Protected => "protected",
            Visibility::Internal => "internal",
            Visibility::FilePrivate => "fileprivate",
            Visibility::Open => "open",
        }
    }

    pub fn from_storage_str(value: &str) -> Option<Self> {
        match value {
            "public" => Some(Visibility::Public),
            "private" => Some(Visibility::Private),
            "protected" => Some(Visibility::Protected),
            "internal" => Some(Visibility::Internal),
            "fileprivate" => Some(Visibility::FilePrivate),
            "open" => Some(Visibility::Open),
            _ => None,
        }
    }
}

impl std::fmt::Display for Visibility {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Visibility::Public => write!(f, "Public"),
            Visibility::Private => write!(f, "Private"),
            Visibility::Protected => write!(f, "Protected"),
            Visibility::Internal => write!(f, "Internal"),
            Visibility::FilePrivate => write!(f, "FilePrivate"),
            Visibility::Open => write!(f, "Open"),
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

/// A pending relationship that needs cross-file resolution after indexing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PendingRelationship {
    #[serde(rename = "fromSymbolId")]
    pub from_symbol_id: String,
    #[serde(rename = "calleeName")]
    pub callee_name: String,
    pub kind: RelationshipKind,
    #[serde(rename = "filePath")]
    pub file_path: String,
    #[serde(rename = "lineNumber")]
    pub line_number: u32,
    pub confidence: f32,
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
    pub fn try_from_string(s: &str) -> Option<Self> {
        match s {
            "calls" => Some(RelationshipKind::Calls),
            "extends" => Some(RelationshipKind::Extends),
            "implements" => Some(RelationshipKind::Implements),
            "uses" => Some(RelationshipKind::Uses),
            "returns" => Some(RelationshipKind::Returns),
            "parameter" => Some(RelationshipKind::Parameter),
            "imports" => Some(RelationshipKind::Imports),
            "instantiates" => Some(RelationshipKind::Instantiates),
            "references" => Some(RelationshipKind::References),
            "defines" => Some(RelationshipKind::Defines),
            "overrides" => Some(RelationshipKind::Overrides),
            "contains" => Some(RelationshipKind::Contains),
            "joins" => Some(RelationshipKind::Joins),
            "composition" => Some(RelationshipKind::Composition),
            _ => None,
        }
    }

    /// Convert from string representation (for database deserialization)
    #[allow(dead_code)] // TODO: Used for database deserialization
    pub fn from_string(s: &str) -> Self {
        Self::try_from_string(s).unwrap_or_else(|| panic!("unknown relationship kind: {s}"))
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
    pub annotations: Vec<AnnotationMarker>,
}

/// Extraction results - matches getResults return type
#[derive(Debug, Clone)]
pub struct ExtractionResults {
    pub symbols: Vec<Symbol>,
    pub relationships: Vec<Relationship>,
    /// Pending relationships that need cross-file resolution after workspace indexing
    pub pending_relationships: Vec<PendingRelationship>,
    /// Structured pending relationships preserve unresolved call context.
    pub structured_pending_relationships: Vec<StructuredPendingRelationship>,
    pub types: HashMap<String, TypeInfo>,
    pub identifiers: Vec<Identifier>, // Include identifiers for LSP-quality tools
    pub parse_diagnostics: Vec<ParseDiagnostic>,
}
