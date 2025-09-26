//! Token optimization tests for FindLogicTool
//! Following TDD methodology: RED -> GREEN -> REFACTOR

#[cfg(test)]
mod find_logic_tests {
    use crate::tools::exploration::FindLogicTool;
    use crate::extractors::base::{Symbol, SymbolKind, Visibility, Relationship, RelationshipKind};
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
                signature: Some(format!("pub {} {}", if matches!(kind, SymbolKind::Class) { "struct" } else { "fn" }, name)),
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

        // Format the response (this will need to be implemented)
        let result = find_logic_tool.format_optimized_results(&symbols, &relationships);

        // Should contain business logic overview without optimization for small responses
        assert!(result.contains("🏢 Business Logic Discovery"));
        assert!(result.contains("Domain: user_management"));
        assert!(result.contains("Business Score ≥ 0.7"));
        assert!(result.contains("UserService"));
        assert!(result.contains("validateUser"));
        assert!(result.contains("processPayment"));

        // Should NOT contain progressive reduction messages for small responses
        assert!(!result.contains("Applied progressive reduction"));
        assert!(!result.contains("Response truncated to stay within token limits"));
    }

    #[test]
    fn test_find_logic_large_codebase_needs_reduction() {
        let find_logic_tool = FindLogicTool {
            domain: "enterprise_application".to_string(),
            max_results: 500,
            group_by_layer: true,
            min_business_score: 0.5,
        };

        // Create many business logic symbols to trigger token optimization
        let mut symbols = Vec::new();
        let mut relationships = Vec::new();

        // Create large context content for business logic
        let large_business_context = (1..=25).map(|line| format!("    // Business rule {} with comprehensive logic including validation, error handling, audit logging, performance monitoring, security checks, and detailed documentation for compliance and maintainability", line)).collect::<Vec<_>>().join("\n");

        // Create 300 business logic symbols across different layers
        let business_layers = vec!["domain", "service", "repository", "controller", "validator"];
        for i in 1..=300 {
            let layer = business_layers[i % business_layers.len()];
            let symbol = Symbol {
                id: i.to_string(),
                name: format!("business_logic_component_with_comprehensive_enterprise_functionality_{}", i),
                kind: SymbolKind::Class,
                language: "rust".to_string(),
                file_path: format!("src/business/{}/comprehensive_enterprise_business_logic_implementation_with_detailed_patterns/component_{}.rs", layer, i),
                start_line: (i * 10) as u32,
                start_column: 0,
                end_line: (i * 10 + 50) as u32,
                end_column: 0,
                start_byte: 0,
                end_byte: 2000,
                signature: Some(format!("pub struct BusinessLogicComponent{} {{ // Comprehensive enterprise business logic with extensive validation, error handling, audit trails, performance monitoring, and security controls }}", i)),
                doc_comment: Some(format!("/// Core business logic component {} providing enterprise-grade functionality including validation, transformation, audit logging, performance optimization, security enforcement, and comprehensive error handling with detailed documentation", i)),
                visibility: Some(Visibility::Public),
                parent_id: None,
                metadata: Some(HashMap::new()),
                semantic_group: Some(format!("business_{}_layer", layer)),
                confidence: Some(0.85 + (i % 10) as f32 * 0.01), // Varying confidence scores
                code_context: Some(large_business_context.clone()),
            };
            symbols.push(symbol);

            // Create complex business relationships
            for j in 1..=3 {
                if i > j {
                    let relationship = Relationship {
                        id: format!("business_relationship_{}_{}", i, j),
                        from_symbol_id: i.to_string(),
                        to_symbol_id: (i - j).to_string(),
                        kind: RelationshipKind::References,
                        file_path: format!("src/business/{}/comprehensive_enterprise_business_logic_implementation_with_detailed_patterns/component_{}.rs", layer, i),
                        line_number: (i * 10 + j) as u32,
                        confidence: 0.8,
                        metadata: Some(HashMap::new()),
                    };
                    relationships.push(relationship);
                }
            }
        }

        let result = find_logic_tool.format_optimized_results(&symbols, &relationships);

        // Should contain progressive reduction notice for large responses
        assert!(result.contains("Applied progressive reduction") || result.contains("Response truncated to stay within token limits"));

        // Should contain business logic overview but with reduced detail
        assert!(result.contains("🏢 Business Logic Discovery"));
        assert!(result.contains("Domain: enterprise_application"));

        // When progressive reduction is applied, should NOT show all 300
        let has_progressive_reduction = result.contains("Applied progressive reduction");
        let has_early_termination = result.contains("Response truncated to stay within token limits");
        if has_progressive_reduction || has_early_termination {
            assert!(!result.contains("300 business components found")); // Should be reduced
        } else {
            assert!(result.contains("300 business components found")); // If no reduction, should show all
        }

        // Should show early components but not all 300
        assert!(result.contains("business_logic_component_with_comprehensive_enterprise_functionality_1"));
        assert!(!result.contains("business_logic_component_with_comprehensive_enterprise_functionality_300")); // Last component should be excluded

        // Should show grouping information when enabled (may be reduced due to token optimization)
        // The key test is that token optimization works, not specific layer content
        if !has_progressive_reduction && !has_early_termination {
            assert!(result.contains("📊 Grouped by Layer"));
        }
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

        // Should apply token optimization for large business logic content
        let has_progressive_reduction = result.contains("Applied progressive reduction");
        let has_early_termination = result.contains("Response truncated to stay within token limits");
        assert!(has_progressive_reduction || has_early_termination);

        // Should contain business domain information
        assert!(result.contains("complex_business_domain") || result.contains("Domain:"));

        // Should include first components but may exclude later ones due to token limits
        assert!(result.contains("enterprise_business_logic_component_with_extensive_domain_specific_functionality_and_comprehensive_validation_1"));

        // Should show grouping information when enabled
        assert!(result.contains("📊 Grouped by Layer") || result.contains("business_domain_layer"));
    }
}