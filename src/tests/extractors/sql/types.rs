/// Tests for SQL type extraction through the factory

#[cfg(test)]
mod tests {
    use crate::extractors::factory::extract_symbols_and_relationships;
    use std::path::PathBuf;
    use tree_sitter::Parser;

    #[test]
    fn test_factory_extracts_sql_types() {
        let code = r#"
CREATE TABLE users (
    id INT PRIMARY KEY,
    name VARCHAR(100) NOT NULL,
    email VARCHAR(255) UNIQUE,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE FUNCTION get_user_count() RETURNS INTEGER AS $$
BEGIN
    RETURN (SELECT COUNT(*) FROM users);
END;
$$ LANGUAGE plpgsql;
"#;

        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_sequel::LANGUAGE.into())
            .expect("Error loading SQL grammar");
        let tree = parser.parse(code, None).expect("Error parsing code");

        let workspace_root = PathBuf::from("/tmp/test");
        let results = extract_symbols_and_relationships(
            &tree,
            "test.sql",
            code,
            "sql",
            &workspace_root,
        )
        .expect("Extraction failed");

        assert!(
            !results.types.is_empty(),
            "SQL type extraction returned EMPTY types HashMap!"
        );

        println!("Extracted {} types from SQL code", results.types.len());
        for (symbol_id, type_info) in &results.types {
            println!("  {} -> {} (inferred: {})", symbol_id, type_info.resolved_type, type_info.is_inferred);
        }

        assert!(results.types.len() >= 1);
        for type_info in results.types.values() {
            assert_eq!(type_info.language, "sql");
            assert!(type_info.is_inferred);
        }
    }
}
