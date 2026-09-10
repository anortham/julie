//! Search documents built from facts rows plus the file's text. Same field
//! set as the database-backed builders in `apply.rs`; those go with Task 13.

use std::collections::HashSet;

use julie_extractors::AnnotationMarker;
use julie_facts::rows::{Span, SymbolRow};
use tantivy::schema::TantivyDocument;

use super::apply::RELATIONSHIP_TEXT_MAX_BYTES;
use super::facts_text::truncate_to_whitespace_boundary;
use crate::graph::FileRows;
use crate::search::index::{SearchDocument, declaration_role_from_metadata, truncate_utf8_bytes};
use crate::search::schema::SchemaFields;
use crate::search::scoring::{classify_role, test_subrole};
use crate::search::tokenizer::pretokenize_code;

const CODE_BODY_MAX_BYTES: usize = 2000;

fn basename_of(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

pub fn file_document(path: &str, language: &str, text: &str) -> SearchDocument {
    let basename = basename_of(path);
    let name = match basename.rfind('.') {
        Some(dot) => basename[..dot].to_string(),
        None => basename.clone(),
    };
    SearchDocument {
        doc_type: "file".to_string(),
        id: String::new(),
        name,
        language: language.to_string(),
        file_path: path.to_string(),
        basename,
        kind: "file".to_string(),
        role: classify_role(path, language).to_string(),
        test_role: test_subrole(path).to_string(),
        declaration_role: String::new(),
        signature: String::new(),
        doc_comment: String::new(),
        code_body: String::new(),
        annotation_keys: Vec::new(),
        annotations_text: String::new(),
        owner_names_text: String::new(),
        start_line: 0,
        content: text.to_string(),
        path_text: path.to_string(),
        pretokenized_code: pretokenize_code(truncate_utf8_bytes(text, CODE_BODY_MAX_BYTES)),
        relationship_text: String::new(),
    }
}

fn slice(text: &str, span: Span) -> &str {
    text.get(span.start_byte as usize..span.end_byte as usize)
        .unwrap_or("")
}

fn row_at(rows: &FileRows, ordinal: u32) -> Option<&SymbolRow> {
    let symbols = &rows.symbols;
    match symbols.get(ordinal as usize) {
        Some(row) if row.ordinal == ordinal => Some(row),
        _ => symbols
            .binary_search_by_key(&ordinal, |row| row.ordinal)
            .ok()
            .map(|at| &symbols[at]),
    }
}

fn push_nonempty(parts: &mut Vec<String>, value: &str) {
    let value = value.trim();
    if !value.is_empty() {
        parts.push(value.to_string());
    }
}

fn annotation_text(annotations: &[AnnotationMarker]) -> (Vec<String>, String) {
    let mut keys = Vec::new();
    let mut seen = HashSet::new();
    let mut parts = Vec::new();
    for marker in annotations {
        let key = marker.annotation_key.trim().to_ascii_lowercase();
        if !key.is_empty() && seen.insert(key.clone()) {
            keys.push(key);
        }
        push_nonempty(&mut parts, &marker.annotation);
        push_nonempty(&mut parts, &marker.annotation_key);
        if let Some(raw) = &marker.raw_text {
            push_nonempty(&mut parts, raw);
        }
    }
    (keys, parts.join(" "))
}

fn owner_names(rows: &FileRows, row: &SymbolRow) -> String {
    let mut names = Vec::new();
    let mut seen = HashSet::new();
    let mut parent = row.parent_ordinal;
    while let Some(ordinal) = parent {
        if !seen.insert(ordinal) {
            break;
        }
        let Some(owner) = row_at(rows, ordinal) else {
            break;
        };
        push_nonempty(&mut names, &owner.name);
        parent = owner.parent_ordinal;
    }
    names.join(" ")
}

fn relationship_text(rows: &FileRows, ordinal: u32) -> String {
    let mut names: Vec<&str> = rows
        .relationships
        .iter()
        .filter(|rel| rel.from_ordinal == Some(ordinal))
        .map(|rel| rel.to_name.as_str())
        .collect();
    names.sort_unstable();
    names.dedup();
    truncate_to_whitespace_boundary(&names.join(" "), RELATIONSHIP_TEXT_MAX_BYTES).to_string()
}

fn symbol_document(rows: &FileRows, row: &SymbolRow, text: &str) -> SearchDocument {
    let path = row.path.as_str();
    let path_role = classify_role(path, &row.language);
    let path_test_role = test_subrole(path);
    let meta = row.metadata.as_ref();
    let metadata_is_test = meta
        .and_then(|m| m.get("is_test"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let metadata_test_role = meta
        .and_then(|m| m.get("test_role"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let test_role = metadata_test_role.unwrap_or_else(|| path_test_role.to_string());
    let role = if metadata_is_test { "test" } else { path_role };

    let code_body = truncate_utf8_bytes(
        slice(text, row.body_span.unwrap_or(row.span)),
        CODE_BODY_MAX_BYTES,
    )
    .to_string();
    let signature = row.signature.clone().unwrap_or_default();
    let (annotation_keys, annotations_text) = annotation_text(&row.annotations);

    SearchDocument {
        doc_type: "symbol".to_string(),
        id: row.id.clone(),
        name: row.name.clone(),
        language: row.language.clone(),
        file_path: path.to_string(),
        basename: basename_of(path),
        kind: row.kind.to_string(),
        role: role.to_string(),
        test_role,
        declaration_role: declaration_role_from_metadata(meta),
        pretokenized_code: pretokenize_code(&format!("{} {} {}", row.name, signature, code_body)),
        signature,
        doc_comment: row.doc_comment.clone().unwrap_or_default(),
        code_body,
        annotation_keys,
        annotations_text,
        owner_names_text: owner_names(rows, row),
        start_line: row.span.start_line,
        content: String::new(),
        path_text: String::new(),
        relationship_text: relationship_text(rows, row.ordinal),
    }
}

/// The file document plus one document per symbol row of `rows`.
pub fn documents(rows: &FileRows, language: &str, text: &str) -> Vec<SearchDocument> {
    let mut docs = Vec::with_capacity(rows.symbols.len() + 1);
    docs.push(file_document(&rows.path, language, text));
    docs.extend(
        rows.symbols
            .iter()
            .map(|row| symbol_document(rows, row, text)),
    );
    docs
}

pub fn tantivy_document(fields: &SchemaFields, doc: &SearchDocument) -> TantivyDocument {
    let mut out = TantivyDocument::new();
    out.add_text(fields.doc_type, &doc.doc_type);
    out.add_text(fields.id, &doc.id);
    out.add_text(fields.file_path, &doc.file_path);
    out.add_text(fields.basename, &doc.basename);
    out.add_text(fields.language, &doc.language);
    out.add_text(fields.kind, &doc.kind);
    out.add_text(fields.role, &doc.role);
    out.add_text(fields.test_role, &doc.test_role);
    out.add_text(fields.declaration_role, &doc.declaration_role);
    out.add_text(fields.name, &doc.name);
    out.add_text(fields.signature, &doc.signature);
    out.add_text(fields.doc_comment, &doc.doc_comment);
    out.add_text(fields.code_body, &doc.code_body);
    for key in &doc.annotation_keys {
        out.add_text(fields.annotations_exact, key);
    }
    out.add_text(fields.annotations_text, &doc.annotations_text);
    out.add_text(fields.owner_names_text, &doc.owner_names_text);
    out.add_u64(fields.start_line, doc.start_line as u64);
    out.add_text(fields.path_text, &doc.path_text);
    out.add_text(fields.content, &doc.content);
    out.add_text(fields.pretokenized_code, &doc.pretokenized_code);
    out.add_text(fields.relationship_text, &doc.relationship_text);
    out
}
