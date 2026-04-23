// Inline tests extracted from src/extractors/rust/functions.rs
//
// This module contains tests for Rust function and method extraction:
// - Function extraction and parameter parsing
// - Method extraction from impl blocks
// - Impl block processing and linking to parent types
// - Function signature building

#[cfg(test)]
mod tests {
    use crate::base::SymbolKind;
    use crate::rust::RustExtractor;
    use std::path::PathBuf;

    #[test]
    fn test_function_extraction() {
        // Function extraction is tested through integration tests
    }

    #[test]
    fn test_function_parameters() {
        // Function parameter extraction is tested through integration tests
    }

    #[test]
    fn test_impl_block_processing() {
        // Impl block processing is tested through integration tests
    }

    #[test]
    fn rust_test_attribute_markers_persist_and_drive_test_detection() {
        let code = r#"
#[test]
fn plain_test() {}

#[tokio::test]
async fn async_test() {}
"#;

        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .expect("failed to load Rust grammar");
        let tree = parser.parse(code, None).expect("failed to parse Rust");

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = RustExtractor::new(
            "rust".to_string(),
            "lib.rs".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);

        let plain_test = symbols
            .iter()
            .find(|s| s.name == "plain_test" && s.kind == SymbolKind::Function)
            .expect("plain_test should be extracted");
        assert_eq!(plain_test.annotations.len(), 1);
        assert_eq!(plain_test.annotations[0].annotation, "test");
        assert_eq!(plain_test.annotations[0].annotation_key, "test");
        assert_eq!(plain_test.annotations[0].raw_text.as_deref(), Some("test"));
        assert_eq!(plain_test.annotations[0].carrier, None);
        assert_eq!(
            plain_test
                .metadata
                .as_ref()
                .and_then(|m| m.get("is_test"))
                .and_then(|v| v.as_bool()),
            Some(true)
        );

        let async_test = symbols
            .iter()
            .find(|s| s.name == "async_test" && s.kind == SymbolKind::Function)
            .expect("async_test should be extracted");
        assert_eq!(async_test.annotations.len(), 1);
        assert_eq!(async_test.annotations[0].annotation, "tokio::test");
        assert_eq!(async_test.annotations[0].annotation_key, "tokio::test");
        assert_eq!(
            async_test.annotations[0].raw_text.as_deref(),
            Some("tokio::test")
        );
        assert_eq!(
            async_test
                .metadata
                .as_ref()
                .and_then(|m| m.get("is_test"))
                .and_then(|v| v.as_bool()),
            Some(true)
        );
    }
}
