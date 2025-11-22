/// Tests for HTML type extraction through the factory

#[cfg(test)]
mod tests {
    use crate::factory::extract_symbols_and_relationships;
    use std::path::PathBuf;
    use tree_sitter::Parser;

    #[test]
    fn test_factory_extracts_html_types() {
        let code = r#"
<!DOCTYPE html>
<html>
<head>
    <title>Test Page</title>
</head>
<body>
    <input type="text" name="username" id="user-input" />
    <input type="email" name="email" />
    <button type="submit" onclick="handleSubmit()">Submit</button>
</body>
</html>
"#;

        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_html::LANGUAGE.into())
            .expect("Error loading HTML grammar");
        let tree = parser.parse(code, None).expect("Error parsing code");

        let workspace_root = PathBuf::from("/tmp/test");
        let results = extract_symbols_and_relationships(
            &tree,
            "test.html",
            code,
            "html",
            &workspace_root,
        )
        .expect("Extraction failed");

        assert!(
            !results.types.is_empty(),
            "HTML type extraction returned EMPTY types HashMap!"
        );

        println!("Extracted {} types from HTML code", results.types.len());
        for (symbol_id, type_info) in &results.types {
            println!("  {} -> {} (inferred: {})", symbol_id, type_info.resolved_type, type_info.is_inferred);
        }

        assert!(results.types.len() >= 1);
        for type_info in results.types.values() {
            assert_eq!(type_info.language, "html");
            assert!(type_info.is_inferred);
        }
    }
}
