/// Tests for Go type extraction through the factory

#[cfg(test)]
mod tests {
    use crate::factory::extract_symbols_and_relationships;
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
        let results =
            extract_symbols_and_relationships(&tree, "test.go", code, "go", &workspace_root)
                .expect("Extraction failed");

        let type_map: std::collections::HashMap<_, _> = results
            .types
            .values()
            .filter_map(|type_info| {
                results
                    .symbols
                    .iter()
                    .find(|symbol| symbol.id == type_info.symbol_id)
                    .map(|symbol| (symbol.name.as_str(), type_info.resolved_type.as_str()))
            })
            .collect();

        assert_eq!(type_map.len(), 2);
        assert_eq!(type_map.get("GetUserName"), Some(&"string"));
        assert_eq!(type_map.get("GetUserScores"), Some(&"map[string]int"));
        assert!(!type_map.contains_key("GetAllUsers"));
    }
}
