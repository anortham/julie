//! Phase 2 unified schema — new field presence tests.
//!
//! Verifies:
//! - `pretokenized_code` and `relationship_text` fields exist in the schema.
//! - Adding these fields changes `compatibility_signature` relative to a
//!   hand-built schema that lacks them (regression guard: ensures the
//!   signature is schema-content-sensitive).

use crate::search::schema::{compatibility_signature, create_schema};
use tantivy::schema::{IndexRecordOption, STORED, STRING, Schema, TextFieldIndexing, TextOptions};

// ---------------------------------------------------------------------------
// Field presence
// ---------------------------------------------------------------------------

#[test]
fn schema_contains_pretokenized_code_field() {
    let schema = create_schema();
    assert!(
        schema.get_field("pretokenized_code").is_ok(),
        "schema must define `pretokenized_code` field"
    );
}

#[test]
fn pretokenized_code_uses_simple_code_tokenizer() {
    let schema = create_schema();
    let field = schema
        .get_field("pretokenized_code")
        .expect("pretokenized_code field");
    let entry = schema.get_field_entry(field);
    let field_type = format!("{:?}", entry.field_type());

    assert!(
        field_type.contains("simple_code"),
        "pretokenized_code must use simple_code tokenizer, got {field_type}"
    );
    assert!(
        !field_type.contains("tokenizer: \"code\""),
        "pretokenized_code must not use legacy code tokenizer, got {field_type}"
    );
}

#[test]
fn schema_contains_relationship_text_field() {
    let schema = create_schema();
    assert!(
        schema.get_field("relationship_text").is_ok(),
        "schema must define `relationship_text` field"
    );
}

// ---------------------------------------------------------------------------
// SchemaFields struct wires through both new fields
// ---------------------------------------------------------------------------

#[test]
fn schema_fields_struct_resolves_phase2_fields() {
    // SchemaFields::new() panics if any field is missing — constructing it
    // is proof the new fields are wired through end-to-end.
    let schema = create_schema();
    let _fields = crate::search::SchemaFields::new(&schema);
}

// ---------------------------------------------------------------------------
// compatibility_signature is sensitive to the new fields
// ---------------------------------------------------------------------------

#[test]
fn compatibility_signature_differs_without_new_fields() {
    // Build a "pre-phase-2" schema that is identical to the current one but
    // lacks pretokenized_code and relationship_text.
    let mut builder = Schema::builder();

    let code_text_not_stored = TextOptions::default().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer("code")
            .set_index_option(IndexRecordOption::WithFreqsAndPositions),
    );
    let code_text_stored = TextOptions::default()
        .set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("code")
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        )
        .set_stored();

    // Replicate all current fields (but not pretokenized_code / relationship_text).
    builder.add_text_field("doc_type", STRING | STORED);
    builder.add_text_field("id", STRING | STORED);
    builder.add_text_field("file_path", STRING | STORED);
    builder.add_text_field("basename", STRING | STORED);
    builder.add_text_field("path_text", code_text_not_stored.clone());
    builder.add_text_field("language", STRING | STORED);
    builder.add_text_field("name", code_text_stored.clone());
    builder.add_text_field("signature", code_text_stored.clone());
    builder.add_text_field("doc_comment", code_text_stored.clone());
    builder.add_text_field("code_body", code_text_not_stored.clone());
    builder.add_text_field("annotations_exact", STRING);
    builder.add_text_field("annotations_text", code_text_not_stored.clone());
    builder.add_text_field("owner_names_text", code_text_not_stored.clone());
    builder.add_text_field("kind", STRING | STORED);
    builder.add_u64_field(
        "start_line",
        tantivy::schema::NumericOptions::default().set_stored(),
    );
    builder.add_text_field("role", STRING | STORED);
    builder.add_text_field("test_role", STRING | STORED);
    builder.add_text_field("content", code_text_not_stored.clone());

    let old_schema = builder.build();
    let new_schema = create_schema();

    let old_sig = compatibility_signature(&old_schema);
    let new_sig = compatibility_signature(&new_schema);

    assert_ne!(
        old_sig, new_sig,
        "compatibility_signature must differ when pretokenized_code / relationship_text are added"
    );
}
