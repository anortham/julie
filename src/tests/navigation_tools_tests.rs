//! Token optimization tests for navigation tools
//! Following TDD methodology: RED -> GREEN -> REFACTOR

#[cfg(test)]
mod navigation_tools_tests {
    use crate::extractors::base::{Relationship, RelationshipKind, Symbol, SymbolKind, Visibility};
    use crate::tools::navigation::{FastGotoTool, FastRefsTool};
    use std::collections::HashMap;

    #[test]
    fn test_fast_refs_short_response_unchanged() {
        let refs_tool = FastRefsTool {
            symbol: "test".to_string(),
            include_definition: true,
            limit: 10,
            workspace: Some("primary".to_string()),
        };

        // Create a small number of symbols and relationships
        let mut symbols = Vec::new();
        let mut relationships = Vec::new();

        // Create 3 symbols with references
        for i in 1..=3 {
            let symbol = Symbol {
                id: i.to_string(),
                name: format!("function_{}", i),
                kind: SymbolKind::Function,
                language: "rust".to_string(),
                file_path: format!("file_{}.rs", i),
                start_line: 1,
                start_column: 0,
                end_line: 5,
                end_column: 0,
                start_byte: 0,
                end_byte: 50,
                signature: Some(format!("fn function_{}()", i)),
                doc_comment: None,
                visibility: Some(Visibility::Public),
                parent_id: None,
                metadata: Some(HashMap::new()),
                semantic_group: None,
                confidence: Some(0.9),
                code_context: None, // FastRefsTool doesn't use code context currently
            };
            symbols.push(symbol);

            // Create a reference relationship
            let relationship = Relationship {
                id: format!("ref_{}", i),
                from_symbol_id: i.to_string(),
                to_symbol_id: "test".to_string(),
                kind: RelationshipKind::References,
                file_path: format!("file_{}.rs", i),
                line_number: 10 + i,
                confidence: 0.9,
                metadata: Some(HashMap::new()),
            };
            relationships.push(relationship);
        }

        // Format the response (this will need to be implemented)
        let result = refs_tool.format_optimized_results(&symbols, &relationships);

        // Should contain all 3 results without optimization for small responses
        assert!(result.contains("function_1"));
        assert!(result.contains("function_2"));
        assert!(result.contains("function_3"));
        assert!(result.contains("file_1.rs"));
        assert!(result.contains("file_2.rs"));
        assert!(result.contains("file_3.rs"));

        // Should NOT contain progressive reduction messages for small responses
        assert!(!result.contains("Applied progressive reduction"));
        assert!(!result.contains("Response truncated to stay within token limits"));
    }

    #[test]
    fn test_fast_refs_large_response_needs_reduction() {
        let refs_tool = FastRefsTool {
            symbol: "popular_function".to_string(),
            include_definition: true,
            limit: 200, // Large limit to test token optimization
            workspace: Some("primary".to_string()),
        };

        // Create many symbols and relationships to trigger token optimization
        let mut symbols = Vec::new();
        let mut relationships = Vec::new();

        // Create 400 references with very long paths and names to trigger progressive reduction
        for i in 1..=400 {
            let symbol = Symbol {
                id: i.to_string(),
                name: format!("very_long_caller_function_with_detailed_name_describing_complex_functionality_{}", i),
                kind: SymbolKind::Function,
                language: "rust".to_string(),
                file_path: format!("src/extremely/deeply/nested/module/structure/with/many/levels/of/organization/submodule_{}/detailed_implementation_file_{}.rs", i % 10, i),
                start_line: i * 10,
                start_column: 0,
                end_line: i * 10 + 5,
                end_column: 0,
                start_byte: 0,
                end_byte: 100,
                signature: Some(format!("fn very_long_caller_function_with_detailed_name_describing_complex_functionality_{}() -> Result<ComplexReturnType, DetailedError>", i)),
                doc_comment: None,
                visibility: Some(Visibility::Public),
                parent_id: None,
                metadata: Some(HashMap::new()),
                semantic_group: None,
                confidence: Some(0.9),
                code_context: None,
            };
            symbols.push(symbol);

            // Create reference relationship
            let relationship = Relationship {
                id: format!("ref_{}", i),
                from_symbol_id: i.to_string(),
                to_symbol_id: "popular_function".to_string(),
                kind: RelationshipKind::References,
                file_path: format!("src/extremely/deeply/nested/module/structure/with/many/levels/of/organization/submodule_{}/detailed_implementation_file_{}.rs", i % 10, i),
                line_number: i * 10 + 3,
                confidence: 0.9,
                metadata: Some(HashMap::new()),
            };
            relationships.push(relationship);
        }

        let result = refs_tool.format_optimized_results(&symbols, &relationships);

        // Should contain progressive reduction notice for large responses
        assert!(
            result.contains("Applied progressive reduction")
                || result.contains("Response truncated to stay within token limits")
        );

        // Should contain early references but not all 400
        assert!(result.contains(
            "very_long_caller_function_with_detailed_name_describing_complex_functionality_1"
        ));
        assert!(!result.contains(
            "very_long_caller_function_with_detailed_name_describing_complex_functionality_400"
        )); // Last function should be excluded

        // Should show accurate count (not showing all 400)
        assert!(!result.contains("Showing 400 of 400"));
    }

    #[test]
    fn test_fast_refs_respects_token_limits() {
        let refs_tool = FastRefsTool {
            symbol: "test_symbol".to_string(),
            include_definition: true,
            limit: 50,
            workspace: Some("primary".to_string()),
        };

        // Create 300 references with extremely long file paths and names to trigger token limits
        let mut symbols = Vec::new();
        let mut relationships = Vec::new();

        for i in 1..=300 {
            let symbol = Symbol {
                id: i.to_string(),
                name: format!("extremely_long_reference_symbol_with_very_detailed_descriptive_naming_convention_{}", i),
                kind: SymbolKind::Function,
                language: "rust".to_string(),
                file_path: format!("src/extremely/deeply/nested/module/structure/with/excessive/directory/nesting/levels/for/token/testing/submodule_{}/implementation_file_with_long_name_{}.rs", i % 20, i),
                start_line: 1,
                start_column: 0,
                end_line: 10,
                end_column: 0,
                start_byte: 0,
                end_byte: 200,
                signature: Some(format!("fn extremely_long_reference_symbol_with_very_detailed_descriptive_naming_convention_{}() -> Result<ComplexReturnTypeWithManyGenerics, VeryDescriptiveErrorType>", i)),
                doc_comment: None,
                visibility: Some(Visibility::Public),
                parent_id: None,
                metadata: Some(HashMap::new()),
                semantic_group: None,
                confidence: Some(0.9),
                code_context: None,
            };
            symbols.push(symbol);

            let relationship = Relationship {
                id: format!("ref_{}", i),
                from_symbol_id: i.to_string(),
                to_symbol_id: "test_symbol".to_string(),
                kind: RelationshipKind::References,
                file_path: format!("src/extremely/deeply/nested/module/structure/with/excessive/directory/nesting/levels/for/token/testing/submodule_{}/implementation_file_with_long_name_{}.rs", i % 20, i),
                line_number: 15,
                confidence: 0.9,
                metadata: Some(HashMap::new()),
            };
            relationships.push(relationship);
        }

        let result = refs_tool.format_optimized_results(&symbols, &relationships);

        // Should apply token optimization - either progressive reduction or early termination
        let has_progressive_reduction = result.contains("Applied progressive reduction");
        let has_early_termination =
            result.contains("Response truncated to stay within token limits");
        assert!(has_progressive_reduction || has_early_termination);

        // Should include first references but may exclude later ones due to token limits
        assert!(result.contains(
            "extremely_long_reference_symbol_with_very_detailed_descriptive_naming_convention_1"
        ));
        // Depending on token optimization, may or may not contain the last symbol
    }

    // FastGotoTool token optimization tests
    #[test]
    fn test_fast_goto_short_response_unchanged() {
        let goto_tool = FastGotoTool {
            symbol: "UserService".to_string(),
            context_file: None,
            line_number: None,
            workspace: Some("primary".to_string()),
        };

        // Create a small number of definitions for the symbol
        let mut symbols = Vec::new();

        // Create 3 definitions in different files
        for i in 1..=3 {
            let symbol = Symbol {
                id: i.to_string(),
                name: "UserService".to_string(),
                kind: SymbolKind::Class,
                language: "rust".to_string(),
                file_path: format!("src/services/module_{}/user_service.rs", i),
                start_line: 10,
                start_column: 0,
                end_line: 50,
                end_column: 0,
                start_byte: 0,
                end_byte: 500,
                signature: Some(format!(
                    "pub struct UserService {{ // Module {} implementation }}",
                    i
                )),
                doc_comment: Some(format!("User service for module {}", i)),
                visibility: Some(Visibility::Public),
                parent_id: None,
                metadata: Some(HashMap::new()),
                semantic_group: None,
                confidence: Some(0.95),
                code_context: None,
            };
            symbols.push(symbol);
        }

        // Format the response (this will need to be implemented)
        let result = goto_tool.format_optimized_results(&symbols);

        // Should contain all 3 definitions without optimization for small responses
        assert!(result.contains("🎯 Go to Definition: UserService"));
        assert!(result.contains("📊 Showing 3 of 3 definitions"));
        assert!(result.contains("user_service.rs"));
        assert!(result.contains("pub struct UserService"));

        // Should NOT contain progressive reduction messages for small responses
        assert!(!result.contains("Applied progressive reduction"));
        assert!(!result.contains("Response truncated to stay within token limits"));
    }

    #[test]
    fn test_fast_goto_large_codebase_needs_reduction() {
        let goto_tool = FastGotoTool {
            symbol: "CommonUtility".to_string(),
            context_file: None,
            line_number: None,
            workspace: Some("primary".to_string()),
        };

        // Create many definitions across a large codebase to trigger progressive reduction
        let mut symbols = Vec::new();

        // Create large context content to trigger token optimization
        let large_context = (1..=30).map(|line| format!("    // Comprehensive documentation line {} with substantial content describing complex functionality, implementation details, usage patterns, error handling strategies, performance considerations, and extensive examples for progressive reduction testing", line)).collect::<Vec<_>>().join("\n");

        // Create 200 definitions with substantial content to trigger token optimization
        for i in 1..=200 {
            let symbol = Symbol {
                id: i.to_string(),
                name: "CommonUtility".to_string(),
                kind: SymbolKind::Class,
                language: "rust".to_string(),
                file_path: format!("src/extremely/deeply/nested/module/structure/with/extensive/organization/patterns/module_{}/utilities/common_utility_implementation_with_detailed_naming.rs", i),
                start_line: i * 10,
                start_column: 0,
                end_line: i * 10 + 100,
                end_column: 0,
                start_byte: 0,
                end_byte: 5000,
                signature: Some(format!("pub struct CommonUtility {{ // Comprehensive implementation for module {} with extensive functionality and detailed error handling }} impl CommonUtility {{ pub fn new() -> Self {{ Self {{ /* detailed initialization */ }} }}", i)),
                doc_comment: Some(format!("/// Comprehensive utility class for module {} providing extensive functionality including data processing, validation, transformation, and error handling with detailed documentation and usage examples", i)),
                visibility: Some(Visibility::Public),
                parent_id: None,
                metadata: Some(HashMap::new()),
                semantic_group: Some(format!("utility_group_{}", i % 20)),
                confidence: Some(0.98),
                code_context: Some(large_context.clone()),
            };
            symbols.push(symbol);
        }

        let result = goto_tool.format_optimized_results(&symbols);

        // Should contain progressive reduction notice for large responses
        assert!(
            result.contains("Applied progressive reduction")
                || result.contains("Response truncated to stay within token limits")
        );

        // Should contain early definitions but not all 200
        assert!(result.contains("CommonUtility"));
        assert!(result.contains("module_1"));
        assert!(!result.contains("module_200")); // Last definition should be excluded

        // Should show accurate count (not showing all 200)
        assert!(!result.contains("Showing 200 of 200"));
    }

    #[test]
    fn test_fast_goto_respects_token_limits_with_context() {
        let goto_tool = FastGotoTool {
            symbol: "ProcessorInterface".to_string(),
            context_file: Some("src/main.rs".to_string()),
            line_number: Some(42),
            workspace: Some("primary".to_string()),
        };

        // Create 100 definitions with extensive signatures and documentation
        let mut symbols = Vec::new();

        for i in 1..=100 {
            let symbol = Symbol {
                id: i.to_string(),
                name: "ProcessorInterface".to_string(),
                kind: SymbolKind::Interface,
                language: "rust".to_string(),
                file_path: format!("src/complex/architecture/with/extensive/module/hierarchies/and/detailed/organization/patterns/processor_module_{}/interface_definitions_with_comprehensive_specifications.rs", i),
                start_line: 1,
                start_column: 0,
                end_line: 200,
                end_column: 0,
                start_byte: 0,
                end_byte: 10000,
                signature: Some(format!("pub trait ProcessorInterface {{ fn process_data(&self, input: ComplexDataStructureWithManyGenerics<T, U, V>) -> Result<ProcessedOutputWithDetailedMetadata, ComprehensiveErrorInformationWithContext>; fn validate_input(&self, data: &InputValidationStructure) -> ValidationResult; fn handle_errors(&self, error: ProcessingError) -> ErrorRecoveryStrategy; }} // Module {} implementation", i)),
                doc_comment: Some(format!("/// Comprehensive processor interface for module {} providing extensive data processing capabilities with detailed error handling, validation, transformation, and metadata management functionality for complex enterprise applications", i)),
                visibility: Some(Visibility::Public),
                parent_id: None,
                metadata: Some(HashMap::new()),
                semantic_group: Some(format!("processor_interfaces_{}", i % 10)),
                confidence: Some(0.99),
                code_context: None,
            };
            symbols.push(symbol);
        }

        let result = goto_tool.format_optimized_results(&symbols);

        // Should apply token optimization for large content
        let has_progressive_reduction = result.contains("Applied progressive reduction");
        let has_early_termination =
            result.contains("Response truncated to stay within token limits");
        assert!(has_progressive_reduction || has_early_termination);

        // Should contain some definitions but with context information
        assert!(result.contains("ProcessorInterface"));
        assert!(result.contains("Context: src/main.rs:42"));

        // Should include first definitions but may exclude later ones due to token limits
        assert!(result.contains("processor_module_1"));
    }
}
