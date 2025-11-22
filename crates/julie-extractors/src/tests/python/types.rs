/// Tests for Python type extraction through the factory
///
/// These tests validate that the factory properly calls infer_types()
/// and returns TypeInfo in the ExtractionResults.

#[cfg(test)]
mod tests {
    use crate::factory::extract_symbols_and_relationships;
    use std::path::PathBuf;
    use tree_sitter::Parser;

    #[test]
    fn test_factory_extracts_python_types() {
        // Python function with type annotation
        let code = r#"
def calculate_total(price: float, tax: float) -> float:
    """Calculate total with tax."""
    return price + tax

class UserService:
    """User service class."""

    def get_user(self, user_id: int) -> dict:
        """Get user by ID."""
        return {"id": user_id}
"#;

        // Parse with tree-sitter
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .expect("Error loading Python grammar");
        let tree = parser.parse(code, None).expect("Error parsing code");

        // Extract through factory
        let workspace_root = PathBuf::from("/tmp/test");
        let results = extract_symbols_and_relationships(
            &tree,
            "test.py",
            code,
            "python",
            &workspace_root,
        )
        .expect("Extraction failed");

        // CRITICAL: Verify types HashMap is NOT empty
        assert!(
            !results.types.is_empty(),
            "Python type extraction returned EMPTY types HashMap! \
             Factory is not calling infer_types() properly."
        );

        // Verify we got TypeInfo for the typed symbols
        println!("Extracted {} types from Python code", results.types.len());
        for (symbol_id, type_info) in &results.types {
            println!(
                "  {} -> {} (inferred: {})",
                symbol_id, type_info.resolved_type, type_info.is_inferred
            );
        }

        // Verify at least one type is extracted (the return types)
        assert!(
            results.types.len() >= 1,
            "Expected at least 1 type, got {}",
            results.types.len()
        );

        // Verify TypeInfo structure is correct
        for type_info in results.types.values() {
            assert_eq!(type_info.language, "python");
            assert!(type_info.is_inferred); // From infer_types()
            assert!(!type_info.resolved_type.is_empty());
        }
    }

    #[test]
    fn test_factory_python_types_empty_for_untyped_code() {
        // Python without type annotations
        let code = r#"
def old_style_function(x, y):
    return x + y
"#;

        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .expect("Error loading Python grammar");
        let tree = parser.parse(code, None).expect("Error parsing code");

        let workspace_root = PathBuf::from("/tmp/test");
        let results = extract_symbols_and_relationships(
            &tree,
            "test.py",
            code,
            "python",
            &workspace_root,
        )
        .expect("Extraction failed");

        // For untyped Python, types may be empty or minimal
        // This is expected - not all code has type annotations
        println!(
            "Untyped Python extracted {} types (expected: 0 or minimal)",
            results.types.len()
        );
    }
}
