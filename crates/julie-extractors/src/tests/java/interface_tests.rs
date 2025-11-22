// Interface Extraction Tests
//
// Tests for Java interface extraction including:
// - Interface definitions with modifiers
// - Abstract methods
// - Default methods (Java 8+)
// - Static methods (Java 8+)

use crate::base::{SymbolKind, Visibility};
use crate::java::JavaExtractor;
use crate::tests::test_utils::init_parser;
use std::path::PathBuf;

#[cfg(test)]
mod interface_tests {
    use super::*;

    #[test]
    fn test_extract_interface_definitions() {
        let workspace_root = PathBuf::from("/tmp/test");
        let code = r#"
package com.example;

public interface Calculator {
    int add(int a, int b);
    int subtract(int a, int b);
}

interface AdvancedCalculator extends Calculator {
    double power(double base, int exponent);
}

private interface InternalInterface {
    void internalMethod();
}
"#;

        let tree = init_parser(code, "java");

        let mut extractor = JavaExtractor::new(
            "java".to_string(),
            "test.java".to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);

        let calculator_interface = symbols.iter().find(|s| s.name == "Calculator");
        assert!(calculator_interface.is_some());
        assert_eq!(calculator_interface.unwrap().kind, SymbolKind::Interface);
        assert!(
            calculator_interface
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("public interface Calculator")
        );

        let advanced_interface = symbols.iter().find(|s| s.name == "AdvancedCalculator");
        assert!(advanced_interface.is_some());
        assert_eq!(advanced_interface.unwrap().kind, SymbolKind::Interface);
        assert!(
            advanced_interface
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("interface AdvancedCalculator extends Calculator")
        );
    }

    #[test]
    fn test_extract_interface_methods() {
        let workspace_root = PathBuf::from("/tmp/test");
        let code = r#"
public interface Service {
    void process();                    // Abstract method
    default String getName() {         // Default method
        return "Service";
    }
    static boolean isValid() {         // Static method
        return true;
    }
}
"#;

        let tree = init_parser(code, "java");

        let mut extractor = JavaExtractor::new(
            "java".to_string(),
            "test.java".to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);

        let process_method = symbols.iter().find(|s| s.name == "process");
        assert!(process_method.is_some());
        assert_eq!(process_method.unwrap().kind, SymbolKind::Method);

        let get_name_method = symbols.iter().find(|s| s.name == "getName");
        assert!(get_name_method.is_some());
        assert_eq!(get_name_method.unwrap().kind, SymbolKind::Method);

        let is_valid_method = symbols.iter().find(|s| s.name == "isValid");
        assert!(is_valid_method.is_some());
        assert_eq!(is_valid_method.unwrap().kind, SymbolKind::Method);
    }
}
