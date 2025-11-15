/// Tests for Java type extraction through the factory
///
/// These tests validate that the factory properly calls infer_types()
/// and returns TypeInfo in the ExtractionResults.

#[cfg(test)]
mod tests {
    use crate::extractors::factory::extract_symbols_and_relationships;
    use std::path::PathBuf;
    use tree_sitter::Parser;

    #[test]
    fn test_factory_extracts_java_types() {
        // Java code with type declarations
        let code = r#"
public class Calculator {
    public int add(int a, int b) {
        return a + b;
    }

    public String getUserName(int userId) {
        return "User" + userId;
    }

    public List<User> getAllUsers() {
        return new ArrayList<>();
    }
}
"#;

        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_java::LANGUAGE.into())
            .expect("Error loading Java grammar");
        let tree = parser.parse(code, None).expect("Error parsing code");

        let workspace_root = PathBuf::from("/tmp/test");
        let results = extract_symbols_and_relationships(
            &tree,
            "test.java",
            code,
            "java",
            &workspace_root,
        )
        .expect("Extraction failed");

        // CRITICAL: Verify types HashMap is NOT empty
        assert!(
            !results.types.is_empty(),
            "Java type extraction returned EMPTY types HashMap! \
             Factory is not calling infer_types() properly."
        );

        println!("Extracted {} types from Java code", results.types.len());
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
            assert_eq!(type_info.language, "java");
            assert!(type_info.is_inferred);
            assert!(!type_info.resolved_type.is_empty());
        }
    }
}
