/// Tests for Dart type extraction through the factory

#[cfg(test)]
mod tests {
    use crate::base::SymbolKind;
    use crate::dart::DartExtractor;
    use crate::factory::extract_symbols_and_relationships;
    use serde_json::Value;
    use std::collections::HashMap;
    use std::collections::HashSet;
    use std::path::Path;
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
            .set_language(&tree_sitter_dart::LANGUAGE.into())
            .expect("Error loading Dart grammar");
        let tree = parser.parse(code, None).expect("Error parsing code");

        let workspace_root = PathBuf::from("/tmp/test");
        let results =
            extract_symbols_and_relationships(&tree, "test.dart", code, "dart", &workspace_root)
                .expect("Extraction failed");

        assert!(
            !results.types.is_empty(),
            "Dart type extraction returned EMPTY types HashMap!"
        );

        println!("Extracted {} types from Dart code", results.types.len());
        for (symbol_id, type_info) in &results.types {
            println!(
                "  {} -> {} (inferred: {})",
                symbol_id, type_info.resolved_type, type_info.is_inferred
            );
        }

        assert!(results.types.len() >= 1);
        for type_info in results.types.values() {
            assert_eq!(type_info.language, "dart");
            assert!(type_info.is_inferred);
        }
    }

    #[test]
    fn test_factory_dart_type_keys_are_symbol_ids() {
        let code = r#"
class UserService {
  final int maxUsers = 10;
  static const String appName = 'julie';

  String getUserName(int userId) {
    return 'User$userId';
  }
}
"#;

        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_dart::LANGUAGE.into())
            .expect("Error loading Dart grammar");
        let tree = parser.parse(code, None).expect("Error parsing code");

        let workspace_root = PathBuf::from("/tmp/test");
        let results =
            extract_symbols_and_relationships(&tree, "types.dart", code, "dart", &workspace_root)
                .expect("Extraction failed");

        let symbol_ids: HashSet<&str> = results
            .symbols
            .iter()
            .map(|symbol| symbol.id.as_str())
            .collect();
        assert!(
            !results.types.is_empty(),
            "Expected inferred Dart types for fixture, got empty map"
        );
        for type_key in results.types.keys() {
            assert!(
                symbol_ids.contains(type_key.as_str()),
                "Type key '{}' is not a real symbol id",
                type_key
            );
        }

        let mut final_metadata = HashMap::new();
        final_metadata.insert("isFinal".to_string(), Value::Bool(true));
        let mut const_metadata = HashMap::new();
        const_metadata.insert("isConst".to_string(), Value::Bool(true));

        let final_symbol = crate::base::Symbol {
            id: "dart-final-id".to_string(),
            name: "status".to_string(),
            kind: SymbolKind::Variable,
            language: "dart".to_string(),
            file_path: "types.dart".to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 1,
            end_column: 6,
            start_byte: 0,
            end_byte: 6,
            signature: None,
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: Some(final_metadata),
            annotations: Vec::new(),
            semantic_group: None,
            confidence: None,
            code_context: None,
            content_type: None,
        };
        let const_symbol = crate::base::Symbol {
            id: "dart-const-id".to_string(),
            name: "status".to_string(),
            kind: SymbolKind::Constant,
            language: "dart".to_string(),
            file_path: "types.dart".to_string(),
            start_line: 2,
            start_column: 0,
            end_line: 2,
            end_column: 6,
            start_byte: 7,
            end_byte: 13,
            signature: None,
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: Some(const_metadata),
            annotations: Vec::new(),
            semantic_group: None,
            confidence: None,
            code_context: None,
            content_type: None,
        };

        let extractor = DartExtractor::new(
            "dart".to_string(),
            "types.dart".to_string(),
            String::new(),
            Path::new("/tmp/test"),
        );
        let inferred_types = extractor.infer_types(&[final_symbol, const_symbol]);

        assert!(
            inferred_types.contains_key("dart-final-id"),
            "Missing final type keyed by symbol id"
        );
        assert!(
            inferred_types.contains_key("dart-const-id"),
            "Missing const type keyed by symbol id"
        );
        assert_eq!(
            inferred_types.get("dart-final-id").map(String::as_str),
            Some("final")
        );
        assert_eq!(
            inferred_types.get("dart-const-id").map(String::as_str),
            Some("const")
        );
    }
}
