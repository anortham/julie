/// Tests for Dart type extraction through the factory

#[cfg(test)]
mod tests {
    use crate::extractors::factory::extract_symbols_and_relationships;
    use std::path::PathBuf;
    use tree_sitter::Parser;

    #[test]
    fn test_factory_extracts_dart_types() {
        let code = r#"
class UserService {
  String getUserName(int userId) {
    return 'User$userId';
  }

  Future<List<User>> getAllUsers() async {
    return await repository.findAll();
  }

  Map<String, int> getUserScores() {
    return {};
  }
}
"#;

        let mut parser = Parser::new();
        parser
            .set_language(&harper_tree_sitter_dart::LANGUAGE.into())
            .expect("Error loading Dart grammar");
        let tree = parser.parse(code, None).expect("Error parsing code");

        let workspace_root = PathBuf::from("/tmp/test");
        let results = extract_symbols_and_relationships(
            &tree,
            "test.dart",
            code,
            "dart",
            &workspace_root,
        )
        .expect("Extraction failed");

        assert!(
            !results.types.is_empty(),
            "Dart type extraction returned EMPTY types HashMap!"
        );

        println!("Extracted {} types from Dart code", results.types.len());
        for (symbol_id, type_info) in &results.types {
            println!("  {} -> {} (inferred: {})", symbol_id, type_info.resolved_type, type_info.is_inferred);
        }

        assert!(results.types.len() >= 1);
        for type_info in results.types.values() {
            assert_eq!(type_info.language, "dart");
            assert!(type_info.is_inferred);
        }
    }
}
