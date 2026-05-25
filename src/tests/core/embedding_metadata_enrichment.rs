//! Tests for variable embedding budget, test symbol exclusion, callee/field
//! enrichment, and extract_doc_excerpt (split from embedding_metadata.rs).

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::embeddings::metadata::{
        VariableEmbeddingPolicy, extract_doc_excerpt, has_simple_default_literal,
        is_test_symbol_for_embedding, prepare_batch_for_embedding, select_budgeted_variables,
    };
    use crate::extractors::{Symbol, SymbolKind};

    mod batch_enrichment;
    mod doc_excerpt;
    mod symbol_filtering;
    mod variable_budgeting;

    /// Helper: create a minimal test symbol.
    fn make_symbol(
        id: &str,
        name: &str,
        kind: SymbolKind,
        signature: Option<&str>,
        doc_comment: Option<&str>,
    ) -> Symbol {
        Symbol {
            id: id.to_string(),
            name: name.to_string(),
            kind,
            language: "rust".to_string(),
            file_path: "src/lib.rs".to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 10,
            end_column: 0,
            start_byte: 0,
            end_byte: 100,
            signature: signature.map(|s| s.to_string()),
            doc_comment: doc_comment.map(|s| s.to_string()),
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
            content_type: None,
            body_span: None,
            body_hash: None,
            annotations: Vec::new(),
        }
    }

    fn make_symbol_with_lang(id: &str, name: &str, kind: SymbolKind, language: &str) -> Symbol {
        let mut s = make_symbol(id, name, kind, None, None);
        s.language = language.to_string();
        s
    }
}
