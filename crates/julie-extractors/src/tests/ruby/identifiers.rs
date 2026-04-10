/// Tests for Ruby identifier extraction — type_usage for centrality scoring
///
/// Ruby has no static type annotations, but constants serve as type references:
/// - Superclass references: `class Foo < Bar`
/// - Module includes: `include Helpers`
/// - Scope resolution: `Sinatra::Base`
/// These must produce TypeUsage identifiers so the centrality pipeline can
/// boost well-connected classes/modules.
use crate::base::IdentifierKind;
use crate::ruby::RubyExtractor;
use std::path::PathBuf;

#[test]
fn test_ruby_type_usage_identifiers() {
    let code = r#"
class AppController < BaseController
  include Helpers
  extend ClassMethods
  prepend Logging
end

obj = Namespace::HelperClass.new
"#;

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_ruby::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor =
        RubyExtractor::new("test.rb".to_string(), code.to_string(), &workspace_root);
    let symbols = extractor.extract_symbols(&tree);
    let identifiers = extractor.extract_identifiers(&tree, &symbols);

    let type_usages: Vec<_> = identifiers
        .iter()
        .filter(|id| id.kind == IdentifierKind::TypeUsage)
        .collect();
    let type_names: Vec<&str> = type_usages.iter().map(|id| id.name.as_str()).collect();

    // Superclass reference MUST be TypeUsage
    assert!(
        type_names.contains(&"BaseController"),
        "Superclass reference 'BaseController' must produce TypeUsage. Got: {:?}",
        type_names
    );

    // include/extend/prepend arguments MUST be TypeUsage
    assert!(
        type_names.contains(&"Helpers"),
        "include arg 'Helpers' must produce TypeUsage. Got: {:?}",
        type_names
    );
    assert!(
        type_names.contains(&"ClassMethods"),
        "extend arg 'ClassMethods' must produce TypeUsage. Got: {:?}",
        type_names
    );
    assert!(
        type_names.contains(&"Logging"),
        "prepend arg 'Logging' must produce TypeUsage. Got: {:?}",
        type_names
    );

    // Scope resolution references MUST be TypeUsage
    assert!(
        type_names.contains(&"Namespace"),
        "Scope resolution namespace 'Namespace' must produce TypeUsage. Got: {:?}",
        type_names
    );
    assert!(
        type_names.contains(&"HelperClass"),
        "Scope resolution name 'HelperClass' must produce TypeUsage. Got: {:?}",
        type_names
    );

    // Class/module declaration names must NOT be TypeUsage
    assert!(
        !type_names.contains(&"AppController"),
        "Class declaration name 'AppController' must NOT be TypeUsage. Got: {:?}",
        type_names
    );
}
