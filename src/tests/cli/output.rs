// Inline tests extracted from cli/output.rs
//
// Tests for output formatting and writing functionality:
// - JSON output formatting
// - NDJSON streaming output
// - Symbol buffering and batching

use crate::cli::output::{OutputFormat, OutputWriter};
use crate::extractors::base::{Symbol, SymbolKind};

#[test]
fn test_json_output() {
    let symbols = vec![Symbol {
        id: "test1".to_string(),
        name: "TestSymbol".to_string(),
        kind: SymbolKind::Function,
        file_path: "test.rs".to_string(),
        start_line: 1,
        start_column: 0,
        end_line: 5,
        end_column: 0,
        start_byte: 0,
        end_byte: 100,
        signature: Some("fn test()".to_string()),
        doc_comment: None,
        parent_id: None,
        visibility: None,
        language: "rust".to_string(),
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: None,
        content_type: None,
    }];

    let mut writer = OutputWriter::new(OutputFormat::Json).unwrap();
    writer.write_batch(&symbols).unwrap();
}

#[test]
fn test_ndjson_streaming() {
    let symbol = Symbol {
        id: "test1".to_string(),
        name: "TestSymbol".to_string(),
        kind: SymbolKind::Function,
        file_path: "test.rs".to_string(),
        start_line: 1,
        start_column: 0,
        end_line: 5,
        end_column: 0,
        start_byte: 0,
        end_byte: 100,
        signature: Some("fn test()".to_string()),
        doc_comment: None,
        parent_id: None,
        visibility: None,
        language: "rust".to_string(),
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: None,
        content_type: None,
    };

    let mut writer = OutputWriter::new(OutputFormat::Ndjson).unwrap();
    writer.write_symbol(&symbol).unwrap();
}
