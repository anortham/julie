/// Tests for Swift type extraction through the factory

#[cfg(test)]
mod tests {
    use crate::factory::extract_symbols_and_relationships;
    use std::path::PathBuf;
    use tree_sitter::Parser;

    #[test]
    fn test_factory_extracts_swift_types() {
        let code = r#"
class UserService {
    func getUserName(userId: Int) -> String {
        return "User\(userId)"
    }

    func getAllUsers() -> [User] {
        return repository.findAll()
    }

    func getUserScores() -> [String: Int] {
        return [:]
    }
}
"#;

        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_swift::LANGUAGE.into())
            .expect("Error loading Swift grammar");
        let tree = parser.parse(code, None).expect("Error parsing code");

        let workspace_root = PathBuf::from("/tmp/test");
        let results = extract_symbols_and_relationships(
            &tree,
            "test.swift",
            code,
            "swift",
            &workspace_root,
        )
        .expect("Extraction failed");

        assert!(
            !results.types.is_empty(),
            "Swift type extraction returned EMPTY types HashMap!"
        );

        println!("Extracted {} types from Swift code", results.types.len());
        for (symbol_id, type_info) in &results.types {
            println!("  {} -> {} (inferred: {})", symbol_id, type_info.resolved_type, type_info.is_inferred);
        }

        assert!(results.types.len() >= 1);
        for type_info in results.types.values() {
            assert_eq!(type_info.language, "swift");
            assert!(type_info.is_inferred);
        }
    }
}
