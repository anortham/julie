//! Tests for cross-file relationship resolution (resolver.rs)
//!
//! Tests the disambiguation logic that resolves PendingRelationships
//! (callee name only) into full Relationships (with target symbol ID).

#[cfg(test)]
mod resolver_tests {
    use julie_extractors::base::{
        PendingRelationship, Relationship, RelationshipKind, Symbol, SymbolKind, Visibility,
    };
    use crate::tools::workspace::indexing::resolver::{
        select_best_candidate, build_resolved_relationship, ResolutionStats,
    };

    /// Helper to create a minimal Symbol for testing
    fn make_symbol(name: &str, kind: SymbolKind, language: &str, file_path: &str) -> Symbol {
        Symbol {
            id: format!("{}_{}", name, file_path.replace('/', "_")),
            name: name.to_string(),
            kind,
            language: language.to_string(),
            file_path: file_path.to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 10,
            end_column: 0,
            start_byte: 0,
            end_byte: 100,
            signature: None,
            doc_comment: None,
            visibility: Some(Visibility::Public),
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
            content_type: None,
        }
    }

    /// Helper to create a PendingRelationship
    fn make_pending(
        from_id: &str,
        callee_name: &str,
        file_path: &str,
    ) -> PendingRelationship {
        PendingRelationship {
            from_symbol_id: from_id.to_string(),
            callee_name: callee_name.to_string(),
            kind: RelationshipKind::Calls,
            file_path: file_path.to_string(),
            line_number: 42,
            confidence: 0.8,
        }
    }

    // =========================================================================
    // Kind filtering
    // =========================================================================

    #[test]
    fn test_excludes_import_symbols() {
        // An Import symbol should never be selected as a resolution target
        let candidates = vec![
            make_symbol("process", SymbolKind::Import, "rust", "src/lib.rs"),
        ];
        let pending = make_pending("caller_1", "process", "src/main.rs");

        let result = select_best_candidate(&candidates, &pending);
        assert!(result.is_none(), "Import symbols should not be valid resolution targets");
    }

    #[test]
    fn test_excludes_export_symbols() {
        let candidates = vec![
            make_symbol("process", SymbolKind::Export, "typescript", "src/index.ts"),
        ];
        let pending = make_pending("caller_1", "process", "src/main.ts");

        let result = select_best_candidate(&candidates, &pending);
        assert!(result.is_none(), "Export symbols should not be valid resolution targets");
    }

    #[test]
    fn test_prefers_function_over_import_when_both_exist() {
        let candidates = vec![
            make_symbol("process", SymbolKind::Import, "rust", "src/lib.rs"),
            make_symbol("process", SymbolKind::Function, "rust", "src/utils.rs"),
        ];
        let pending = make_pending("caller_1", "process", "src/main.rs");

        let result = select_best_candidate(&candidates, &pending);
        assert!(result.is_some());
        assert_eq!(result.unwrap().kind, SymbolKind::Function);
    }

    #[test]
    fn test_resolves_class_targets() {
        // When callee is a class (e.g., Instantiates relationship), should resolve
        let candidates = vec![
            make_symbol("UserService", SymbolKind::Class, "typescript", "src/services/user.ts"),
        ];
        let pending = PendingRelationship {
            from_symbol_id: "caller_1".to_string(),
            callee_name: "UserService".to_string(),
            kind: RelationshipKind::Instantiates,
            file_path: "src/main.ts".to_string(),
            line_number: 10,
            confidence: 0.8,
        };

        let result = select_best_candidate(&candidates, &pending);
        assert!(result.is_some(), "Class symbols should be valid resolution targets");
    }

    #[test]
    fn test_resolves_trait_targets() {
        let candidates = vec![
            make_symbol("Serialize", SymbolKind::Trait, "rust", "src/traits.rs"),
        ];
        let pending = PendingRelationship {
            from_symbol_id: "caller_1".to_string(),
            callee_name: "Serialize".to_string(),
            kind: RelationshipKind::Implements,
            file_path: "src/model.rs".to_string(),
            line_number: 5,
            confidence: 0.9,
        };

        let result = select_best_candidate(&candidates, &pending);
        assert!(result.is_some(), "Trait symbols should be valid resolution targets");
    }

    // =========================================================================
    // Language disambiguation
    // =========================================================================

    #[test]
    fn test_prefers_same_language() {
        // Caller is in a Rust file. Two candidates: one Rust, one Python.
        // Should pick the Rust one.
        let candidates = vec![
            make_symbol("process", SymbolKind::Function, "python", "lib/process.py"),
            make_symbol("process", SymbolKind::Function, "rust", "src/process.rs"),
        ];
        let pending = make_pending("caller_1", "process", "src/main.rs");

        let result = select_best_candidate(&candidates, &pending);
        assert!(result.is_some());
        assert_eq!(result.unwrap().language, "rust");
    }

    #[test]
    fn test_falls_back_to_different_language_when_no_same_lang() {
        // Caller is Rust but only Python candidate exists — should still resolve
        let candidates = vec![
            make_symbol("analyze", SymbolKind::Function, "python", "scripts/analyze.py"),
        ];
        let pending = make_pending("caller_1", "analyze", "src/main.rs");

        let result = select_best_candidate(&candidates, &pending);
        assert!(result.is_some(), "Should fall back to different-language candidate when same-language unavailable");
    }

    // =========================================================================
    // Path proximity
    // =========================================================================

    #[test]
    fn test_prefers_same_directory() {
        // Two candidates in same language, one in same dir, one far away
        let candidates = vec![
            make_symbol("helper", SymbolKind::Function, "rust", "src/utils/helper.rs"),
            make_symbol("helper", SymbolKind::Function, "rust", "src/services/helper.rs"),
        ];
        let pending = make_pending("caller_1", "helper", "src/utils/main.rs");

        let result = select_best_candidate(&candidates, &pending);
        assert!(result.is_some());
        assert_eq!(result.unwrap().file_path, "src/utils/helper.rs");
    }

    #[test]
    fn test_prefers_parent_directory_over_unrelated() {
        let candidates = vec![
            make_symbol("config", SymbolKind::Function, "rust", "src/config.rs"),
            make_symbol("config", SymbolKind::Function, "rust", "tests/config.rs"),
        ];
        // Caller is in src/tools/main.rs — src/config.rs is a parent dir, tests/ is unrelated
        let pending = make_pending("caller_1", "config", "src/tools/main.rs");

        let result = select_best_candidate(&candidates, &pending);
        assert!(result.is_some());
        assert_eq!(result.unwrap().file_path, "src/config.rs");
    }

    // =========================================================================
    // Calls-specific: prefer callable kinds
    // =========================================================================

    #[test]
    fn test_calls_prefers_function_over_class() {
        // For a Calls relationship, prefer Function over Class when both match
        let candidates = vec![
            make_symbol("Process", SymbolKind::Class, "rust", "src/process.rs"),
            make_symbol("Process", SymbolKind::Function, "rust", "src/process.rs"),
        ];
        let pending = make_pending("caller_1", "Process", "src/main.rs");

        let result = select_best_candidate(&candidates, &pending);
        assert!(result.is_some());
        assert_eq!(result.unwrap().kind, SymbolKind::Function);
    }

    // =========================================================================
    // Combined scoring
    // =========================================================================

    #[test]
    fn test_language_trumps_path_proximity() {
        // Same-language in distant dir should beat different-language in same dir
        let candidates = vec![
            make_symbol("parse", SymbolKind::Function, "python", "src/parse.py"),
            make_symbol("parse", SymbolKind::Function, "rust", "lib/parse.rs"),
        ];
        let pending = make_pending("caller_1", "parse", "src/main.rs");

        let result = select_best_candidate(&candidates, &pending);
        assert!(result.is_some());
        // Rust candidate should win even though Python is in the same directory
        assert_eq!(result.unwrap().language, "rust");
    }

    #[test]
    fn test_no_candidates_returns_none() {
        let candidates: Vec<Symbol> = vec![];
        let pending = make_pending("caller_1", "nonexistent", "src/main.rs");

        let result = select_best_candidate(&candidates, &pending);
        assert!(result.is_none());
    }

    #[test]
    fn test_only_invalid_candidates_returns_none() {
        let candidates = vec![
            make_symbol("foo", SymbolKind::Import, "rust", "src/lib.rs"),
            make_symbol("foo", SymbolKind::Export, "rust", "src/lib.rs"),
            make_symbol("foo", SymbolKind::Variable, "rust", "src/lib.rs"),
        ];
        let pending = make_pending("caller_1", "foo", "src/main.rs");

        let result = select_best_candidate(&candidates, &pending);
        assert!(result.is_none());
    }

    // =========================================================================
    // build_resolved_relationship
    // =========================================================================

    #[test]
    fn test_build_resolved_relationship() {
        let pending = make_pending("from_abc", "target_fn", "src/caller.rs");
        let target = make_symbol("target_fn", SymbolKind::Function, "rust", "src/target.rs");

        let resolved = build_resolved_relationship(&pending, &target);

        assert_eq!(resolved.from_symbol_id, "from_abc");
        assert_eq!(resolved.to_symbol_id, target.id);
        assert_eq!(resolved.kind, RelationshipKind::Calls);
        assert_eq!(resolved.file_path, "src/caller.rs");
        assert_eq!(resolved.line_number, 42);
        assert!(resolved.id.contains("resolved"));
    }
}
