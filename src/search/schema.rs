//! Tantivy schema definition for code symbol and file content indexing.
//!
//! Two document types share the same index, distinguished by a `doc_type` field:
//! - `"symbol"`: Code symbols (functions, classes, structs, etc.)
//! - `"file"`: Full file content for line-level search

use tantivy::schema::{
    Field, IndexRecordOption, NumericOptions, Schema, TextFieldIndexing, TextOptions, STORED,
    STRING,
};

/// Field name constants for the search schema.
pub mod fields {
    pub const DOC_TYPE: &str = "doc_type";
    pub const ID: &str = "id";
    pub const FILE_PATH: &str = "file_path";
    pub const LANGUAGE: &str = "language";
    pub const NAME: &str = "name";
    pub const SIGNATURE: &str = "signature";
    pub const DOC_COMMENT: &str = "doc_comment";
    pub const CODE_BODY: &str = "code_body";
    pub const KIND: &str = "kind";
    pub const START_LINE: &str = "start_line";
    pub const CONTENT: &str = "content";
}

/// Build the Tantivy schema for code search.
///
/// Uses the `"code"` tokenizer for text fields that need code-aware tokenization
/// (CamelCase splitting, snake_case splitting, operator preservation).
/// Uses `STRING` (raw tokenizer) for fields that should be matched exactly
/// (doc_type, id, file_path, language, kind).
pub fn create_schema() -> Schema {
    let mut builder = Schema::builder();

    let code_text_options = TextOptions::default()
        .set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("code")
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        )
        .set_stored();

    let code_text_not_stored = TextOptions::default().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer("code")
            .set_index_option(IndexRecordOption::WithFreqsAndPositions),
    );

    // Common fields (exact match, stored)
    builder.add_text_field(fields::DOC_TYPE, STRING | STORED);
    builder.add_text_field(fields::ID, STRING | STORED);
    builder.add_text_field(fields::FILE_PATH, STRING | STORED);
    builder.add_text_field(fields::LANGUAGE, STRING | STORED);

    // Symbol fields (code-tokenized)
    builder.add_text_field(fields::NAME, code_text_options.clone());
    builder.add_text_field(fields::SIGNATURE, code_text_options.clone());
    builder.add_text_field(fields::DOC_COMMENT, code_text_options.clone());
    builder.add_text_field(fields::CODE_BODY, code_text_not_stored.clone());
    builder.add_text_field(fields::KIND, STRING | STORED);
    builder.add_u64_field(fields::START_LINE, NumericOptions::default().set_stored());

    // File content field (code-tokenized, not stored)
    builder.add_text_field(fields::CONTENT, code_text_not_stored);

    builder.build()
}

/// Pre-resolved field handles for efficient document construction and retrieval.
#[derive(Clone)]
pub struct SchemaFields {
    pub doc_type: Field,
    pub id: Field,
    pub file_path: Field,
    pub language: Field,
    pub name: Field,
    pub signature: Field,
    pub doc_comment: Field,
    pub code_body: Field,
    pub kind: Field,
    pub start_line: Field,
    pub content: Field,
}

impl SchemaFields {
    /// Resolve all field handles from a schema.
    ///
    /// # Panics
    /// Panics if the schema was not created by `create_schema()`.
    pub fn new(schema: &Schema) -> Self {
        Self {
            doc_type: schema.get_field(fields::DOC_TYPE).unwrap(),
            id: schema.get_field(fields::ID).unwrap(),
            file_path: schema.get_field(fields::FILE_PATH).unwrap(),
            language: schema.get_field(fields::LANGUAGE).unwrap(),
            name: schema.get_field(fields::NAME).unwrap(),
            signature: schema.get_field(fields::SIGNATURE).unwrap(),
            doc_comment: schema.get_field(fields::DOC_COMMENT).unwrap(),
            code_body: schema.get_field(fields::CODE_BODY).unwrap(),
            kind: schema.get_field(fields::KIND).unwrap(),
            start_line: schema.get_field(fields::START_LINE).unwrap(),
            content: schema.get_field(fields::CONTENT).unwrap(),
        }
    }
}
