use crate::base::body::{body_hash, infer_body_span_from_span};
use crate::base::{BaseExtractor, NormalizedSpan, Symbol, SymbolKind, SymbolOptions, Visibility};
use serde_json::Value;
use std::collections::HashMap;

#[allow(clippy::too_many_arguments)] // Matches API for compatibility
pub(super) fn create_symbol_manual(
    base: &BaseExtractor,
    name: &str,
    kind: SymbolKind,
    start_line: usize,
    start_column: usize,
    end_line: usize,
    end_column: usize,
    signature: Option<String>,
    documentation: Option<String>,
    metadata: Option<HashMap<String, Value>>,
) -> Symbol {
    let options = SymbolOptions {
        signature,
        doc_comment: documentation,
        visibility: Some(Visibility::Public),
        parent_id: None,
        metadata,
        annotations: Vec::new(),
    };

    let start_byte = byte_for_position(&base.content, start_line, start_column).unwrap_or(0);
    let mut end_byte = byte_for_position(&base.content, end_line, end_column).unwrap_or(start_byte);
    if end_byte <= start_byte {
        end_byte = start_byte.saturating_add(name.len() as u32);
    }

    let span = NormalizedSpan {
        start_line: start_line as u32,
        start_column: start_column as u32,
        end_line: end_line as u32,
        end_column: end_column as u32,
        start_byte,
        end_byte,
    };
    let id = base.generate_id_for_span(name, &span);
    let body_span = infer_body_span_from_span(&base.content, span);
    let body_hash = body_span.and_then(|span| body_hash(&base.content, span));

    Symbol {
        id,
        name: name.to_string(),
        kind,
        language: base.language.clone(),
        file_path: base.file_path.clone(),
        start_line: start_line as u32,
        start_column: start_column as u32,
        end_line: end_line as u32,
        end_column: end_column as u32,
        start_byte,
        end_byte,
        body_span,
        body_hash,
        signature: options.signature,
        doc_comment: options.doc_comment,
        visibility: options.visibility,
        parent_id: options.parent_id,
        metadata: Some(options.metadata.unwrap_or_default()),
        annotations: options.annotations,
        semantic_group: None,
        confidence: None,
        code_context: None,
        content_type: None,
    }
}

fn byte_for_position(content: &str, line: usize, column: usize) -> Option<u32> {
    let mut byte = 0usize;
    for (idx, current_line) in content.split_inclusive('\n').enumerate() {
        if idx + 1 == line {
            return Some((byte + column.saturating_sub(1).min(current_line.len())) as u32);
        }
        byte += current_line.len();
    }

    if line == content.lines().count() + 1 {
        Some(content.len() as u32)
    } else {
        None
    }
}
