#[cfg(test)]
mod ruby_property_tests {
    use crate::base::SymbolKind;
    use crate::ruby::RubyExtractor;
    use std::path::PathBuf;

    fn extract_symbols(code: &str) -> Vec<crate::base::Symbol> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_ruby::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor =
            RubyExtractor::new("test.rb".to_string(), code.to_string(), &workspace_root);
        extractor.extract_symbols(&tree)
    }

    #[test]
    fn test_ruby_attr_accessor_emits_all_property_symbols_with_class_parent() {
        let code = r#"
class User
  attr_accessor :name, :email, :timezone
  attr_reader :id, :status
  attr_writer :password, :token
end
"#;

        let symbols = extract_symbols(code);
        let user = symbols
            .iter()
            .find(|symbol| symbol.name == "User" && symbol.kind == SymbolKind::Class)
            .expect("User class should be extracted");

        let expected = [
            ("name", "attr_accessor"),
            ("email", "attr_accessor"),
            ("timezone", "attr_accessor"),
            ("id", "attr_reader"),
            ("status", "attr_reader"),
            ("password", "attr_writer"),
            ("token", "attr_writer"),
        ];

        for (name, accessor) in expected {
            let property = symbols
                .iter()
                .find(|symbol| symbol.name == name && symbol.kind == SymbolKind::Property)
                .unwrap_or_else(|| panic!("missing property symbol for {name}"));

            assert_eq!(
                property.parent_id.as_deref(),
                Some(user.id.as_str()),
                "property {name} should keep the class parent"
            );
            assert_eq!(
                property.signature.as_deref(),
                Some(format!("{accessor} :{name}").as_str())
            );
        }

        let property_count = symbols
            .iter()
            .filter(|symbol| symbol.kind == SymbolKind::Property)
            .count();
        assert_eq!(property_count, expected.len());
    }
}
