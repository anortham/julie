//! Token optimization tests for exploration tools
//! Following TDD methodology: RED -> GREEN -> REFACTOR

#[cfg(test)]
mod exploration_tools_tests {
    use crate::tools::exploration::FastExploreTool;
    use crate::extractors::base::{Symbol, SymbolKind, Visibility, Relationship, RelationshipKind};
    use std::collections::HashMap;

    #[test]
    fn test_fast_explore_short_response_unchanged() {
        let explore_tool = FastExploreTool {
            mode: "overview".to_string(),
            depth: "shallow".to_string(),
            focus: None,
        };

        // Create a small number of symbols for overview
        let mut symbols = Vec::new();
        let mut relationships = Vec::new();

        // Create 5 symbols across different types
        let symbol_types = vec![
            SymbolKind::Function,
            SymbolKind::Class,
            SymbolKind::Variable,
            SymbolKind::Module,
            SymbolKind::Interface,
        ];

        for (i, kind) in symbol_types.iter().enumerate() {
            let symbol = Symbol {
                id: (i + 1).to_string(),
                name: format!("symbol_{}", i + 1),
                kind: kind.clone(),
                language: "rust".to_string(),
                file_path: format!("src/file_{}.rs", i + 1),
                start_line: 1,
                start_column: 0,
                end_line: 5,
                end_column: 0,
                start_byte: 0,
                end_byte: 50,
                signature: Some(format!("signature_{}", i + 1)),
                doc_comment: None,
                visibility: Some(Visibility::Public),
                parent_id: None,
                metadata: Some(HashMap::new()),
                semantic_group: None,
                confidence: Some(0.9),
                code_context: None,
            };
            symbols.push(symbol);

            // Create a relationship
            if i > 0 {
                let relationship = Relationship {
                    id: format!("rel_{}", i),
                    from_symbol_id: i.to_string(),
                    to_symbol_id: (i + 1).to_string(),
                    kind: RelationshipKind::References,
                    file_path: format!("src/file_{}.rs", i),
                    line_number: 10,
                    confidence: 0.9,
                    metadata: Some(HashMap::new()),
                };
                relationships.push(relationship);
            }
        }

        // Format the response (this will need to be implemented)
        let result = explore_tool.format_optimized_results(&symbols, &relationships);

        // Should contain all basic overview elements without optimization for small responses
        assert!(result.contains("🧭 Codebase Overview"));
        assert!(result.contains("Total Symbols: 5"));
        assert!(result.contains("Total Files: 5"));
        assert!(result.contains("Total Relationships: 4"));
        assert!(result.contains("Symbol Types:"));
        assert!(result.contains("Languages:"));

        // Should NOT contain progressive reduction messages for small responses
        assert!(!result.contains("Applied progressive reduction"));
        assert!(!result.contains("Response truncated to stay within token limits"));
    }

    #[test]
    fn test_fast_explore_large_codebase_needs_reduction() {
        let explore_tool = FastExploreTool {
            mode: "overview".to_string(),
            depth: "deep".to_string(),
            focus: None,
        };

        // Create a massive codebase simulation to trigger token optimization
        let mut symbols = Vec::new();
        let mut relationships = Vec::new();

        // Create 1000 symbols across many files and languages to trigger progressive reduction
        let languages = vec!["rust", "typescript", "python", "java", "go", "cpp", "javascript", "php"];
        let symbol_types = vec![
            SymbolKind::Function, SymbolKind::Class, SymbolKind::Variable,
            SymbolKind::Module, SymbolKind::Interface, SymbolKind::Struct,
            SymbolKind::Enum, SymbolKind::Trait
        ];

        for i in 1..=1000 {
            let lang = languages[i % languages.len()];
            let kind = symbol_types[i % symbol_types.len()].clone();

            let symbol = Symbol {
                id: i.to_string(),
                name: format!("extremely_long_descriptive_symbol_name_with_detailed_functionality_description_{}", i),
                kind,
                language: lang.to_string(),
                file_path: format!("src/extremely/deeply/nested/module/structure/with/many/levels/of/organization/{}/implementation/details/file_with_very_long_descriptive_name_{}.{}",
                    lang, i, if lang == "rust" { "rs" } else if lang == "typescript" { "ts" } else { "ext" }),
                start_line: (i * 10) as u32,
                start_column: 0,
                end_line: (i * 10 + 20) as u32,
                end_column: 0,
                start_byte: 0,
                end_byte: 500,
                signature: Some(format!("fn extremely_long_descriptive_symbol_name_with_detailed_functionality_description_{}() -> Result<ComplexReturnTypeWithManyGenerics, VeryDescriptiveErrorType>", i)),
                doc_comment: Some(format!("/// This is a very detailed documentation comment for symbol {} that provides extensive information about its functionality and usage patterns", i)),
                visibility: Some(Visibility::Public),
                parent_id: None,
                metadata: Some(HashMap::new()),
                semantic_group: None,
                confidence: Some(0.9),
                code_context: None,
            };
            symbols.push(symbol);

            // Create multiple relationships per symbol to increase content
            for j in 1..=3 {
                if i > j {
                    let relationship = Relationship {
                        id: format!("rel_{}_{}", i, j),
                        from_symbol_id: i.to_string(),
                        to_symbol_id: (i - j).to_string(),
                        kind: RelationshipKind::References,
                        file_path: format!("src/extremely/deeply/nested/module/structure/with/many/levels/of/organization/{}/implementation/details/file_with_very_long_descriptive_name_{}.{}",
                            lang, i, if lang == "rust" { "rs" } else if lang == "typescript" { "ts" } else { "ext" }),
                        line_number: (i * 10 + j) as u32,
                        confidence: 0.9,
                        metadata: Some(HashMap::new()),
                    };
                    relationships.push(relationship);
                }
            }
        }

        let result = explore_tool.format_optimized_results(&symbols, &relationships);

        // Should contain progressive reduction notice for large responses
        assert!(result.contains("Applied progressive reduction") || result.contains("Response truncated to stay within token limits"));

        // Should contain basic overview but with reduced detail
        assert!(result.contains("🧭 Codebase Overview"));
        assert!(result.contains("Total Symbols: 1000"));

        // Should show accurate count after reduction (not showing full details)
        assert!(!result.contains("showing all 1000 symbols in detail"));
    }

    #[test]
    fn test_fast_explore_respects_token_limits_different_modes() {
        let explore_tool = FastExploreTool {
            mode: "dependencies".to_string(),
            depth: "deep".to_string(),
            focus: Some("performance".to_string()),
        };

        // Create 500 symbols with extensive dependency relationships
        let mut symbols = Vec::new();
        let mut relationships = Vec::new();

        for i in 1..=500 {
            let symbol = Symbol {
                id: i.to_string(),
                name: format!("complex_dependency_symbol_with_extensive_interconnections_and_detailed_naming_{}", i),
                kind: SymbolKind::Function,
                language: "rust".to_string(),
                file_path: format!("src/complex/dependency/graph/with/extensive/module/hierarchies/performance_critical/module_{}/implementation_file_{}.rs", i % 50, i),
                start_line: 1,
                start_column: 0,
                end_line: 50,
                end_column: 0,
                start_byte: 0,
                end_byte: 1000,
                signature: Some(format!("fn complex_dependency_symbol_with_extensive_interconnections_and_detailed_naming_{}() -> Result<PerformanceCriticalReturnType, DetailedErrorInformation>", i)),
                doc_comment: Some(format!("/// Performance-critical function {} with complex dependencies and extensive documentation describing optimization strategies and implementation details", i)),
                visibility: Some(Visibility::Public),
                parent_id: None,
                metadata: Some(HashMap::new()),
                semantic_group: Some(format!("performance_group_{}", i % 10)),
                confidence: Some(0.95),
                code_context: None,
            };
            symbols.push(symbol);

            // Create complex dependency relationships
            for j in 1..=5 {
                if i > j {
                    let relationship = Relationship {
                        id: format!("dep_{}_{}", i, j),
                        from_symbol_id: i.to_string(),
                        to_symbol_id: (i - j).to_string(),
                        kind: RelationshipKind::References,
                        file_path: format!("src/complex/dependency/graph/with/extensive/module/hierarchies/performance_critical/module_{}/implementation_file_{}.rs", i % 50, i),
                        line_number: (j * 10) as u32,
                        confidence: 0.95,
                        metadata: Some(HashMap::new()),
                    };
                    relationships.push(relationship);
                }
            }
        }

        let result = explore_tool.format_optimized_results(&symbols, &relationships);

        // Should apply token optimization for dependencies mode
        let has_progressive_reduction = result.contains("Applied progressive reduction");
        let has_early_termination = result.contains("Response truncated to stay within token limits");
        assert!(has_progressive_reduction || has_early_termination);

        // Should contain mode-specific content
        assert!(result.contains("dependencies") || result.contains("Dependencies"));

        // Should include first symbols but may exclude later ones due to token limits
        assert!(result.contains("complex_dependency_symbol_with_extensive_interconnections_and_detailed_naming_1"));
    }

    #[test]
    fn test_fast_explore_ignores_code_context_bug() {
        let explore_tool = FastExploreTool {
            mode: "overview".to_string(),
            depth: "deep".to_string(),
            focus: None,
        };

        // BUG REPRODUCTION: Create symbols with LARGE code_context that should trigger token optimization
        // This test should FAIL because FastExploreTool ignores code_context (unlike other tools)
        let massive_code_context = (1..=100).map(|line| {
            format!("    // Massive context line {} with extremely detailed content that includes comprehensive documentation, implementation details, usage examples, error handling patterns, performance considerations, security protocols, validation logic, business rules, integration patterns, and extensive technical specifications that should definitely trigger token optimization when properly included in output formatting", line)
        }).collect::<Vec<_>>().join("\n");

        let mut symbols = Vec::new();

        // Create just 5 symbols but with MASSIVE code_context (should trigger token limits)
        for i in 1..=5 {
            let symbol = Symbol {
                id: i.to_string(),
                name: format!("TestSymbol{}", i),
                kind: SymbolKind::Function,
                language: "rust".to_string(),
                file_path: format!("file_{}.rs", i),
                start_line: 10,
                start_column: 0,
                end_line: 50,
                end_column: 0,
                start_byte: 0,
                end_byte: 500,
                signature: Some(format!("fn test_symbol_{}()", i)),
                doc_comment: Some(format!("Test symbol {}", i)),
                visibility: Some(Visibility::Public),
                parent_id: None,
                metadata: Some(HashMap::new()),
                semantic_group: None,
                confidence: Some(0.9),
                code_context: Some(massive_code_context.clone()), // THIS IS THE KEY - MASSIVE CONTENT
            };
            symbols.push(symbol);
        }

        let relationships = vec![]; // Empty for focused testing

        let result = explore_tool.format_optimized_results(&symbols, &relationships);

        // BUG: This assertion should FAIL because FastExploreTool ignores code_context
        // Even with massive code_context, progressive reduction won't trigger because
        // FastExploreTool doesn't include code_context in its output like other tools do
        assert!(
            result.contains("Applied progressive reduction") ||
            result.contains("Response truncated to stay within token limits"),
            "FastExploreTool should trigger progressive reduction with massive code_context, but it ignores code_context unlike other tools"
        );

        // This test documents the exact bug:
        // - Other tools (FastGotoTool, FindLogicTool) include code_context → trigger progressive reduction
        // - FastExploreTool ignores code_context → never triggers progressive reduction
        // - Expected: FastExploreTool should include code_context like other tools
    }
}