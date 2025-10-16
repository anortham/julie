// Inline tests extracted from src/embeddings/cross_language.rs
//
// These tests validate the SemanticGrouper functionality including:
// - Levenshtein distance calculation
// - Name normalization
// - Name similarity checking
// - Common property extraction
// - Architectural pattern detection
// - Structural similarity calculations

#[cfg(test)]
mod tests {
    use crate::embeddings::cross_language::{
        ArchitecturalPattern, SemanticGrouper,
    };
    use crate::extractors::base::{Symbol, SymbolKind};

    #[test]
    fn test_levenshtein_distance() {
        let grouper = SemanticGrouper::new(0.7);

        // Identical strings
        assert_eq!(grouper.levenshtein_distance("hello", "hello"), 0);

        // One character difference
        assert_eq!(grouper.levenshtein_distance("hello", "hallo"), 1);

        // Multiple differences
        assert_eq!(grouper.levenshtein_distance("kitten", "sitting"), 3);

        // Empty strings
        assert_eq!(grouper.levenshtein_distance("", "hello"), 5);
        assert_eq!(grouper.levenshtein_distance("hello", ""), 5);

        // Completely different
        assert_eq!(grouper.levenshtein_distance("abc", "xyz"), 3);
    }

    #[test]
    fn test_normalize_name() {
        let grouper = SemanticGrouper::new(0.7);

        // Remove common suffixes
        assert_eq!(grouper.normalize_name("UserDto"), "user");
        assert_eq!(grouper.normalize_name("UserEntity"), "user");
        assert_eq!(grouper.normalize_name("UserModel"), "user");
        assert_eq!(grouper.normalize_name("users"), "user"); // Remove plural

        // Remove interface prefix
        assert_eq!(grouper.normalize_name("IUserRepository"), "userrepository");

        // Lowercase conversion
        assert_eq!(grouper.normalize_name("UserService"), "userservice");
    }

    #[test]
    fn test_names_are_similar() {
        let grouper = SemanticGrouper::new(0.7);

        // Exact match after normalization
        assert!(grouper.names_are_similar("user", "user"));

        // Normalized equivalents
        assert!(grouper.names_are_similar("user", "users"));

        assert!(grouper.names_are_similar("UserDto", "UserEntity"));

        // Containment
        assert!(grouper.names_are_similar("user", "userdata"));
        assert!(grouper.names_are_similar("repository", "userrepository"));

        // Similar with small differences
        assert!(grouper.names_are_similar("user", "usar")); // 1 char diff in 4 = 25%

        // Too different
        assert!(!grouper.names_are_similar("user", "product"));
        assert!(!grouper.names_are_similar("authentication", "database"));
    }

    #[test]
    fn test_has_name_similarity() {
        let grouper = SemanticGrouper::new(0.7);

        // Create similar symbols
        let user_ts = Symbol {
            id: "1".to_string(),
            name: "User".to_string(),
            kind: SymbolKind::Interface,
            language: "typescript".to_string(),
            file_path: "/frontend/types.ts".to_string(),
            start_line: 1,
            start_column: 1,
            end_line: 5,
            end_column: 1,
            start_byte: 0,
            end_byte: 100,
            signature: None,
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
        };

        let user_dto = Symbol {
            id: "2".to_string(),
            name: "UserDto".to_string(),
            kind: SymbolKind::Class,
            language: "csharp".to_string(),
            file_path: "/backend/Models/UserDto.cs".to_string(),
            start_line: 1,
            start_column: 1,
            end_line: 10,
            end_column: 1,
            start_byte: 0,
            end_byte: 200,
            signature: None,
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
        };

        let product_class = Symbol {
            id: "3".to_string(),
            name: "Product".to_string(),
            kind: SymbolKind::Class,
            language: "java".to_string(),
            file_path: "/backend/Product.java".to_string(),
            start_line: 1,
            start_column: 1,
            end_line: 10,
            end_column: 1,
            start_byte: 0,
            end_byte: 200,
            signature: None,
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
        };

        // Similar names should return true
        let similar_symbols = vec![&user_ts, &user_dto];
        assert!(grouper.has_name_similarity(&similar_symbols));

        // Different names should return false
        let different_symbols = vec![&user_ts, &product_class];
        assert!(!grouper.has_name_similarity(&different_symbols));

        // Single symbol should return false
        let single_symbol = vec![&user_ts];
        assert!(!grouper.has_name_similarity(&single_symbol));
    }

    #[test]
    fn test_extract_common_properties() {
        let grouper = SemanticGrouper::new(0.7);

        let user_ts = Symbol {
            id: "1".to_string(),
            name: "getUserData".to_string(),
            kind: SymbolKind::Function,
            language: "typescript".to_string(),
            file_path: "/frontend/user.ts".to_string(),
            start_line: 1,
            start_column: 1,
            end_line: 5,
            end_column: 1,
            start_byte: 0,
            end_byte: 100,
            signature: Some("function getUserData(): Promise<User>".to_string()),
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
        };

        let user_cs = Symbol {
            id: "2".to_string(),
            name: "GetUserData".to_string(),
            kind: SymbolKind::Method,
            language: "csharp".to_string(),
            file_path: "/backend/UserService.cs".to_string(),
            start_line: 1,
            start_column: 1,
            end_line: 10,
            end_column: 1,
            start_byte: 0,
            end_byte: 200,
            signature: Some("public User GetUserData()".to_string()),
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
        };

        let symbols = vec![&user_ts, &user_cs];
        let common_props = grouper.extract_common_properties(&symbols);

        // Should find common words like "user" and "data"
        assert!(common_props.contains(&"user".to_string()));
        assert!(common_props.contains(&"data".to_string()));
    }

    #[test]
    fn test_architectural_pattern_detection() {
        let grouper = SemanticGrouper::new(0.7);

        // Test FullStackEntity pattern (frontend + backend + database)
        let full_stack_symbols = vec![
            Symbol {
                id: "1".to_string(),
                name: "User".to_string(),
                kind: SymbolKind::Interface,
                language: "typescript".to_string(),
                file_path: "/frontend/types.ts".to_string(),
                start_line: 1,
                start_column: 1,
                end_line: 5,
                end_column: 1,
                start_byte: 0,
                end_byte: 100,
                signature: None,
                doc_comment: None,
                visibility: None,
                parent_id: None,
                metadata: None,
                semantic_group: None,
                confidence: None,
                code_context: None,
            },
            Symbol {
                id: "2".to_string(),
                name: "UserDto".to_string(),
                kind: SymbolKind::Class,
                language: "csharp".to_string(),
                file_path: "/backend/Models/UserDto.cs".to_string(),
                start_line: 1,
                start_column: 1,
                end_line: 10,
                end_column: 1,
                start_byte: 0,
                end_byte: 200,
                signature: None,
                doc_comment: None,
                visibility: None,
                parent_id: None,
                metadata: None,
                semantic_group: None,
                confidence: None,
                code_context: None,
            },
            Symbol {
                id: "3".to_string(),
                name: "users".to_string(),
                kind: SymbolKind::Type,
                language: "sql".to_string(),
                file_path: "/database/schema.sql".to_string(),
                start_line: 1,
                start_column: 1,
                end_line: 5,
                end_column: 1,
                start_byte: 0,
                end_byte: 150,
                signature: None,
                doc_comment: None,
                visibility: None,
                parent_id: None,
                metadata: None,
                semantic_group: None,
                confidence: None,
                code_context: None,
            },
        ];

        let pattern = grouper.detect_architectural_pattern(&full_stack_symbols);
        assert!(matches!(pattern, ArchitecturalPattern::FullStackEntity));

        // Test ApiContract pattern (frontend + backend, no database)
        let api_symbols = vec![
            full_stack_symbols[0].clone(), // TypeScript
            full_stack_symbols[1].clone(), // C#
        ];

        let pattern = grouper.detect_architectural_pattern(&api_symbols);
        assert!(matches!(pattern, ArchitecturalPattern::ApiContract));

        // Test DataLayer pattern (backend + database, no frontend)
        let data_symbols = vec![
            full_stack_symbols[1].clone(), // C#
            full_stack_symbols[2].clone(), // SQL
        ];

        let pattern = grouper.detect_architectural_pattern(&data_symbols);
        assert!(matches!(pattern, ArchitecturalPattern::DataLayer));

        // Test Unknown pattern (only one language)
        let single_symbols = vec![full_stack_symbols[0].clone()];
        let pattern = grouper.detect_architectural_pattern(&single_symbols);
        assert!(matches!(pattern, ArchitecturalPattern::Unknown));
    }

    #[test]
    fn test_calculate_structure_similarity() {
        let grouper = SemanticGrouper::new(0.7);

        // Symbols with signatures should have higher score
        let symbol_with_sig = Symbol {
            id: "1".to_string(),
            name: "User".to_string(),
            kind: SymbolKind::Interface,
            language: "typescript".to_string(),
            file_path: "/test.ts".to_string(),
            start_line: 1,
            start_column: 1,
            end_line: 5,
            end_column: 1,
            start_byte: 0,
            end_byte: 100,
            signature: Some("interface User { id: string; name: string; }".to_string()),
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
        };

        let symbol_without_sig = Symbol {
            id: "2".to_string(),
            name: "User".to_string(),
            kind: SymbolKind::Class,
            language: "java".to_string(),
            file_path: "/Test.java".to_string(),
            start_line: 1,
            start_column: 1,
            end_line: 10,
            end_column: 1,
            start_byte: 0,
            end_byte: 200,
            signature: None,
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
        };

        // All symbols with signatures
        let symbols_with_sigs = vec![&symbol_with_sig, &symbol_with_sig];
        let score = grouper.calculate_structure_similarity(&symbols_with_sigs);
        assert_eq!(score, 0.7);

        // Mixed signatures
        let mixed_symbols = vec![&symbol_with_sig, &symbol_without_sig];
        let score = grouper.calculate_structure_similarity(&mixed_symbols);
        assert_eq!(score, 0.5);

        // No signatures
        let no_sig_symbols = vec![&symbol_without_sig, &symbol_without_sig];
        let score = grouper.calculate_structure_similarity(&no_sig_symbols);
        assert_eq!(score, 0.3);
    }
}
