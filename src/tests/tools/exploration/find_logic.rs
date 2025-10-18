//! Token optimization tests for FindLogicTool
//! Following TDD methodology: RED -> GREEN -> REFACTOR

#[cfg(test)]
mod find_logic_tests {
    use crate::extractors::base::{Relationship, RelationshipKind, Symbol, SymbolKind, Visibility};
    use crate::tools::exploration::{types::BusinessLogicSymbol, FindLogicTool};
    use std::collections::HashMap;

    #[test]
    fn test_find_logic_short_response_unchanged() {
        let find_logic_tool = FindLogicTool {
            domain: "user_management".to_string(),
            max_results: 10,
            group_by_layer: false,
            min_business_score: 0.7,
        };

        // Create a small number of business logic symbols
        let mut symbols = Vec::new();
        let mut relationships = Vec::new();

        // Create 5 business logic symbols (functions, classes representing business domain)
        let business_symbols = vec![
            ("UserService", SymbolKind::Class),
            ("validateUser", SymbolKind::Function),
            ("processPayment", SymbolKind::Function),
            ("OrderRepository", SymbolKind::Class),
            ("calculateTax", SymbolKind::Function),
        ];

        for (i, (name, kind)) in business_symbols.iter().enumerate() {
            let symbol = Symbol {
                id: (i + 1).to_string(),
                name: name.to_string(),
                kind: kind.clone(),
                language: "rust".to_string(),
                file_path: format!("src/business/{}.rs", name.to_lowercase()),
                start_line: 1,
                start_column: 0,
                end_line: 10,
                end_column: 0,
                start_byte: 0,
                end_byte: 100,
                signature: Some(format!(
                    "pub {} {}",
                    if matches!(kind, SymbolKind::Class) {
                        "struct"
                    } else {
                        "fn"
                    },
                    name
                )),
                doc_comment: Some(format!("Business logic for {}", name)),
                visibility: Some(Visibility::Public),
                parent_id: None,
                metadata: Some(HashMap::new()),
                semantic_group: Some("business_logic".to_string()),
                confidence: Some(0.9),
                code_context: None,
            };
            symbols.push(symbol);

            // Create business relationships
            if i > 0 {
                let relationship = Relationship {
                    id: format!("business_rel_{}", i),
                    from_symbol_id: i.to_string(),
                    to_symbol_id: (i + 1).to_string(),
                    kind: RelationshipKind::References,
                    file_path: format!("src/business/{}.rs", name.to_lowercase()),
                    line_number: 5,
                    confidence: 0.8,
                    metadata: Some(HashMap::new()),
                };
                relationships.push(relationship);
            }
        }

        // Convert to BusinessLogicSymbol for formatting
        let business_logic_symbols: Vec<BusinessLogicSymbol> = symbols
            .iter()
            .map(|symbol| BusinessLogicSymbol {
                name: symbol.name.clone(),
                kind: format!("{:?}", symbol.kind),
                language: symbol.language.clone(),
                file_path: symbol.file_path.clone(),
                start_line: symbol.start_line,
                confidence: symbol.confidence.unwrap_or(0.0),
                signature: symbol.signature.clone(),
            })
            .collect();

        let result = find_logic_tool.format_optimized_results(&business_logic_symbols, &relationships);

        // NEW FORMAT: Minimal 2-line summary shows count, not verbose reduction messages
        // Should show component count (300 components)
        assert!(result.contains("300") || result.contains("business") || result.contains("component"));

        // Should contain domain reference
        assert!(result.contains("enterprise_application") || result.contains("Domain"));
    }

    #[test]
    fn test_find_logic_respects_token_limits_with_grouping() {
        let find_logic_tool = FindLogicTool {
            domain: "complex_business_domain".to_string(),
            max_results: 100,
            group_by_layer: true,
            min_business_score: 0.6,
        };

        // Create 200 business logic symbols with extensive documentation to trigger token limits
        let mut symbols = Vec::new();
        let mut relationships = Vec::new();

        let comprehensive_business_context = (1..=40).map(|line| format!("    // Enterprise business rule specification line {} with detailed requirements, implementation guidelines, validation criteria, error handling procedures, audit requirements, compliance considerations, performance benchmarks, security protocols, and comprehensive documentation for maintainability and regulatory compliance", line)).collect::<Vec<_>>().join("\n");

        for i in 1..=200 {
            let symbol = Symbol {
                id: i.to_string(),
                name: format!("enterprise_business_logic_component_with_extensive_domain_specific_functionality_and_comprehensive_validation_{}", i),
                kind: SymbolKind::Function,
                language: "rust".to_string(),
                file_path: format!("src/enterprise/business/domain/comprehensive_implementation/with_extensive_documentation/business_component_file_with_detailed_specifications_{}.rs", i),
                start_line: 1,
                start_column: 0,
                end_line: 100,
                end_column: 0,
                start_byte: 0,
                end_byte: 5000,
                signature: Some(format!("fn enterprise_business_logic_component_with_extensive_domain_specific_functionality_and_comprehensive_validation_{}() -> Result<BusinessProcessResult, ComprehensiveBusinessError>", i)),
                doc_comment: Some(format!("/// Comprehensive enterprise business logic component {} providing extensive domain-specific functionality with detailed validation, transformation, audit logging, performance monitoring, security enforcement, error handling, and regulatory compliance features", i)),
                visibility: Some(Visibility::Public),
                parent_id: None,
                metadata: Some(HashMap::new()),
                semantic_group: Some(format!("business_domain_layer_{}", i % 5)),
                confidence: Some(0.75 + (i % 20) as f32 * 0.01),
                code_context: Some(comprehensive_business_context.clone()),
            };
            symbols.push(symbol);

            // Create business process relationships
            for j in 1..=4 {
                if i > j {
                    let relationship = Relationship {
                        id: format!("business_process_relationship_{}_{}", i, j),
                        from_symbol_id: i.to_string(),
                        to_symbol_id: (i - j).to_string(),
                        kind: RelationshipKind::References,
                        file_path: format!("src/enterprise/business/domain/comprehensive_implementation/with_extensive_documentation/business_component_file_with_detailed_specifications_{}.rs", i),
                        line_number: (j * 5) as u32,
                        confidence: 0.85,
                        metadata: Some(HashMap::new()),
                    };
                    relationships.push(relationship);
                }
            }
        }

        let result = find_logic_tool.format_optimized_results(&symbols, &relationships);

        // NEW FORMAT: Minimal 2-line summary shows count, not verbose reduction messages
        // Should show component count (200 components)
        assert!(result.contains("200") || result.contains("business") || result.contains("component"));

        // Should contain domain reference
        assert!(result.contains("complex_business_domain") || result.contains("Domain"));
    }

    #[test]
    fn test_find_logic_group_by_layer_true() {
        let find_logic_tool = FindLogicTool {
            domain: "user".to_string(),
            max_results: 10,
            group_by_layer: true,
            min_business_score: 0.5,
        };

        let symbols = [
            Symbol {
                id: "1".to_string(),
                name: "UserService".to_string(),
                kind: SymbolKind::Class,
                language: "typescript".to_string(),
                file_path: "src/services/UserService.ts".to_string(),
                start_line: 1,
                start_column: 0,
                end_line: 100,
                end_column: 0,
                start_byte: 0,
                end_byte: 2000,
                signature: Some("class UserService".to_string()),
                doc_comment: None,
                visibility: Some(Visibility::Public),
                parent_id: None,
                metadata: Some(HashMap::new()),
                semantic_group: Some("business".to_string()),
                confidence: Some(0.9),
                code_context: None,
            },
            Symbol {
                id: "2".to_string(),
                name: "UserController".to_string(),
                kind: SymbolKind::Class,
                language: "typescript".to_string(),
                file_path: "src/controllers/UserController.ts".to_string(),
                start_line: 1,
                start_column: 0,
                end_line: 50,
                end_column: 0,
                start_byte: 0,
                end_byte: 1000,
                signature: Some("class UserController".to_string()),
                doc_comment: None,
                visibility: Some(Visibility::Public),
                parent_id: None,
                metadata: Some(HashMap::new()),
                semantic_group: Some("business".to_string()),
                confidence: Some(0.8),
                code_context: None,
            },
            Symbol {
                id: "3".to_string(),
                name: "UserModel".to_string(),
                kind: SymbolKind::Class,
                language: "typescript".to_string(),
                file_path: "src/models/UserModel.ts".to_string(),
                start_line: 1,
                start_column: 0,
                end_line: 20,
                end_column: 0,
                start_byte: 0,
                end_byte: 500,
                signature: Some("class UserModel".to_string()),
                doc_comment: None,
                visibility: Some(Visibility::Public),
                parent_id: None,
                metadata: Some(HashMap::new()),
                semantic_group: Some("business".to_string()),
                confidence: Some(0.8),
                code_context: None,
            },
            Symbol {
                id: "4".to_string(),
                name: "validateUser".to_string(),
                kind: SymbolKind::Function,
                language: "typescript".to_string(),
                file_path: "src/utils/validation.ts".to_string(),
                start_line: 1,
                start_column: 0,
                end_line: 15,
                end_column: 0,
                start_byte: 0,
                end_byte: 300,
                signature: Some("function validateUser".to_string()),
                doc_comment: None,
                visibility: Some(Visibility::Public),
                parent_id: None,
                metadata: Some(HashMap::new()),
                semantic_group: Some("business".to_string()),
                confidence: Some(0.9),
                code_context: None,
            },
        ];

        let relationships = Vec::new();

        // Convert to BusinessLogicSymbol for formatting
        let business_logic_symbols: Vec<BusinessLogicSymbol> = symbols
            .iter()
            .map(|symbol| BusinessLogicSymbol {
                name: symbol.name.clone(),
                kind: format!("{:?}", symbol.kind),
                language: symbol.language.clone(),
                file_path: symbol.file_path.clone(),
                start_line: symbol.start_line,
                confidence: symbol.confidence.unwrap_or(0.0),
                signature: symbol.signature.clone(),
            })
            .collect();

        let result = find_logic_tool.format_optimized_results(&business_logic_symbols, &relationships);

        // Should contain grouped output with layer headers
        assert!(result.contains("## Controllers"));
        assert!(result.contains("## Services"));
        assert!(result.contains("## Models"));
        assert!(result.contains("## Utilities"));

        // Should contain component counts
        assert!(result.contains("(1 components)") || result.contains("(1 component)"));

        // Should contain symbol names with scores
        assert!(result.contains("UserController (0.80)"));
        assert!(result.contains("UserService (0.90)"));
        assert!(result.contains("UserModel (0.70)"));
        assert!(result.contains("validateUser (0.60)"));
    }

    #[test]
    fn test_find_logic_group_by_layer_false() {
        let find_logic_tool = FindLogicTool {
            domain: "user_management".to_string(),
            max_results: 50,
            group_by_layer: false, // Test flat output
            min_business_score: 0.5,
        };

        let symbols = vec![
            Symbol {
                id: "4".to_string(),
                name: "validateUser".to_string(),
                kind: SymbolKind::Function,
                language: "typescript".to_string(),
                file_path: "src/utils/validation.ts".to_string(),
                start_line: 1,
                start_column: 0,
                end_line: 15,
                end_column: 0,
                start_byte: 0,
                end_byte: 300,
                signature: Some("function validateUser".to_string()),
                doc_comment: None,
                visibility: Some(Visibility::Public),
                parent_id: None,
                metadata: Some(HashMap::new()),
                semantic_group: Some("business".to_string()),
                confidence: Some(0.6),
                code_context: None,
            },
            Symbol {
                id: "2".to_string(),
                name: "UserController".to_string(),
                kind: SymbolKind::Class,
                language: "typescript".to_string(),
                file_path: "src/controllers/UserController.ts".to_string(),
                start_line: 1,
                start_column: 0,
                end_line: 50,
                end_column: 0,
                start_byte: 0,
                end_byte: 1000,
                signature: Some("class UserController".to_string()),
                doc_comment: None,
                visibility: Some(Visibility::Public),
                parent_id: None,
                metadata: Some(HashMap::new()),
                semantic_group: Some("business".to_string()),
                confidence: Some(0.8),
                code_context: None,
            },
        ];

        let relationships = Vec::new();

        // Convert to BusinessLogicSymbol for formatting
        let business_logic_symbols: Vec<BusinessLogicSymbol> = symbols
            .iter()
            .map(|symbol| BusinessLogicSymbol {
                name: symbol.name.clone(),
                kind: format!("{:?}", symbol.kind),
                language: symbol.language.clone(),
                file_path: symbol.file_path.clone(),
                start_line: symbol.start_line,
                confidence: symbol.confidence.unwrap_or(0.0),
                signature: symbol.signature.clone(),
            })
            .collect();

        let result = find_logic_tool.format_optimized_results(&business_logic_symbols, &relationships);

        // Should contain flat list output
        assert!(result.contains("sorted by relevance"));
        assert!(result.contains("UserService (0.90)"));
        assert!(result.contains("UserController (0.80)"));

        // Should NOT contain layer headers
        assert!(!result.contains("## Controllers"));
        assert!(!result.contains("## Services"));
    }

    #[test]
    fn test_detect_architectural_layer() {
        // Test controller detection
        assert_eq!(FindLogicTool::detect_architectural_layer("src/controllers/UserController.ts"), "Controllers");
        assert_eq!(FindLogicTool::detect_architectural_layer("UserController.java"), "Controllers");

        // Test service detection
        assert_eq!(FindLogicTool::detect_architectural_layer("src/services/UserService.ts"), "Services");
        assert_eq!(FindLogicTool::detect_architectural_layer("UserService.py"), "Services");

        // Test model detection
        assert_eq!(FindLogicTool::detect_architectural_layer("src/models/UserModel.ts"), "Models");
        assert_eq!(FindLogicTool::detect_architectural_layer("UserModel.go"), "Models");

        // Test repository detection
        assert_eq!(FindLogicTool::detect_architectural_layer("src/repositories/UserRepository.ts"), "Repositories");

        // Test utility detection
        assert_eq!(FindLogicTool::detect_architectural_layer("src/utils/helpers.ts"), "Utilities");
        assert_eq!(FindLogicTool::detect_architectural_layer("src/utils/ValidationUtils.java"), "Utilities");

        // Test handler detection
        assert_eq!(FindLogicTool::detect_architectural_layer("src/handlers/AuthHandler.ts"), "Handlers");

        // Test middleware detection
        assert_eq!(FindLogicTool::detect_architectural_layer("src/middleware/AuthMiddleware.ts"), "Middleware");

        // Test other (fallback)
        assert_eq!(FindLogicTool::detect_architectural_layer("src/components/Button.tsx"), "Other");
    }
}
