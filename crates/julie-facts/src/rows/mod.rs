//! Row types read back from `facts.sqlite`, plus the normalization applied to
//! extraction results before their rows are written.

mod test_roles;
mod type_arguments;

use std::collections::HashMap;

use julie_extractors::{
    AnnotationMarker, ExtractionResults, IdentifierKind, NormalizedSpan, ParseDiagnosticKind,
    RelationshipKind, SymbolKind, Visibility,
};
use serde_json::Value;

pub use test_roles::{TestRoleConfig, classify_symbols_by_role};
pub use type_arguments::{FlatTypeArgument, flatten_type_argument_usages};

/// Per-language classifier inputs applied before rows are written. Literal
/// carrier policy is owned by `julie_extractors` and needs no input here.
#[derive(Debug, Clone, Default)]
pub struct Normalization {
    pub test_roles: HashMap<String, TestRoleConfig>,
}

impl Normalization {
    pub fn apply(&self, results: &mut ExtractionResults) {
        julie_extractors::classify_literals_by_carrier(&mut results.literals);
        classify_symbols_by_role(&mut results.symbols, &self.test_roles);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start_line: u32,
    pub start_col: u32,
    pub end_line: u32,
    pub end_col: u32,
    pub start_byte: u32,
    pub end_byte: u32,
}

impl From<NormalizedSpan> for Span {
    fn from(s: NormalizedSpan) -> Self {
        Self {
            start_line: s.start_line,
            start_col: s.start_column,
            end_line: s.end_line,
            end_col: s.end_column,
            start_byte: s.start_byte,
            end_byte: s.end_byte,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SymbolRow {
    /// `"<blob_hash>:<ordinal>"`.
    pub id: String,
    pub blob_hash: String,
    pub ordinal: u32,
    pub path: String,
    pub language: String,
    pub name: String,
    pub kind: SymbolKind,
    pub span: Span,
    pub body_span: Option<Span>,
    pub body_hash: Option<String>,
    pub signature: Option<String>,
    pub doc_comment: Option<String>,
    pub visibility: Option<Visibility>,
    pub parent_ordinal: Option<u32>,
    pub annotations: Vec<AnnotationMarker>,
    pub metadata: Option<HashMap<String, Value>>,
    pub semantic_group: Option<String>,
    pub confidence: Option<f32>,
    pub content_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IdentifierRow {
    pub id: String,
    pub blob_hash: String,
    pub ordinal: u32,
    pub path: String,
    pub language: String,
    pub name: String,
    pub kind: IdentifierKind,
    pub span: Span,
    pub containing_ordinal: Option<u32>,
    pub receiver_type: Option<String>,
    pub code_context: Option<String>,
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RelationshipRow {
    pub blob_hash: String,
    pub ordinal: u32,
    pub path: String,
    pub from_ordinal: Option<u32>,
    pub to_name: String,
    pub to_blob_hash: Option<String>,
    pub to_ordinal: Option<u32>,
    pub kind: RelationshipKind,
    pub line_number: u32,
    pub span: Option<Span>,
    pub reference_site_is_exact: bool,
    pub confidence: f32,
    pub metadata: Option<HashMap<String, Value>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeRow {
    pub blob_hash: String,
    pub symbol_ordinal: u32,
    pub path: String,
    pub resolved_type: String,
    pub generic_params: Option<Vec<String>>,
    pub constraints: Option<Vec<String>>,
    pub is_inferred: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourceRegionRow {
    pub blob_hash: String,
    pub ordinal: u32,
    pub path: String,
    pub kind: String,
    pub containing_ordinal: Option<u32>,
    pub span: Span,
    pub metadata: Option<HashMap<String, Value>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructuralFactRow {
    pub blob_hash: String,
    pub ordinal: u32,
    pub path: String,
    pub language: String,
    pub pattern_id: String,
    pub capture_name: String,
    pub node_kind: String,
    pub containing_ordinal: Option<u32>,
    pub span: Span,
    pub confidence: f32,
    pub metadata: Option<HashMap<String, Value>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComplexityRow {
    pub blob_hash: String,
    pub ordinal: u32,
    pub scope: String,
    pub symbol_ordinal: Option<u32>,
    pub algorithm_id: String,
    pub covered_lines: u32,
    pub covered_bytes: u32,
    pub decision_count: u32,
    pub loop_count: u32,
    pub max_nesting_depth: u32,
    pub parameter_count: Option<u32>,
    pub span: Span,
    pub metadata: Option<HashMap<String, Value>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticRow {
    pub blob_hash: String,
    pub ordinal: u32,
    pub path: String,
    pub kind: ParseDiagnosticKind,
    pub message: Option<String>,
    pub span: Span,
}

/// The one encoder whose vectors the store holds. `id` is the encoder's
/// storage key; the other columns describe the identity behind it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncoderRow {
    pub id: String,
    pub model_checksum: String,
    pub dimensions: u32,
    pub pooling: String,
    pub normalization: String,
    pub instruction_policy: String,
}

/// One symbol embedding keyed by the symbol's blob and ordinal.
#[derive(Debug, Clone, PartialEq)]
pub struct VectorRow {
    pub blob_hash: String,
    pub symbol_ordinal: u32,
    pub vector: Vec<f32>,
}

impl VectorRow {
    /// The `"<blob_hash>:<ordinal>"` id of the symbol this vector belongs to.
    pub fn symbol_id(&self) -> String {
        format!("{}:{}", self.blob_hash, self.symbol_ordinal)
    }

    /// Little-endian f32 bytes, the on-disk form of `vector`.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.vector.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    pub fn vector_from_bytes(bytes: &[u8]) -> Vec<f32> {
        bytes
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathRow {
    pub path: String,
    pub blob_hash: String,
    pub language: String,
}

/// Filter for `FactsReader::structural_facts`. Empty `pattern_ids` matches
/// every pattern; `path_pattern` is a SQLite GLOB over `paths.path`.
#[derive(Debug, Clone)]
pub struct StructuralFactQuery {
    pub pattern_ids: Vec<String>,
    pub path_pattern: Option<String>,
    pub language: Option<String>,
    pub limit: usize,
}

impl Default for StructuralFactQuery {
    fn default() -> Self {
        Self {
            pattern_ids: Vec::new(),
            path_pattern: None,
            language: None,
            limit: 50,
        }
    }
}

pub fn diagnostic_kind_str(kind: ParseDiagnosticKind) -> &'static str {
    match kind {
        ParseDiagnosticKind::Error => "error",
        ParseDiagnosticKind::Missing => "missing",
        ParseDiagnosticKind::DepthTruncated => "depth_truncated",
    }
}

pub fn diagnostic_kind_from_str(value: &str) -> ParseDiagnosticKind {
    match value {
        "missing" => ParseDiagnosticKind::Missing,
        "depth_truncated" => ParseDiagnosticKind::DepthTruncated,
        _ => ParseDiagnosticKind::Error,
    }
}
