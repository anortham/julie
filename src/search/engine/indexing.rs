use super::result::SearchResult;
use super::SearchEngine;
use crate::extractors::Symbol;
use anyhow::Result;
use tantivy::schema::{Field, Value};
use tantivy::Term;
use tracing::debug;

impl SearchEngine {
    /// Index a batch of symbols
    pub async fn index_symbols(&mut self, symbols: Vec<Symbol>) -> Result<()> {
        let symbol_count = symbols.len();

        for (index, symbol) in symbols.into_iter().enumerate() {
            if index % 100 == 0 {
                debug!("📝 Indexed {}/{} symbols", index, symbol_count);
            }

            let doc = super::super::schema::SearchDocument {
                symbol_id: symbol.id.clone(),
                symbol_name: symbol.name.clone(),
                symbol_kind: symbol.kind.to_string(),
                language: symbol.language.clone(),
                file_path: symbol.file_path.clone(),
                signature: symbol.signature.clone(),
                doc_comment: symbol.doc_comment.clone(),
                code_context: symbol.code_context.clone(),
                start_line: symbol.start_line,
                end_line: symbol.end_line,
                start_column: symbol.start_column,
                end_column: symbol.end_column,
                start_byte: symbol.start_byte,
                end_byte: symbol.end_byte,
                visibility: symbol.visibility.map(|v| v.to_string()),
                parent_id: symbol.parent_id.clone(),
                metadata: symbol
                    .metadata
                    .as_ref()
                    .map(|metadata| serde_json::to_string(metadata).unwrap_or_default()),
                semantic_group: symbol.semantic_group.clone(),
                confidence: symbol.confidence.map(|c| c as f64),
            };

            self.add_document(doc)?;
        }

        self.commit().await?;
        Ok(())
    }

    /// Add a single document to the index
    fn add_document(&mut self, doc: super::super::schema::SearchDocument) -> Result<()> {
        let mut tantivy_doc = tantivy::doc!();
        let fields = self.schema.fields();

        tantivy_doc.add_text(fields.symbol_id, &doc.symbol_id);
        tantivy_doc.add_text(fields.symbol_name, &doc.symbol_name);
        tantivy_doc.add_text(fields.symbol_name_exact, &doc.symbol_name);
        tantivy_doc.add_text(fields.symbol_kind, &doc.symbol_kind);
        tantivy_doc.add_text(fields.language, &doc.language);
        tantivy_doc.add_text(fields.file_path, &doc.file_path);
        tantivy_doc.add_text(fields.file_path_exact, &doc.file_path);

        if let Some(signature) = &doc.signature {
            tantivy_doc.add_text(fields.signature, signature);
            tantivy_doc.add_text(fields.signature_exact, signature);
        }

        if let Some(doc_comment) = &doc.doc_comment {
            tantivy_doc.add_text(fields.doc_comment, doc_comment);
        }

        if let Some(context) = &doc.code_context {
            tantivy_doc.add_text(fields.code_context, context);
        }

        tantivy_doc.add_u64(fields.start_line, doc.start_line as u64);
        tantivy_doc.add_u64(fields.end_line, doc.end_line as u64);
        tantivy_doc.add_u64(fields.start_column, doc.start_column as u64);
        tantivy_doc.add_u64(fields.end_column, doc.end_column as u64);
        tantivy_doc.add_u64(fields.start_byte, doc.start_byte as u64);
        tantivy_doc.add_u64(fields.end_byte, doc.end_byte as u64);

        if let Some(visibility) = &doc.visibility {
            tantivy_doc.add_text(fields.visibility, visibility);
        }

        if let Some(parent_id) = &doc.parent_id {
            tantivy_doc.add_text(fields.parent_id, parent_id);
        }

        if let Some(metadata) = &doc.metadata {
            tantivy_doc.add_text(fields.metadata, metadata);
        }

        if let Some(semantic_group) = &doc.semantic_group {
            tantivy_doc.add_text(fields.semantic_group, semantic_group);
        }

        if let Some(confidence) = doc.confidence {
            tantivy_doc.add_f64(fields.confidence, confidence);
        }

        let all_text = doc.generate_all_text();
        tantivy_doc.add_text(fields.all_text, &all_text);

        let exact_matches = doc.generate_exact_matches();
        tantivy_doc.add_text(fields.exact_matches, &exact_matches);

        let language_boost = self.language_boosting.get_language_boost(&doc.language);
        tantivy_doc.add_f64(fields.language_boost, language_boost);

        self.writer.add_document(tantivy_doc)?;
        Ok(())
    }

    /// Commit pending changes to the index
    pub async fn commit(&mut self) -> Result<()> {
        self.writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    /// Delete symbols for a file (for incremental updates)
    pub async fn delete_file_symbols(&mut self, file_path: &str) -> Result<()> {
        let fields = self.schema.fields();
        let term = Term::from_field_text(fields.file_path_exact, file_path);
        self.writer.delete_term(term);
        Ok(())
    }

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
