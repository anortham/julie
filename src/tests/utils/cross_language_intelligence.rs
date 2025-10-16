// Inline tests extracted from src/utils/cross_language_intelligence.rs
//
// These tests validate the cross-language intelligence module including:
// - Naming convention conversions (snake_case, camelCase, PascalCase, kebab-case, SCREAMING_SNAKE_CASE)
// - Naming variant generation
// - Symbol kind equivalence mapping
// - Intelligence configuration presets

#[cfg(test)]
mod tests {
    use crate::utils::cross_language_intelligence::{
        generate_naming_variants, to_camel_case, to_kebab_case, to_pascal_case,
        to_screaming_snake_case, to_snake_case, IntelligenceConfig, SymbolKindEquivalence,
    };
    use crate::extractors::SymbolKind;

    #[test]
    fn test_snake_case_conversion() {
        assert_eq!(to_snake_case("getUserData"), "get_user_data");
        assert_eq!(to_snake_case("HTTPServer"), "http_server");
        assert_eq!(to_snake_case("parseXMLFile"), "parse_xml_file");
        assert_eq!(to_snake_case("already_snake"), "already_snake");
    }

    #[test]
    fn test_camel_case_conversion() {
        assert_eq!(to_camel_case("get_user_data"), "getUserData");
        assert_eq!(to_camel_case("http-server"), "httpServer");
        assert_eq!(to_camel_case("ParseXMLFile"), "parseXMLFile");
        assert_eq!(to_camel_case("alreadyCamel"), "alreadyCamel");
    }

    #[test]
    fn test_pascal_case_conversion() {
        assert_eq!(to_pascal_case("get_user_data"), "GetUserData");
        assert_eq!(to_pascal_case("http-server"), "HttpServer");
        assert_eq!(to_pascal_case("parseXMLFile"), "ParseXMLFile");
        assert_eq!(to_pascal_case("AlreadyPascal"), "AlreadyPascal");
    }

    #[test]
    fn test_kebab_case_conversion() {
        assert_eq!(to_kebab_case("getUserData"), "get-user-data");
        assert_eq!(to_kebab_case("HTTPServer"), "http-server");
    }

    #[test]
    fn test_screaming_snake_case_conversion() {
        assert_eq!(to_screaming_snake_case("getUserData"), "GET_USER_DATA");
        assert_eq!(to_screaming_snake_case("maxConnections"), "MAX_CONNECTIONS");
    }

    #[test]
    fn test_generate_naming_variants() {
        let variants = generate_naming_variants("getUserData");
        assert!(variants.contains(&"getUserData".to_string())); // original
        assert!(variants.contains(&"get_user_data".to_string())); // snake
        assert!(variants.contains(&"GetUserData".to_string())); // pascal
        assert!(variants.contains(&"GET_USER_DATA".to_string())); // screaming
        assert!(variants.len() >= 4); // at least these 4
    }

    #[test]
    fn test_symbol_kind_equivalence() {
        let eq = SymbolKindEquivalence::new();

        // Class-like equivalence
        assert!(eq.are_equivalent(SymbolKind::Class, SymbolKind::Struct));
        assert!(eq.are_equivalent(SymbolKind::Class, SymbolKind::Interface));
        assert!(eq.are_equivalent(SymbolKind::Struct, SymbolKind::Interface));

        // Function-like equivalence
        assert!(eq.are_equivalent(SymbolKind::Function, SymbolKind::Method));

        // Non-equivalent
        assert!(!eq.are_equivalent(SymbolKind::Class, SymbolKind::Function));
        assert!(!eq.are_equivalent(SymbolKind::Variable, SymbolKind::Constant));
    }

    #[test]
    fn test_get_equivalents() {
        let eq = SymbolKindEquivalence::new();
        let class_equiv = eq.get_equivalents(SymbolKind::Class);

        assert!(class_equiv.contains(&SymbolKind::Class));
        assert!(class_equiv.contains(&SymbolKind::Struct));
        assert!(class_equiv.contains(&SymbolKind::Interface));
    }

    #[test]
    fn test_intelligence_config_presets() {
        let strict = IntelligenceConfig::strict();
        assert!(strict.enable_naming_variants);
        assert!(!strict.enable_semantic_similarity);
        assert_eq!(strict.semantic_similarity_threshold, 0.9);

        let relaxed = IntelligenceConfig::relaxed();
        assert!(relaxed.enable_naming_variants);
        assert!(relaxed.enable_semantic_similarity);
        assert_eq!(relaxed.semantic_similarity_threshold, 0.6);
    }
}
