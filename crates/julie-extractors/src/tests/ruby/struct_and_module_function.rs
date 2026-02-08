/// Tests for Struct.new class detection and module_function handling
/// TDD: RED phase - these tests should fail until implementation is complete

#[cfg(test)]
mod tests {
    use crate::base::{SymbolKind, Visibility};
    use crate::tests::ruby::ruby_extractor_tests::create_extractor_and_parse;

    // ========================================================================
    // Struct.new Detection Tests
    // ========================================================================

    #[test]
    fn test_struct_new_simple() {
        let code = r#"Person = Struct.new(:name, :age, :email)"#;
        let (mut extractor, tree) = create_extractor_and_parse(code);
        let symbols = extractor.extract_symbols(&tree);

        let person = symbols
            .iter()
            .find(|s| s.name == "Person" && s.kind == SymbolKind::Class);
        assert!(
            person.is_some(),
            "Struct.new should create a Class symbol named Person, got: {:?}",
            symbols
                .iter()
                .filter(|s| s.name == "Person")
                .map(|s| (&s.name, &s.kind))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_struct_new_with_block() {
        let code = r#"
Point = Struct.new(:x, :y) do
  def distance
    Math.sqrt(x**2 + y**2)
  end
end
"#;
        let (mut extractor, tree) = create_extractor_and_parse(code);
        let symbols = extractor.extract_symbols(&tree);

        let point = symbols
            .iter()
            .find(|s| s.name == "Point" && s.kind == SymbolKind::Class);
        assert!(
            point.is_some(),
            "Struct.new with block should create a Class symbol"
        );

        // Methods defined in the do_block should be children of the struct class
        let distance = symbols.iter().find(|s| s.name == "distance");
        assert!(
            distance.is_some(),
            "Method in Struct.new block should be extracted"
        );
        assert_eq!(
            distance.unwrap().parent_id,
            Some(point.unwrap().id.clone()),
            "Method in do block should have the Struct class as parent"
        );
    }

    #[test]
    fn test_struct_new_signature() {
        let code = r#"Person = Struct.new(:name, :age, :email)"#;
        let (mut extractor, tree) = create_extractor_and_parse(code);
        let symbols = extractor.extract_symbols(&tree);

        let person = symbols
            .iter()
            .find(|s| s.name == "Person" && s.kind == SymbolKind::Class);
        assert!(person.is_some());
        let signature = person.unwrap().signature.as_ref().unwrap();
        assert!(
            signature.contains("Struct.new"),
            "Signature '{}' should contain 'Struct.new'",
            signature
        );
        assert!(
            signature.contains(":name"),
            "Signature '{}' should contain field names",
            signature
        );
    }

    #[test]
    fn test_struct_new_no_duplicate() {
        let code = r#"Person = Struct.new(:name, :age, :email)"#;
        let (mut extractor, tree) = create_extractor_and_parse(code);
        let symbols = extractor.extract_symbols(&tree);

        // There should be exactly one "Person" symbol (Class), not a Constant + Class
        let person_symbols: Vec<_> = symbols.iter().filter(|s| s.name == "Person").collect();
        assert_eq!(
            person_symbols.len(),
            1,
            "Should have exactly 1 Person symbol (no duplicate), found {} with kinds: {:?}",
            person_symbols.len(),
            person_symbols.iter().map(|s| &s.kind).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_struct_new_inside_module() {
        let code = r#"
module Models
  Person = Struct.new(:name, :age)
end
"#;
        let (mut extractor, tree) = create_extractor_and_parse(code);
        let symbols = extractor.extract_symbols(&tree);

        let person = symbols
            .iter()
            .find(|s| s.name == "Person" && s.kind == SymbolKind::Class);
        assert!(
            person.is_some(),
            "Struct.new inside module should create a Class symbol"
        );
    }

    #[test]
    fn test_struct_new_with_keyword_init() {
        // Struct.new with keyword_init is a common Ruby 2.5+ pattern
        let code = r#"Config = Struct.new(:host, :port, keyword_init: true)"#;
        let (mut extractor, tree) = create_extractor_and_parse(code);
        let symbols = extractor.extract_symbols(&tree);

        let config = symbols
            .iter()
            .find(|s| s.name == "Config" && s.kind == SymbolKind::Class);
        assert!(
            config.is_some(),
            "Struct.new with keyword_init should create a Class symbol"
        );
    }

    #[test]
    fn test_multiple_struct_new_definitions() {
        let code = r#"
Person = Struct.new(:name, :age)
Point = Struct.new(:x, :y)
Color = Struct.new(:r, :g, :b)
"#;
        let (mut extractor, tree) = create_extractor_and_parse(code);
        let symbols = extractor.extract_symbols(&tree);

        for name in &["Person", "Point", "Color"] {
            let sym = symbols
                .iter()
                .find(|s| s.name == *name && s.kind == SymbolKind::Class);
            assert!(
                sym.is_some(),
                "Struct.new should create Class for {}",
                name
            );
        }
    }

    // ========================================================================
    // Struct.new Field Properties Tests
    // ========================================================================

    #[test]
    fn test_struct_new_field_properties() {
        let code = r#"Person = Struct.new(:name, :age, :email)"#;
        let (mut extractor, tree) = create_extractor_and_parse(code);
        let symbols = extractor.extract_symbols(&tree);

        let person = symbols
            .iter()
            .find(|s| s.name == "Person" && s.kind == SymbolKind::Class)
            .unwrap();
        let props: Vec<_> = symbols
            .iter()
            .filter(|s| {
                s.kind == SymbolKind::Property && s.parent_id.as_deref() == Some(&person.id)
            })
            .collect();
        assert_eq!(
            props.len(),
            3,
            "Struct.new fields should be Property children"
        );
        let names: Vec<_> = props.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"name"));
        assert!(names.contains(&"age"));
        assert!(names.contains(&"email"));
    }

    // ========================================================================
    // module_function Tests
    // ========================================================================

    #[test]
    fn test_module_function_with_arg() {
        let code = r#"
module MyModule
  def helper
    "help"
  end
  module_function :helper
end
"#;
        let (mut extractor, tree) = create_extractor_and_parse(code);
        let symbols = extractor.extract_symbols(&tree);

        // helper method should be extracted
        let helper = symbols
            .iter()
            .find(|s| s.name == "helper" && s.kind == SymbolKind::Method);
        assert!(
            helper.is_some(),
            "helper method should be extracted when module_function is used"
        );
    }

    #[test]
    fn test_module_function_bare_visibility() {
        // The bare module_function form acts as a visibility modifier:
        // methods after it become module functions (effectively public)
        let code = r#"
module Utils
  private

  def private_helper
    "private"
  end

  module_function

  def format_name(name)
    name
  end
end
"#;
        let (mut extractor, tree) = create_extractor_and_parse(code);
        let symbols = extractor.extract_symbols(&tree);

        let private_helper = symbols.iter().find(|s| s.name == "private_helper");
        assert!(private_helper.is_some());
        assert_eq!(
            private_helper.unwrap().visibility.as_ref().unwrap(),
            &Visibility::Private,
            "Method before module_function should remain private"
        );

        let format_name = symbols.iter().find(|s| s.name == "format_name");
        assert!(format_name.is_some());
        assert_eq!(
            format_name.unwrap().visibility.as_ref().unwrap(),
            &Visibility::Public,
            "Method after bare module_function should be public"
        );
    }
}
