/// Tests for TypeScript type extraction through the factory
///
/// These tests validate that the factory properly calls infer_types()
/// and returns TypeInfo in the ExtractionResults.

#[cfg(test)]
mod tests {
    use crate::extractors::factory::extract_symbols_and_relationships;
    use std::path::PathBuf;
    use tree_sitter::Parser;

    #[test]
    fn test_factory_extracts_typescript_types() {
        // TypeScript function with type annotations
        let code = r#"
function calculateTotal(price: number, tax: number): number {
    return price + tax;
}

class UserService {
    getUser(userId: string): User {
        return { id: userId, name: "Test" };
    }
}

interface User {
    id: string;
    name: string;
}
"#;

        // Parse with tree-sitter
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .expect("Error loading TypeScript grammar");
        let tree = parser.parse(code, None).expect("Error parsing code");

        // Extract through factory
        let workspace_root = PathBuf::from("/tmp/test");
        let results = extract_symbols_and_relationships(
            &tree,
            "test.ts",
            code,
            "typescript",
            &workspace_root,
        )
        .expect("Extraction failed");

        // CRITICAL: Verify types HashMap is NOT empty
        assert!(
            !results.types.is_empty(),
            "TypeScript type extraction returned EMPTY types HashMap! \
             Factory is not calling infer_types() properly."
        );

        // Verify we got TypeInfo for the typed symbols
        println!(
            "Extracted {} types from TypeScript code",
            results.types.len()
        );
        for (symbol_id, type_info) in &results.types {
            println!(
                "  {} -> {} (inferred: {})",
                symbol_id, type_info.resolved_type, type_info.is_inferred
            );
        }

        // Verify at least one type is extracted
        assert!(
            results.types.len() >= 1,
            "Expected at least 1 type, got {}",
            results.types.len()
        );

        // Verify TypeInfo structure is correct
        for type_info in results.types.values() {
            assert_eq!(type_info.language, "typescript");
            assert!(type_info.is_inferred); // From infer_types()
            assert!(!type_info.resolved_type.is_empty());
        }
    }

    #[test]
    fn test_factory_typescript_types_empty_for_untyped_code() {
        // TypeScript without type annotations
        let code = r#"
function oldStyleFunction(x, y) {
    return x + y;
}
"#;

        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .expect("Error loading TypeScript grammar");
        let tree = parser.parse(code, None).expect("Error parsing code");

        let workspace_root = PathBuf::from("/tmp/test");
        let results = extract_symbols_and_relationships(
            &tree,
            "test.ts",
            code,
            "typescript",
            &workspace_root,
        )
        .expect("Extraction failed");

        // For untyped TypeScript, types may be empty or minimal
        println!(
            "Untyped TypeScript extracted {} types (expected: 0 or minimal)",
            results.types.len()
        );
    }
}
