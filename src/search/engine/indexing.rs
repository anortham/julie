// DEPRECATED: All indexing methods moved to SearchIndexWriter in writer.rs
//
// This file is kept for the document_to_search_result helper which is still used by queries.
// All write operations (index_symbols, commit, delete_file_symbols) are now in SearchIndexWriter
// to eliminate RwLock contention during searches.

use super::result::SearchResult;
use super::SearchEngine;
use anyhow::Result;
use tantivy::schema::{Field, Value};

impl SearchEngine {
    /// Convert Tantivy document to search result with full Symbol data
    pub(super) fn document_to_search_result(
        &self,
        doc: &tantivy::TantivyDocument,
    ) -> Result<SearchResult> {
        let fields = self.schema.fields();

        let extract_text = |field: Field| -> String {
            doc.get_first(field)
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string()
        };

        let extract_optional_text = |field: Field| -> Option<String> {
            let text = extract_text(field);
            if text.is_empty() {
                None
            } else {
                Some(text)
            }
        };

        let extract_u64 = |field: Field| -> u32 {
            doc.get_first(field)
                .and_then(|value| value.as_u64())
                .unwrap_or(0) as u32
        };

        let extract_f64 = |field: Field| -> Option<f32> {
            doc.get_first(field)
                .and_then(|value| value.as_f64())
                .map(|value| value as f32)
        };

        let symbol_id = extract_text(fields.symbol_id);
        let symbol_name = extract_text(fields.symbol_name);
        let symbol_kind_str = extract_text(fields.symbol_kind);
        let language = extract_text(fields.language);
        let file_path = extract_text(fields.file_path);
        let signature = extract_optional_text(fields.signature);
        let doc_comment = extract_optional_text(fields.doc_comment);
        let code_context = extract_optional_text(fields.code_context);
        let semantic_group = extract_optional_text(fields.semantic_group);
        let confidence = extract_f64(fields.confidence);

        let start_line = extract_u64(fields.start_line);
        let end_line = extract_u64(fields.end_line);
        let start_column = extract_u64(fields.start_column);
        let end_column = extract_u64(fields.end_column);
        let start_byte = extract_u64(fields.start_byte);
        let end_byte = extract_u64(fields.end_byte);

        let visibility_str = extract_optional_text(fields.visibility);
        let parent_id = extract_optional_text(fields.parent_id);

        let symbol_kind = match symbol_kind_str.as_str() {
            "Class" => crate::extractors::base::SymbolKind::Class,
            "Interface" => crate::extractors::base::SymbolKind::Interface,
            "Function" => crate::extractors::base::SymbolKind::Function,
            "Method" => crate::extractors::base::SymbolKind::Method,
            "Variable" => crate::extractors::base::SymbolKind::Variable,
            "Field" => crate::extractors::base::SymbolKind::Field,
            "Property" => crate::extractors::base::SymbolKind::Property,
            "Type" => crate::extractors::base::SymbolKind::Type,
            "Enum" => crate::extractors::base::SymbolKind::Enum,
            "Constant" => crate::extractors::base::SymbolKind::Constant,
            "Namespace" => crate::extractors::base::SymbolKind::Namespace,
            "Import" => crate::extractors::base::SymbolKind::Import,
            _ => crate::extractors::base::SymbolKind::Function,
        };

        let visibility = visibility_str.and_then(|v| match v.as_str() {
            "Public" => Some(crate::extractors::base::Visibility::Public),
            "Private" => Some(crate::extractors::base::Visibility::Private),
            "Protected" => Some(crate::extractors::base::Visibility::Protected),
            _ => None,
        });

        let metadata = extract_optional_text(fields.metadata)
            .and_then(|meta| serde_json::from_str(&meta).ok());

        let snippet = signature.clone().unwrap_or_default();

        let symbol = crate::extractors::base::Symbol {
            id: symbol_id,
            name: symbol_name,
            kind: symbol_kind,
            language,
            file_path,
            start_line,
            start_column,
            end_line,
            end_column,
            start_byte,
            end_byte,
            signature,
            doc_comment,
            visibility,
            parent_id,
            metadata,
            semantic_group,
            confidence,
            code_context,
        };

        Ok(SearchResult {
            symbol,
            score: 0.0,
            snippet,
        })
    }
}
