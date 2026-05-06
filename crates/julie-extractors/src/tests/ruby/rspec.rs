#[cfg(test)]
mod ruby_rspec_tests {
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
        let mut extractor = RubyExtractor::new(
            "spec/user_spec.rb".to_string(),
            code.to_string(),
            &workspace_root,
        );
        extractor.extract_symbols(&tree)
    }

    fn metadata_bool(symbol: &crate::base::Symbol, key: &str) -> bool {
        symbol
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get(key))
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
    }

    #[test]
    fn test_ruby_rspec_blocks_are_marked_as_tests() {
        let code = r#"
RSpec.describe User do
  context "when valid" do
    it "saves the record" do
      expect(subject.save).to be(true)
    end
  end

  specify "validates email" do
    expect(subject).to be_valid
  end
end
"#;

        let symbols = extract_symbols(code);
        let user_container = symbols
            .iter()
            .find(|symbol| symbol.name == "User")
            .expect("RSpec.describe User should create a test container symbol");
        assert_eq!(user_container.kind, SymbolKind::Namespace);
        assert!(metadata_bool(user_container, "test_container"));
        assert!(!metadata_bool(user_container, "is_test"));

        let context_container = symbols
            .iter()
            .find(|symbol| symbol.name == "when valid")
            .expect("context block should create a nested test container symbol");
        assert_eq!(
            context_container.parent_id.as_deref(),
            Some(user_container.id.as_str())
        );
        assert!(metadata_bool(context_container, "test_container"));

        let saves = symbols
            .iter()
            .find(|symbol| symbol.name == "saves the record")
            .expect("it block should create a test symbol");
        assert_eq!(saves.kind, SymbolKind::Function);
        assert_eq!(
            saves.parent_id.as_deref(),
            Some(context_container.id.as_str())
        );
        assert!(metadata_bool(saves, "is_test"));

        let validates = symbols
            .iter()
            .find(|symbol| symbol.name == "validates email")
            .expect("specify block should create a test symbol");
        assert_eq!(
            validates.parent_id.as_deref(),
            Some(user_container.id.as_str())
        );
        assert!(metadata_bool(validates, "is_test"));
    }
}
