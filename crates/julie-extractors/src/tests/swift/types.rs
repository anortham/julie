/// Tests for Swift type extraction through the factory

#[cfg(test)]
mod tests {
    use crate::base::SymbolKind;
    use crate::factory::extract_symbols_and_relationships;
    use crate::swift::SwiftExtractor;
    use std::path::PathBuf;
    use tree_sitter::Parser;

    fn parse_swift(code: &str) -> (SwiftExtractor, tree_sitter::Tree) {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_swift::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(code, None).unwrap();
        let extractor = SwiftExtractor::new(
            "swift".to_string(),
            "test.swift".to_string(),
            code.to_string(),
            &PathBuf::from("/tmp/test"),
        );
        (extractor, tree)
    }

    /// Regression test: open class Session: @unchecked Sendable was absent from symbol table.
    /// The class body methods were extracted as orphaned top-level functions.
    /// Also guards against @unchecked leaking into class modifiers (wrong signature).
    #[test]
    fn test_open_class_session_unchecked_sendable() {
        let code = r#"open class Session: @unchecked Sendable {
    func request(_ url: String) {}
    func download(_ url: String) {}
}"#;

        let (mut extractor, tree) = parse_swift(code);
        let symbols = extractor.extract_symbols(&tree);

        let session = symbols.iter().find(|s| s.name == "Session");
        assert!(
            session.is_some(),
            "Session class was not extracted — methods became orphaned top-level functions"
        );

        let session = session.unwrap();
        assert_eq!(
            session.kind,
            SymbolKind::Class,
            "Session should be SymbolKind::Class"
        );

        let sig = session.signature.as_deref().unwrap_or("");
        // @unchecked must NOT be pulled into class modifiers; it belongs to the inheritance clause
        assert!(
            sig.contains("open class Session"),
            "Signature should have 'open class Session' (not 'open @unchecked class Session'), got: {:?}",
            sig
        );
        assert!(
            sig.contains("@unchecked Sendable"),
            "Signature should preserve '@unchecked Sendable' in inheritance, got: {:?}",
            sig
        );

        // Methods should be children of Session, not orphans
        let request = symbols.iter().find(|s| s.name == "request");
        assert!(request.is_some(), "request method should be extracted");
        assert_eq!(
            request.unwrap().parent_id.as_deref(),
            Some(session.id.as_str()),
            "request should be a child of Session"
        );
    }

    #[test]
    fn test_factory_extracts_swift_types() {
        let code = r#"
class UserService {
    func getUserName(userId: Int) -> String {
        return "User\(userId)"
    }

    func getAllUsers() -> [User] {
        return repository.findAll()
    }

    func getUserScores() -> [String: Int] {
        return [:]
    }
}
"#;

        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_swift::LANGUAGE.into())
            .expect("Error loading Swift grammar");
        let tree = parser.parse(code, None).expect("Error parsing code");

        let workspace_root = PathBuf::from("/tmp/test");
        let results =
            extract_symbols_and_relationships(&tree, "test.swift", code, "swift", &workspace_root)
                .expect("Extraction failed");

        assert!(
            !results.types.is_empty(),
            "Swift type extraction returned EMPTY types HashMap!"
        );

        println!("Extracted {} types from Swift code", results.types.len());
        for (symbol_id, type_info) in &results.types {
            println!(
                "  {} -> {} (inferred: {})",
                symbol_id, type_info.resolved_type, type_info.is_inferred
            );
        }

        assert!(results.types.len() >= 1);
        for type_info in results.types.values() {
            assert_eq!(type_info.language, "swift");
            assert!(type_info.is_inferred);
        }
    }
}
