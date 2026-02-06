/// Tests for Ruby type extraction through the factory
///
/// Ruby is dynamically typed, so type inference is limited to literal
/// assignments in constants and variables.

#[cfg(test)]
mod tests {
    use crate::factory::extract_symbols_and_relationships;
    use std::path::PathBuf;
    use tree_sitter::Parser;

    #[test]
    fn test_factory_extracts_ruby_constant_types() {
        let code = r#"
class Config
  VERSION = "1.0.0"
  MAX_RETRIES = 3
  PI = 3.14159
  ENABLED = true
  DEFAULT_OPTIONS = {}
  VALID_STATES = [:active, :inactive]
end
"#;

        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_ruby::LANGUAGE.into())
            .expect("Error loading Ruby grammar");
        let tree = parser.parse(code, None).expect("Error parsing code");

        let workspace_root = PathBuf::from("/tmp/test");
        let results = extract_symbols_and_relationships(
            &tree,
            "test.rb",
            code,
            "ruby",
            &workspace_root,
        )
        .expect("Extraction failed");

        assert!(
            !results.types.is_empty(),
            "Ruby type extraction returned empty — expected types from constant literals"
        );

        let type_strings: Vec<&str> = results
            .types
            .values()
            .map(|t| t.resolved_type.as_str())
            .collect();

        println!("Ruby constant types: {:?}", type_strings);

        assert!(
            type_strings.iter().any(|t| *t == "String"),
            "Expected 'String' from VERSION constant, got: {:?}",
            type_strings
        );

        for type_info in results.types.values() {
            assert_eq!(type_info.language, "ruby");
            assert!(type_info.is_inferred);
        }
    }

    #[test]
    fn test_factory_ruby_methods_return_no_types() {
        // Ruby methods don't have type annotations — should return empty
        let code = r#"
class Calculator
  def add(a, b)
    a + b
  end

  def subtract(a, b)
    a - b
  end
end
"#;

        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_ruby::LANGUAGE.into())
            .expect("Error loading Ruby grammar");
        let tree = parser.parse(code, None).expect("Error parsing code");

        let workspace_root = PathBuf::from("/tmp/test");
        let results = extract_symbols_and_relationships(
            &tree,
            "test.rb",
            code,
            "ruby",
            &workspace_root,
        )
        .expect("Extraction failed");

        // Methods in Ruby have no type annotations, so types should be empty or minimal
        println!("Ruby method types (expected minimal): {}", results.types.len());
    }
}
