/// Tests for Go type extraction through the factory

#[cfg(test)]
mod tests {
    use crate::base::SymbolKind;
    use crate::factory::extract_symbols_and_relationships;
    use crate::go::GoExtractor;
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

    #[test]
    fn test_go_structs_are_struct_symbols_and_packages_are_modules() {
        let code = r#"
package main

type User struct {
    ID int64
}
"#;

        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_go::LANGUAGE.into())
            .expect("Error loading Go grammar");
        let tree = parser.parse(code, None).expect("Error parsing Go source");

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = GoExtractor::new(
            "go".to_string(),
            "types.go".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);

        let package = symbols
            .iter()
            .find(|symbol| symbol.name == "main")
            .expect("package symbol should be extracted");
        assert_eq!(package.kind, SymbolKind::Module);
        assert!(package.parent_id.is_none());

        let user = symbols
            .iter()
            .find(|symbol| symbol.name == "User")
            .expect("struct symbol should be extracted");
        assert_eq!(user.kind, SymbolKind::Struct);
        assert!(
            user.signature
                .as_deref()
                .is_some_and(|signature| signature.contains("type User struct"))
        );
    }

    #[test]
    fn test_go_embedded_field_emits_relationship_or_embedding_metadata() {
        let code = r#"
package main

type Base struct{}

type Embedded struct {
    Base
}
"#;

        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_go::LANGUAGE.into())
            .expect("Error loading Go grammar");
        let tree = parser.parse(code, None).expect("Error parsing Go source");

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = GoExtractor::new(
            "go".to_string(),
            "embedded.go".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);

        let embedded_struct = symbols
            .iter()
            .find(|symbol| symbol.name == "Embedded")
            .expect("Embedded struct should be extracted");

        let embedded_field = symbols
            .iter()
            .find(|symbol| {
                symbol.kind == SymbolKind::Field
                    && symbol.name == "Base"
                    && symbol.parent_id.as_deref() == Some(embedded_struct.id.as_str())
            })
            .expect("embedded anonymous field should be extracted as a field symbol");

        let metadata = embedded_field
            .metadata
            .as_ref()
            .expect("embedded field should include embedding metadata");
        assert_eq!(
            metadata.get("go_embedded"),
            Some(&serde_json::Value::Bool(true))
        );
        assert_eq!(
            metadata.get("embedded_type"),
            Some(&serde_json::Value::String("Base".to_string()))
        );
    }
}
