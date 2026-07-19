use julie_extractors::base::{RelationshipKind, Symbol, SymbolKind, Visibility};
use crate::deep_dive::data::{RefEntry, SimilarEntry, SymbolContext};
use crate::deep_dive::formatting::format_symbol_context;

fn make_symbol(
    name: &str,
    kind: SymbolKind,
    file_path: &str,
    line: u32,
    signature: Option<&str>,
    visibility: Option<Visibility>,
    code_context: Option<&str>,
) -> Symbol {
    Symbol {
        id: format!("test_{}_{}", name, line),
        name: name.to_string(),
        kind,
        language: "rust".to_string(),
        file_path: file_path.to_string(),
        start_line: line,
        end_line: line + 10,
        start_column: 0,
        end_column: 0,
        start_byte: 0,
        end_byte: 100,
        parent_id: None,
        signature: signature.map(|s| s.to_string()),
        doc_comment: None,
        visibility,
        metadata: None,
        semantic_group: None,
        confidence: Some(0.9),
        code_context: code_context.map(|s| s.to_string()),
        content_type: None,
        body_span: None,
        body_hash: None,
        annotations: Vec::new(),
    }
}

fn make_ref(kind: RelationshipKind, file: &str, line: u32, sym: Option<Symbol>) -> RefEntry {
    RefEntry {
        kind,
        file_path: file.to_string(),
        line_number: line,
        symbol: sym,
    }
}

fn empty_context(symbol: Symbol) -> SymbolContext {
    SymbolContext {
        symbol,
        complexity: None,
        incoming: vec![],
        incoming_total: 0,
        incoming_calls_total: 0,
        outgoing: vec![],
        outgoing_total: 0,
        outgoing_calls_total: 0,
        children: vec![],
        implementations: vec![],
        test_refs: vec![],
        similar: vec![],
    }
}

mod callable_core;
mod kind_sections;
mod refs_quality_budget;
