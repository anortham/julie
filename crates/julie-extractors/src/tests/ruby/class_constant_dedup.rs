/// Tests for Ruby class/constant deduplication
///
/// When Ruby's tree-sitter parser encounters `class Base < Object`, it produces
/// both a `class` node AND nested `constant` nodes for the class name and superclass.
/// The extractor should NOT create separate Constant symbols for names that are
/// already represented by Class or Module symbols.
///
/// This prevents centrality score splitting where a redundant Constant symbol
/// absorbs references that should go to the Class symbol.

#[cfg(test)]
mod tests {
    use crate::base::SymbolKind;
    use crate::tests::ruby::ruby_extractor_tests::create_extractor_and_parse;

    #[test]
    fn test_class_name_not_duplicated_as_constant() {
        let ruby_code = r#"
class Base
  def initialize
  end
end

class Child < Base
end
"#;

        let (mut extractor, tree) = create_extractor_and_parse(ruby_code);
        let symbols = extractor.extract_symbols(&tree);

        // "Base" should appear as a Class symbol
        let base_classes: Vec<_> = symbols
            .iter()
            .filter(|s| s.name == "Base" && s.kind == SymbolKind::Class)
            .collect();
        assert_eq!(
            base_classes.len(),
            1,
            "Expected exactly 1 Class symbol named 'Base', got {}",
            base_classes.len()
        );

        // "Base" should NOT also appear as a Constant symbol (that's the bug)
        let base_constants: Vec<_> = symbols
            .iter()
            .filter(|s| s.name == "Base" && s.kind == SymbolKind::Constant)
            .collect();
        assert_eq!(
            base_constants.len(),
            0,
            "Expected 0 Constant symbols named 'Base' (class name should not be duplicated), got {}",
            base_constants.len()
        );

        // "Child" should appear as a Class symbol
        let child_classes: Vec<_> = symbols
            .iter()
            .filter(|s| s.name == "Child" && s.kind == SymbolKind::Class)
            .collect();
        assert_eq!(
            child_classes.len(),
            1,
            "Expected exactly 1 Class symbol named 'Child', got {}",
            child_classes.len()
        );

        // "Child" should NOT also appear as a Constant
        let child_constants: Vec<_> = symbols
            .iter()
            .filter(|s| s.name == "Child" && s.kind == SymbolKind::Constant)
            .collect();
        assert_eq!(
            child_constants.len(),
            0,
            "Expected 0 Constant symbols named 'Child', got {}",
            child_constants.len()
        );
    }

    #[test]
    fn test_module_name_not_duplicated_as_constant() {
        let ruby_code = r#"
module Utilities
  def self.helper
  end
end
"#;

        let (mut extractor, tree) = create_extractor_and_parse(ruby_code);
        let symbols = extractor.extract_symbols(&tree);

        // "Utilities" should appear as a Module symbol
        let module_syms: Vec<_> = symbols
            .iter()
            .filter(|s| s.name == "Utilities" && s.kind == SymbolKind::Module)
            .collect();
        assert_eq!(module_syms.len(), 1, "Expected 1 Module 'Utilities'");

        // "Utilities" should NOT appear as a Constant
        let const_syms: Vec<_> = symbols
            .iter()
            .filter(|s| s.name == "Utilities" && s.kind == SymbolKind::Constant)
            .collect();
        assert_eq!(
            const_syms.len(),
            0,
            "Module name should not create a duplicate Constant symbol"
        );
    }

    #[test]
    fn test_superclass_constant_not_duplicated() {
        // The superclass name in `class Child < Base` is also a `constant` node
        // that should be skipped (it's a reference, not a definition)
        let ruby_code = r#"
class Base
end

class Child < Base
end
"#;

        let (mut extractor, tree) = create_extractor_and_parse(ruby_code);
        let symbols = extractor.extract_symbols(&tree);

        // The superclass reference "Base" inside `class Child < Base` should not
        // create a second Constant symbol. Only the Class symbol for Base should exist.
        let base_syms: Vec<_> = symbols.iter().filter(|s| s.name == "Base").collect();

        // Should only be the Class definition, no extra Constant from superclass ref
        let base_class_count = base_syms
            .iter()
            .filter(|s| s.kind == SymbolKind::Class)
            .count();
        let base_const_count = base_syms
            .iter()
            .filter(|s| s.kind == SymbolKind::Constant)
            .count();

        assert_eq!(base_class_count, 1, "Expected 1 Class 'Base'");
        assert_eq!(
            base_const_count, 0,
            "Expected 0 Constant 'Base' (superclass ref should not create constant symbol)"
        );
    }

    #[test]
    fn test_real_constants_still_extracted() {
        // Non-class constants (actual constant definitions) must still be extracted
        let ruby_code = r#"
class Config
  MAX_SIZE = 100
  DEFAULT_NAME = "unnamed"
end

VERSION = "1.0.0"
"#;

        let (mut extractor, tree) = create_extractor_and_parse(ruby_code);
        let symbols = extractor.extract_symbols(&tree);

        // Config should be a Class, not duplicated as Constant
        let config_classes: Vec<_> = symbols
            .iter()
            .filter(|s| s.name == "Config" && s.kind == SymbolKind::Class)
            .collect();
        assert_eq!(config_classes.len(), 1, "Expected 1 Class 'Config'");

        let config_constants: Vec<_> = symbols
            .iter()
            .filter(|s| s.name == "Config" && s.kind == SymbolKind::Constant)
            .collect();
        assert_eq!(
            config_constants.len(),
            0,
            "Config class name should not also be a Constant"
        );

        // MAX_SIZE and DEFAULT_NAME are assignment-based constants — they go through
        // the assignment handler, not the constant handler. Verify they exist.
        let max_size = symbols.iter().find(|s| s.name == "MAX_SIZE");
        assert!(max_size.is_some(), "MAX_SIZE constant should be extracted");

        let default_name = symbols.iter().find(|s| s.name == "DEFAULT_NAME");
        assert!(
            default_name.is_some(),
            "DEFAULT_NAME constant should be extracted"
        );

        // VERSION is a top-level constant assignment
        let version = symbols.iter().find(|s| s.name == "VERSION");
        assert!(version.is_some(), "VERSION constant should be extracted");
    }

    #[test]
    fn test_nested_class_in_module_no_constant_duplication() {
        let ruby_code = r#"
module Sinatra
  class Base
    def call(env)
    end
  end

  class Application < Base
  end
end
"#;

        let (mut extractor, tree) = create_extractor_and_parse(ruby_code);
        let symbols = extractor.extract_symbols(&tree);

        // Sinatra should be Module only, not also Constant
        let sinatra_modules: Vec<_> = symbols
            .iter()
            .filter(|s| s.name == "Sinatra" && s.kind == SymbolKind::Module)
            .collect();
        assert_eq!(sinatra_modules.len(), 1);

        let sinatra_constants: Vec<_> = symbols
            .iter()
            .filter(|s| s.name == "Sinatra" && s.kind == SymbolKind::Constant)
            .collect();
        assert_eq!(
            sinatra_constants.len(),
            0,
            "Module name 'Sinatra' should not be duplicated as Constant"
        );

        // Base should be Class only, not also Constant
        let base_classes: Vec<_> = symbols
            .iter()
            .filter(|s| s.name == "Base" && s.kind == SymbolKind::Class)
            .collect();
        assert_eq!(base_classes.len(), 1);

        let base_constants: Vec<_> = symbols
            .iter()
            .filter(|s| s.name == "Base" && s.kind == SymbolKind::Constant)
            .collect();
        assert_eq!(
            base_constants.len(),
            0,
            "Class name 'Base' should not be duplicated as Constant"
        );

        // Application should be Class only
        let app_classes: Vec<_> = symbols
            .iter()
            .filter(|s| s.name == "Application" && s.kind == SymbolKind::Class)
            .collect();
        assert_eq!(app_classes.len(), 1);

        let app_constants: Vec<_> = symbols
            .iter()
            .filter(|s| s.name == "Application" && s.kind == SymbolKind::Constant)
            .collect();
        assert_eq!(
            app_constants.len(),
            0,
            "Class name 'Application' should not be duplicated as Constant"
        );
    }

    #[test]
    fn test_constant_references_not_extracted_as_symbols() {
        let code = r#"
module Sinatra
  class Base
    def call(env)
    end
  end

  class Application < Base
    include Helpers
  end

  def self.register(klass)
    Sinatra::Base.register(klass)
  end
end
"#;

        let (mut extractor, tree) = create_extractor_and_parse(code);
        let symbols = extractor.extract_symbols(&tree);

        let constant_symbols: Vec<_> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Constant)
            .map(|s| s.name.clone())
            .collect();

        assert!(
            constant_symbols.is_empty(),
            "Expected no Constant symbols (all constants are references), but found: {:?}",
            constant_symbols
        );

        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Sinatra" && s.kind == SymbolKind::Module),
            "Module Sinatra should exist"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Base" && s.kind == SymbolKind::Class),
            "Class Base should exist"
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Application" && s.kind == SymbolKind::Class),
            "Class Application should exist"
        );
    }

    #[test]
    fn test_real_constant_definitions_still_extracted() {
        let code = r#"
module Config
  MAX_SIZE = 100
  VERSION = "1.0"
  DEFAULT_OPTIONS = { timeout: 30 }
end
"#;

        let (mut extractor, tree) = create_extractor_and_parse(code);
        let symbols = extractor.extract_symbols(&tree);

        let constant_names: Vec<_> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Constant)
            .map(|s| s.name.clone())
            .collect();

        assert!(
            constant_names.contains(&"MAX_SIZE".to_string()),
            "MAX_SIZE should be extracted as a Constant, got: {:?}",
            constant_names
        );
        assert!(
            constant_names.contains(&"VERSION".to_string()),
            "VERSION should be extracted as a Constant, got: {:?}",
            constant_names
        );
        assert!(
            constant_names.contains(&"DEFAULT_OPTIONS".to_string()),
            "DEFAULT_OPTIONS should be extracted as a Constant, got: {:?}",
            constant_names
        );
    }
}
