/// Tests for Go type extraction through the factory

#[cfg(test)]
mod tests {
    use crate::extractors::factory::extract_symbols_and_relationships;
    use std::path::PathBuf;
    use tree_sitter::Parser;

    #[test]
    fn test_factory_extracts_go_types() {
        let code = r#"
package main

func GetUserName(userId int) string {
    return fmt.Sprintf("User%d", userId)
}

func GetAllUsers() ([]User, error) {
    return repository.FindAll()
}

func GetUserScores() map[string]int {
    return make(map[string]int)
}
"#;

        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_go::LANGUAGE.into())
            .expect("Error loading Go grammar");
        let tree = parser.parse(code, None).expect("Error parsing code");

        let workspace_root = PathBuf::from("/tmp/test");
        let results = extract_symbols_and_relationships(
            &tree,
            "test.go",
            code,
            "go",
            &workspace_root,
        )
        .expect("Extraction failed");

        assert!(
            !results.types.is_empty(),
            "Go type extraction returned EMPTY types HashMap!"
        );

        println!("Extracted {} types from Go code", results.types.len());
        for (symbol_id, type_info) in &results.types {
            println!("  {} -> {} (inferred: {})", symbol_id, type_info.resolved_type, type_info.is_inferred);
        }

        assert!(results.types.len() >= 1);
        for type_info in results.types.values() {
            assert_eq!(type_info.language, "go");
            assert!(type_info.is_inferred);
        }
    }
}
