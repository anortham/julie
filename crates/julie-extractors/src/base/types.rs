// Base Extractor Types for Julie
//
// All data structures for symbol extraction, identifiers, relationships, and types.
// Lines 15-394 from original base.rs

use md5;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::body::BodySpan;
use super::relationship_resolution::StructuredPendingRelationship;
use super::span::NormalizedSpan;
use super::type_models::{Literal, TypeArgumentUsage};

pub use super::kinds::{IdentifierKind, RelationshipKind, SymbolKind, TestRole, Visibility};

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
    /// Body span for body-bearing symbols.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_span: Option<BodySpan>,
    /// Formatting-insensitive hash of the body span token stream.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_hash: Option<String>,
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
    /// Ordered/nested generic type arguments captured at use sites (Miller
    /// bridge Phase 2). Carried out of the extractor's `BaseExtractor` so the
    /// indexing layer can flatten and persist them. Keyed to a use-site
    /// identifier by `identifier_id`.
    pub type_argument_usages: Vec<TypeArgumentUsage>,
    /// String literals captured at call-argument sites (Miller bridge Phase 3),
    /// config-free (carrier set, kind = Other). The indexing layer classifies +
    /// gates these by carrier before persistence.
    pub literals: Vec<Literal>,
    pub parse_diagnostics: Vec<ParseDiagnostic>,
}
