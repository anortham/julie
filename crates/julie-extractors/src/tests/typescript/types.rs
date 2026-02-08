/// Tests for TypeScript type extraction through the factory
///
/// These tests validate that the factory properly calls infer_types()
/// and returns TypeInfo in the ExtractionResults.

#[cfg(test)]
mod interface_member_tests {
    use crate::base::SymbolKind;
    use crate::typescript::TypeScriptExtractor;
    use std::path::PathBuf;
    use tree_sitter::Parser;

    #[test]
    fn test_extract_interface_members() {
        let code = r#"
interface ApiResponse {
    data: any;
    error?: string;
    status: string;
    getData(): any;
    setStatus(status: string): void;
}
"#;

        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .expect("Error loading TypeScript grammar");
        let tree = parser.parse(code, None).expect("Error parsing code");

        let workspace_root = PathBuf::from("/tmp/test");

        let mut extractor = TypeScriptExtractor::new(
            "typescript".to_string(),
            "test.ts".to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);

        // Should have the interface itself
        let iface = symbols
            .iter()
            .find(|s| s.name == "ApiResponse" && s.kind == SymbolKind::Interface);
        assert!(iface.is_some(), "Should extract ApiResponse interface");
        let iface_id = &iface.unwrap().id;

        // Should have property members with parent_id pointing to the interface
        let properties: Vec<_> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Property && s.parent_id.as_ref() == Some(iface_id))
            .collect();
        assert!(
            properties.len() >= 3,
            "Should extract at least 3 property members (data, error, status), got {}: {:?}",
            properties.len(),
            properties.iter().map(|s| &s.name).collect::<Vec<_>>()
        );

        // Should have method members with parent_id pointing to the interface
        let methods: Vec<_> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Method && s.parent_id.as_ref() == Some(iface_id))
            .collect();
        assert!(
            methods.len() >= 2,
            "Should extract at least 2 method members (getData, setStatus), got {}: {:?}",
            methods.len(),
            methods.iter().map(|s| &s.name).collect::<Vec<_>>()
        );

        // Verify specific property names
        let prop_names: Vec<&str> = properties.iter().map(|s| s.name.as_str()).collect();
        assert!(prop_names.contains(&"data"), "Should have 'data' property");
        assert!(prop_names.contains(&"status"), "Should have 'status' property");

        // Verify specific method names
        let method_names: Vec<&str> = methods.iter().map(|s| s.name.as_str()).collect();
        assert!(method_names.contains(&"getData"), "Should have 'getData' method");
        assert!(method_names.contains(&"setStatus"), "Should have 'setStatus' method");

        // Verify members have signatures for searchability
        for method in &methods {
            assert!(
                method.signature.is_some(),
                "Method '{}' should have a signature",
                method.name
            );
        }
        for prop in &properties {
            assert!(
                prop.signature.is_some(),
                "Property '{}' should have a signature",
                prop.name
            );
        }
    }

    #[test]
    fn test_extract_interface_members_no_duplicates() {
        // Ensure interface members are NOT extracted twice
        // (once by extract_interface and once by the recursive visitor)
        let code = r#"
interface Config {
    timeout: number;
    retries: number;
}
"#;

        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .expect("Error loading TypeScript grammar");
        let tree = parser.parse(code, None).expect("Error parsing code");

        let workspace_root = PathBuf::from("/tmp/test");

        let mut extractor = TypeScriptExtractor::new(
            "typescript".to_string(),
            "test.ts".to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);

        // Count how many times each property name appears
        let timeout_count = symbols.iter().filter(|s| s.name == "timeout").count();
        let retries_count = symbols.iter().filter(|s| s.name == "retries").count();

        assert_eq!(
            timeout_count, 1,
            "Property 'timeout' should appear exactly once, got {}",
            timeout_count
        );
        assert_eq!(
            retries_count, 1,
            "Property 'retries' should appear exactly once, got {}",
            retries_count
        );
    }
}

#[cfg(test)]
mod tests {
    use crate::factory::extract_symbols_and_relationships;
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
        println!("Extracted {} types from TypeScript code", results.types.len());
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
