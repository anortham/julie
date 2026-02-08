// Inline tests extracted from src/extractors/rust/mod.rs
//
// This module contains tests for the main Rust extractor:
// - RustExtractor creation and initialization
// - Two-phase extraction (phase 1: all symbols, phase 2: impl blocks)
// - Tree walking and symbol extraction orchestration

#[cfg(test)]
mod tests {
    use crate::base::SymbolKind;
    use crate::rust::RustExtractor;
    use std::path::PathBuf;
    use tree_sitter::Parser;

    fn init_parser() -> Parser {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .expect("Error loading Rust grammar");
        parser
    }

    fn test_workspace_root() -> PathBuf {
        PathBuf::from("/tmp/test")
    }

    #[test]
    fn test_rust_extractor_creation() {
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = RustExtractor::new(
            "rust".to_string(),
            "test.rs".to_string(),
            "fn main() {}".to_string(),
            &workspace_root,
        );
        assert_eq!(extractor.get_base_mut().file_path, "test.rs");
    }

    #[test]
    fn test_impl_methods_extracted_with_local_type() {
        // impl for a struct defined in the same file — should always work
        let code = r#"
pub struct MyService {
    data: Vec<String>,
}

impl MyService {
    pub fn new() -> Self {
        Self { data: Vec::new() }
    }

    fn process(&self) -> bool {
        true
    }
}
"#;
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = RustExtractor::new(
            "rust".to_string(),
            "test.rs".to_string(),
            code.to_string(),
            &test_workspace_root(),
        );

        let symbols = extractor.extract_symbols(&tree);
        let methods: Vec<_> = symbols.iter().filter(|s| s.kind == SymbolKind::Method).collect();

        assert_eq!(methods.len(), 2, "Should extract 2 methods from impl block");
        assert!(methods.iter().any(|m| m.name == "new"), "Should find 'new' method");
        assert!(methods.iter().any(|m| m.name == "process"), "Should find 'process' method");

        // Methods should be parented to MyService
        let service = symbols.iter().find(|s| s.name == "MyService").unwrap();
        for method in &methods {
            assert_eq!(
                method.parent_id.as_ref().unwrap(),
                &service.id,
                "Method '{}' should be parented to MyService",
                method.name
            );
        }
    }

    #[test]
    fn test_impl_methods_extracted_with_scoped_type() {
        // impl super::Foo pattern — the struct is defined elsewhere, type is scoped.
        // This is the exact pattern that was broken: the extractor only looked for
        // "type_identifier" children but scoped paths produce "scoped_type_identifier".
        let code = r#"
impl super::BashExtractor {
    pub(super) fn extract_variable(&mut self, name: &str) -> Option<String> {
        Some(name.to_string())
    }

    fn is_environment_variable(&self, name: &str) -> bool {
        name.chars().all(|c| c.is_uppercase() || c == '_')
    }
}
"#;
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = RustExtractor::new(
            "rust".to_string(),
            "variables.rs".to_string(),
            code.to_string(),
            &test_workspace_root(),
        );

        let symbols = extractor.extract_symbols(&tree);
        let methods: Vec<_> = symbols.iter().filter(|s| s.kind == SymbolKind::Method).collect();

        assert_eq!(
            methods.len(),
            2,
            "Should extract 2 methods from impl super::BashExtractor block. Got symbols: {:?}",
            symbols.iter().map(|s| (&s.name, &s.kind)).collect::<Vec<_>>()
        );
        assert!(
            methods.iter().any(|m| m.name == "extract_variable"),
            "Should find 'extract_variable' method"
        );
        assert!(
            methods.iter().any(|m| m.name == "is_environment_variable"),
            "Should find 'is_environment_variable' method"
        );
    }

    #[test]
    fn test_impl_methods_extracted_with_crate_scoped_type() {
        // impl crate::some::Type pattern
        let code = r#"
impl crate::base::BaseExtractor {
    pub fn helper(&self) -> String {
        String::new()
    }
}
"#;
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = RustExtractor::new(
            "rust".to_string(),
            "helpers.rs".to_string(),
            code.to_string(),
            &test_workspace_root(),
        );

        let symbols = extractor.extract_symbols(&tree);
        let methods: Vec<_> = symbols.iter().filter(|s| s.kind == SymbolKind::Method).collect();

        assert_eq!(
            methods.len(),
            1,
            "Should extract method from impl crate::base::BaseExtractor. Got symbols: {:?}",
            symbols.iter().map(|s| (&s.name, &s.kind)).collect::<Vec<_>>()
        );
        assert_eq!(methods[0].name, "helper");
    }
}
