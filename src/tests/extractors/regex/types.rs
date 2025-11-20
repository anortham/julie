/// Tests for Regex type extraction through the factory

#[cfg(test)]
mod tests {
    use crate::extractors::factory::extract_symbols_and_relationships;
    use std::path::PathBuf;
    use tree_sitter::Parser;

    #[test]
    fn test_factory_extracts_regex_types() {
        let code = r#"
/^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$/
/^\d{3}-\d{2}-\d{4}$/
/^(https?:\/\/)?([\da-z\.-]+)\.([a-z\.]{2,6})([\/\w \.-]*)*\/?$/
"#;

        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_regex::LANGUAGE.into())
            .expect("Error loading Regex grammar");
        let tree = parser.parse(code, None).expect("Error parsing code");

        let workspace_root = PathBuf::from("/tmp/test");
        let results =
            extract_symbols_and_relationships(&tree, "test.regex", code, "regex", &workspace_root)
                .expect("Extraction failed");

        assert!(
            !results.types.is_empty(),
            "Regex type extraction returned EMPTY types HashMap!"
        );

        println!("Extracted {} types from Regex code", results.types.len());
        for (symbol_id, type_info) in &results.types {
            println!(
                "  {} -> {} (inferred: {})",
                symbol_id, type_info.resolved_type, type_info.is_inferred
            );
        }

        assert!(results.types.len() >= 1);
        for type_info in results.types.values() {
            assert_eq!(type_info.language, "regex");
            assert!(type_info.is_inferred);
        }
    }
}
