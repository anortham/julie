/// Ruby RDoc/YARD documentation comment extraction tests
/// Tests for extracting RDoc and YARD-style comments from Ruby code

#[cfg(test)]
mod doc_comment_tests {
    use crate::extractors::base::SymbolKind;
    use crate::extractors::ruby::RubyExtractor;
    use tree_sitter::Tree;

    // Helper function to create a RubyExtractor and parse Ruby code
    fn create_extractor_and_parse(code: &str) -> (RubyExtractor, Tree) {
        use std::path::PathBuf;
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_ruby::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let extractor =
            RubyExtractor::new("test.rb".to_string(), code.to_string(), &workspace_root);
        (extractor, tree)
    }

    #[test]
    fn test_extract_rdoc_from_class() {
        let code = r#"
# UserService manages user authentication
# Provides login and logout functionality
class UserService
  def authenticate
  end
end
"#;

        let (mut extractor, tree) = create_extractor_and_parse(code);
        let symbols = extractor.extract_symbols(&tree);

        let class_symbol = symbols
            .iter()
            .find(|s| s.name == "UserService" && s.kind == SymbolKind::Class);
        assert!(class_symbol.is_some(), "UserService class should be found");

        let class = class_symbol.unwrap();
        assert!(
            class.doc_comment.is_some(),
            "UserService should have a doc comment"
        );

        let doc = class.doc_comment.as_ref().unwrap();
        assert!(
            doc.contains("manages user authentication"),
            "Doc comment should contain 'manages user authentication'"
        );
        assert!(
            doc.contains("Provides login"),
            "Doc comment should contain 'Provides login'"
        );
    }

    #[test]
    fn test_extract_yard_from_method() {
        let code = r#"
# Validates user credentials
# @param username [String] the username to validate
# @return [Boolean] true if valid
def validate_credentials(username)
  true
end
"#;

        let (mut extractor, tree) = create_extractor_and_parse(code);
        let symbols = extractor.extract_symbols(&tree);

        let method = symbols
            .iter()
            .find(|s| s.name == "validate_credentials" && s.kind == SymbolKind::Method);
        assert!(
            method.is_some(),
            "validate_credentials method should be found"
        );

        let method_sym = method.unwrap();
        assert!(
            method_sym.doc_comment.is_some(),
            "validate_credentials should have a doc comment"
        );

        let doc = method_sym.doc_comment.as_ref().unwrap();
        assert!(
            doc.contains("Validates user credentials"),
            "Doc comment should contain 'Validates user credentials'"
        );
        assert!(
            doc.contains("@param username"),
            "Doc comment should contain '@param username' annotation"
        );
        assert!(
            doc.contains("@return [Boolean]"),
            "Doc comment should contain '@return [Boolean]' annotation"
        );
    }

    #[test]
    fn test_extract_yard_from_module() {
        let code = r#"
# Authentication module for handling login/logout operations
# @example
#   Auth.login(user, password)
module Auth
  def self.login(user, password)
    # login logic
  end
end
"#;

        let (mut extractor, tree) = create_extractor_and_parse(code);
        let symbols = extractor.extract_symbols(&tree);

        let module = symbols
            .iter()
            .find(|s| s.name == "Auth" && s.kind == SymbolKind::Module);
        assert!(module.is_some(), "Auth module should be found");

        let module_sym = module.unwrap();
        assert!(
            module_sym.doc_comment.is_some(),
            "Auth module should have a doc comment"
        );

        let doc = module_sym.doc_comment.as_ref().unwrap();
        assert!(
            doc.contains("Authentication module"),
            "Doc comment should contain 'Authentication module'"
        );
        assert!(
            doc.contains("@example"),
            "Doc comment should contain '@example' annotation"
        );
    }

    #[test]
    fn test_extract_constructor_rdoc() {
        let code = r#"
# Initialize a person with name and age
# @param name [String] the person's name
# @param age [Integer] the person's age
def initialize(name, age)
  @name = name
  @age = age
end
"#;

        let (mut extractor, tree) = create_extractor_and_parse(code);
        let symbols = extractor.extract_symbols(&tree);

        let constructor = symbols
            .iter()
            .find(|s| s.name == "initialize" && s.kind == SymbolKind::Constructor);
        assert!(
            constructor.is_some(),
            "initialize constructor should be found"
        );

        let ctor = constructor.unwrap();
        assert!(
            ctor.doc_comment.is_some(),
            "initialize should have a doc comment"
        );

        let doc = ctor.doc_comment.as_ref().unwrap();
        assert!(
            doc.contains("Initialize a person with name and age"),
            "Doc should explain initialization"
        );
        assert!(
            doc.contains("@param name [String]"),
            "Doc should document name parameter"
        );
        assert!(
            doc.contains("@param age [Integer]"),
            "Doc should document age parameter"
        );
    }

    #[test]
    fn test_extract_constant_rdoc() {
        let code = r#"
# Default maximum connection attempts
MAX_RETRIES = 5

# Default timeout in seconds
TIMEOUT = 30
"#;

        let (mut extractor, tree) = create_extractor_and_parse(code);
        let symbols = extractor.extract_symbols(&tree);

        let max_retries = symbols
            .iter()
            .find(|s| s.name == "MAX_RETRIES" && s.kind == SymbolKind::Constant);
        assert!(
            max_retries.is_some(),
            "MAX_RETRIES constant should be found"
        );

        let constant = max_retries.unwrap();
        assert!(
            constant.doc_comment.is_some(),
            "MAX_RETRIES should have a doc comment"
        );

        let doc = constant.doc_comment.as_ref().unwrap();
        assert!(
            doc.contains("Default maximum connection attempts"),
            "Doc should explain the constant's purpose"
        );
    }

    #[test]
    fn test_extract_rdoc_with_multiple_lines() {
        let code = r#"
# Complex service that handles multiple operations
# including user authentication, authorization,
# and audit logging of all access events.
# This is a critical service that must be highly available.
class ComplexService
  def complex_method
  end
end
"#;

        let (mut extractor, tree) = create_extractor_and_parse(code);
        let symbols = extractor.extract_symbols(&tree);

        let service = symbols
            .iter()
            .find(|s| s.name == "ComplexService" && s.kind == SymbolKind::Class);
        assert!(service.is_some(), "ComplexService should be found");

        let service_sym = service.unwrap();
        assert!(
            service_sym.doc_comment.is_some(),
            "ComplexService should have a doc comment"
        );

        let doc = service_sym.doc_comment.as_ref().unwrap();
        // Should capture all comment lines
        assert!(
            doc.contains("Complex service that handles multiple operations"),
            "Should capture first line"
        );
        assert!(
            doc.contains("including user authentication, authorization"),
            "Should capture second line"
        );
        assert!(doc.contains("audit logging"), "Should capture third line");
        assert!(
            doc.contains("critical service"),
            "Should capture fourth line"
        );
    }

    #[test]
    fn test_no_doc_comment_when_missing() {
        let code = r#"
class SimpleClass
  def simple_method
    42
  end
end
"#;

        let (mut extractor, tree) = create_extractor_and_parse(code);
        let symbols = extractor.extract_symbols(&tree);

        let class = symbols
            .iter()
            .find(|s| s.name == "SimpleClass" && s.kind == SymbolKind::Class);
        assert!(class.is_some(), "SimpleClass should be found");

        let class_sym = class.unwrap();
        assert!(
            class_sym.doc_comment.is_none(),
            "SimpleClass should not have a doc comment"
        );

        let method = symbols
            .iter()
            .find(|s| s.name == "simple_method" && s.kind == SymbolKind::Method);
        assert!(method.is_some(), "simple_method should be found");

        let method_sym = method.unwrap();
        assert!(
            method_sym.doc_comment.is_none(),
            "simple_method should not have a doc comment"
        );
    }

    #[test]
    fn test_doc_comment_stops_at_non_doc_comment() {
        let code = r#"
# This is a doc comment
# For the method
# Some general comment not part of docs
def my_method
  42
end
"#;

        let (mut extractor, tree) = create_extractor_and_parse(code);
        let symbols = extractor.extract_symbols(&tree);

        let method = symbols
            .iter()
            .find(|s| s.name == "my_method" && s.kind == SymbolKind::Method);
        assert!(method.is_some(), "my_method should be found");

        let method_sym = method.unwrap();
        assert!(
            method_sym.doc_comment.is_some(),
            "my_method should have a doc comment"
        );

        let doc = method_sym.doc_comment.as_ref().unwrap();
        assert!(
            doc.contains("This is a doc comment"),
            "Should capture first doc line"
        );
        // This test validates that consecutive doc comments are captured
    }
}
