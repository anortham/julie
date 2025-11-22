/// Tests for PHP type extraction through the factory
///
/// These tests validate that the factory properly calls infer_types()
/// and returns TypeInfo in the ExtractionResults.

#[cfg(test)]
mod tests {
    use crate::factory::extract_symbols_and_relationships;
    use std::path::PathBuf;
    use tree_sitter::Parser;

    #[test]
    fn test_factory_extracts_php_types() {
        // PHP code with type declarations
        let code = r#"
<?php
class UserService {
    public function getUserName(int $userId): string {
        return "User" . $userId;
    }

    public function getAllUsers(): array {
        return $this->repository->findAll();
    }

    public function getUserById(int $id): ?User {
        return $this->repository->find($id);
    }
}
"#;

        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_php::LANGUAGE_PHP.into())
            .expect("Error loading PHP grammar");
        let tree = parser.parse(code, None).expect("Error parsing code");

        let workspace_root = PathBuf::from("/tmp/test");
        let results = extract_symbols_and_relationships(
            &tree,
            "test.php",
            code,
            "php",
            &workspace_root,
        )
        .expect("Extraction failed");

        // CRITICAL: Verify types HashMap is NOT empty
        assert!(
            !results.types.is_empty(),
            "PHP type extraction returned EMPTY types HashMap! \
             Factory is not calling infer_types() properly."
        );

        println!("Extracted {} types from PHP code", results.types.len());
        for (symbol_id, type_info) in &results.types {
            println!(
                "  {} -> {} (inferred: {})",
                symbol_id, type_info.resolved_type, type_info.is_inferred
            );
        }

        assert!(
            results.types.len() >= 1,
            "Expected at least 1 type, got {}",
            results.types.len()
        );

        for type_info in results.types.values() {
            assert_eq!(type_info.language, "php");
            assert!(type_info.is_inferred);
            assert!(!type_info.resolved_type.is_empty());
        }
    }
}
