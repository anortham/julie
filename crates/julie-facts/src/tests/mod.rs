mod reader;
mod schema;
mod version;
mod writer;

use std::cell::Cell;
use std::collections::HashMap;

use julie_extractors::{
    AnnotationMarker, ComplexityMetric, ExtractionResults, Identifier, IdentifierKind, Literal,
    LiteralKind, NormalizedSpan, ParseDiagnostic, ParseDiagnosticKind, PendingRelationship,
    Relationship, RelationshipKind, SourceRegion, SourceRegionKind, StructuralFact, Symbol,
    SymbolKind, TypeArgument, TypeArgumentUsage, TypeInfo,
};

use crate::writer::{Extractor, PathChange};

pub(crate) const OUTER_ID: &str = "sym-outer";
pub(crate) const INNER_ID: &str = "sym-inner";
pub(crate) const IDENT_ID: &str = "ident-1";

pub(crate) struct FakeExtractor {
    pub calls: Cell<usize>,
}

impl FakeExtractor {
    pub fn new() -> Self {
        Self {
            calls: Cell::new(0),
        }
    }
}

impl Extractor for FakeExtractor {
    fn extract(&self, path: &str, _content: &str, language: &str) -> ExtractionResults {
        self.calls.set(self.calls.get() + 1);
        fixture_results(path, language)
    }
}

pub(crate) fn upsert(path: &str, bytes: &str) -> PathChange {
    PathChange::Upsert {
        path: path.to_string(),
        bytes: bytes.as_bytes().to_vec(),
        language: "typescript".to_string(),
    }
}

pub(crate) fn hash_of(bytes: &str) -> String {
    blake3::hash(bytes.as_bytes()).to_hex().to_string()
}

fn span(line: u32) -> NormalizedSpan {
    NormalizedSpan {
        start_line: line,
        start_column: 0,
        end_line: line + 1,
        end_column: 4,
        start_byte: line * 10,
        end_byte: line * 10 + 8,
    }
}

fn symbol(
    id: &str,
    name: &str,
    kind: SymbolKind,
    parent: Option<&str>,
    path: &str,
    language: &str,
) -> Symbol {
    let s = span(1);
    Symbol {
        id: id.to_string(),
        name: name.to_string(),
        kind,
        language: language.to_string(),
        file_path: path.to_string(),
        start_line: s.start_line,
        start_column: s.start_column,
        end_line: s.end_line,
        end_column: s.end_column,
        start_byte: s.start_byte,
        end_byte: s.end_byte,
        body_span: Some(span(2)),
        body_hash: Some("bodyhash".to_string()),
        signature: Some(format!("{name}()")),
        doc_comment: None,
        visibility: Some(julie_extractors::Visibility::Public),
        parent_id: parent.map(str::to_string),
        metadata: None,
        annotations: Vec::new(),
        semantic_group: None,
        confidence: Some(0.9),
        content_type: None,
    }
}

pub(crate) fn fixture_results(path: &str, language: &str) -> ExtractionResults {
    let outer = symbol(OUTER_ID, "Outer", SymbolKind::Class, None, path, language);
    let mut inner = symbol(
        INNER_ID,
        "inner",
        SymbolKind::Method,
        Some(OUTER_ID),
        path,
        language,
    );
    inner.annotations.push(AnnotationMarker {
        annotation: "@test".to_string(),
        annotation_key: "test".to_string(),
        raw_text: None,
        carrier: None,
    });
    let s = span(3);
    let identifier = Identifier {
        id: IDENT_ID.to_string(),
        name: "callee".to_string(),
        kind: IdentifierKind::Call,
        language: language.to_string(),
        file_path: path.to_string(),
        start_line: s.start_line,
        start_column: s.start_column,
        end_line: s.end_line,
        end_column: s.end_column,
        start_byte: s.start_byte,
        end_byte: s.end_byte,
        containing_symbol_id: Some(INNER_ID.to_string()),
        target_symbol_id: None,
        confidence: 1.0,
        receiver_type: Some("Outer".to_string()),
        code_context: None,
    };
    let relationship = Relationship {
        id: "rel-1".to_string(),
        from_symbol_id: INNER_ID.to_string(),
        to_symbol_id: OUTER_ID.to_string(),
        kind: RelationshipKind::Calls,
        file_path: path.to_string(),
        line_number: 3,
        span: Some(span(3)),
        reference_site_is_exact: true,
        confidence: 1.0,
        metadata: None,
    };
    let pending = PendingRelationship {
        from_symbol_id: INNER_ID.to_string(),
        callee_name: "remote_fn".to_string(),
        kind: RelationshipKind::Calls,
        file_path: path.to_string(),
        line_number: 4,
        confidence: 0.8,
    };
    let mut types = HashMap::new();
    types.insert(
        INNER_ID.to_string(),
        TypeInfo {
            symbol_id: INNER_ID.to_string(),
            resolved_type: "i32".to_string(),
            generic_params: Some(vec!["T".to_string()]),
            constraints: None,
            is_inferred: false,
            language: language.to_string(),
            metadata: None,
        },
    );
    let literal = |carrier: &str| Literal {
        id: format!("lit-{carrier}"),
        literal_text: "https://x".to_string(),
        kind: LiteralKind::Other,
        carrier: Some(carrier.to_string()),
        arg_position: 0,
        language: language.to_string(),
        file_path: path.to_string(),
        start_line: 5,
        start_column: 0,
        end_line: 5,
        end_column: 9,
        start_byte: 50,
        end_byte: 59,
        containing_symbol_id: Some(INNER_ID.to_string()),
        confidence: 1.0,
    };
    let s = span(6);
    ExtractionResults {
        symbols: vec![outer, inner],
        relationships: vec![relationship],
        pending_relationships: vec![pending],
        structured_pending_relationships: Vec::new(),
        types,
        identifiers: vec![identifier],
        type_argument_usages: vec![TypeArgumentUsage {
            identifier_id: IDENT_ID.to_string(),
            file_path: path.to_string(),
            language: language.to_string(),
            arguments: vec![TypeArgument {
                ordinal: 0,
                type_name: "List".to_string(),
                children: vec![TypeArgument {
                    ordinal: 0,
                    type_name: "int".to_string(),
                    children: Vec::new(),
                }],
            }],
        }],
        literals: vec![literal("fetch"), literal("nope")],
        source_regions: vec![SourceRegion {
            id: "region-1".to_string(),
            file_path: path.to_string(),
            language: language.to_string(),
            kind: SourceRegionKind::Comment,
            containing_symbol_id: Some(OUTER_ID.to_string()),
            start_line: s.start_line,
            start_column: s.start_column,
            end_line: s.end_line,
            end_column: s.end_column,
            start_byte: s.start_byte,
            end_byte: s.end_byte,
            metadata: None,
        }],
        structural_facts: vec![StructuralFact {
            id: "fact-1".to_string(),
            file_path: path.to_string(),
            language: language.to_string(),
            pattern_id: "route".to_string(),
            capture_name: "handler".to_string(),
            node_kind: "call".to_string(),
            containing_symbol_id: Some(INNER_ID.to_string()),
            start_line: s.start_line,
            start_column: s.start_column,
            end_line: s.end_line,
            end_column: s.end_column,
            start_byte: s.start_byte,
            end_byte: s.end_byte,
            confidence: 0.7,
            metadata: None,
        }],
        complexity_metrics: vec![ComplexityMetric {
            id: "cx-1".to_string(),
            file_path: path.to_string(),
            language: language.to_string(),
            scope: "symbol".to_string(),
            symbol_id: Some(INNER_ID.to_string()),
            algorithm_id: "cyclomatic".to_string(),
            covered_lines: 3,
            covered_bytes: 30,
            decision_count: 2,
            loop_count: 1,
            max_nesting_depth: 1,
            parameter_count: Some(0),
            start_line: s.start_line,
            start_column: s.start_column,
            end_line: s.end_line,
            end_column: s.end_column,
            start_byte: s.start_byte,
            end_byte: s.end_byte,
            metadata: None,
        }],
        parse_diagnostics: vec![ParseDiagnostic {
            kind: ParseDiagnosticKind::Error,
            message: Some("unexpected".to_string()),
            start_line: 7,
            start_column: 0,
            end_line: 7,
            end_column: 1,
            start_byte: 70,
            end_byte: 71,
        }],
    }
}
