//! Tests for cross-file relationship resolution (resolver.rs)
//!
//! Tests the disambiguation logic that resolves PendingRelationships
//! (callee name only) into full Relationships (with target symbol ID).

#[cfg(test)]
mod resolver_tests {
    use crate::tools::workspace::indexing::resolver::{
        ParentReferenceContext, build_resolved_relationship,
        select_best_candidate,
    };
    use julie_extractors::base::{
        PendingRelationship, RelationshipKind, Symbol, SymbolKind, Visibility,
    };
    use std::collections::{HashMap, HashSet};

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
    fn make_pending(from_id: &str, callee_name: &str, file_path: &str) -> PendingRelationship {
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
        let candidates = vec![make_symbol(
            "process",
            SymbolKind::Import,
            "rust",
            "src/lib.rs",
        )];
        let pending = make_pending("caller_1", "process", "src/main.rs");

        let result = select_best_candidate(&candidates, &pending, &ParentReferenceContext::empty());
        assert!(
            result.is_none(),
            "Import symbols should not be valid resolution targets"
        );
    }

    #[test]
    fn test_excludes_export_symbols() {
        let candidates = vec![make_symbol(
            "process",
            SymbolKind::Export,
            "typescript",
            "src/index.ts",
        )];
        let pending = make_pending("caller_1", "process", "src/main.ts");

        let result = select_best_candidate(&candidates, &pending, &ParentReferenceContext::empty());
        assert!(
            result.is_none(),
            "Export symbols should not be valid resolution targets"
        );
    }

    #[test]
    fn test_prefers_function_over_import_when_both_exist() {
        let candidates = vec![
            make_symbol("process", SymbolKind::Import, "rust", "src/lib.rs"),
            make_symbol("process", SymbolKind::Function, "rust", "src/utils.rs"),
        ];
        let pending = make_pending("caller_1", "process", "src/main.rs");

        let result = select_best_candidate(&candidates, &pending, &ParentReferenceContext::empty());
        assert!(result.is_some());
        assert_eq!(result.unwrap().kind, SymbolKind::Function);
    }

    #[test]
    fn test_resolves_class_targets() {
        // When callee is a class (e.g., Instantiates relationship), should resolve
        let candidates = vec![make_symbol(
            "UserService",
            SymbolKind::Class,
            "typescript",
            "src/services/user.ts",
        )];
        let pending = PendingRelationship {
            from_symbol_id: "caller_1".to_string(),
            callee_name: "UserService".to_string(),
            kind: RelationshipKind::Instantiates,
            file_path: "src/main.ts".to_string(),
            line_number: 10,
            confidence: 0.8,
        };

        let result = select_best_candidate(&candidates, &pending, &ParentReferenceContext::empty());
        assert!(
            result.is_some(),
            "Class symbols should be valid resolution targets"
        );
    }

    #[test]
    fn test_resolves_trait_targets() {
        let candidates = vec![make_symbol(
            "Serialize",
            SymbolKind::Trait,
            "rust",
            "src/traits.rs",
        )];
        let pending = PendingRelationship {
            from_symbol_id: "caller_1".to_string(),
            callee_name: "Serialize".to_string(),
            kind: RelationshipKind::Implements,
            file_path: "src/model.rs".to_string(),
            line_number: 5,
            confidence: 0.9,
        };

        let result = select_best_candidate(&candidates, &pending, &ParentReferenceContext::empty());
        assert!(
            result.is_some(),
            "Trait symbols should be valid resolution targets"
        );
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

        let result = select_best_candidate(&candidates, &pending, &ParentReferenceContext::empty());
        assert!(result.is_some());
        assert_eq!(result.unwrap().language, "rust");
    }

    #[test]
    fn test_falls_back_to_different_language_when_no_same_lang() {
        // Caller is Rust but only Python candidate exists — should still resolve
        let candidates = vec![make_symbol(
            "analyze",
            SymbolKind::Function,
            "python",
            "scripts/analyze.py",
        )];
        let pending = make_pending("caller_1", "analyze", "src/main.rs");

        let result = select_best_candidate(&candidates, &pending, &ParentReferenceContext::empty());
        assert!(
            result.is_some(),
            "Should fall back to different-language candidate when same-language unavailable"
        );
    }

    // =========================================================================
    // Path proximity
    // =========================================================================

    #[test]
    fn test_prefers_same_directory() {
        // Two candidates in same language, one in same dir, one far away
        let candidates = vec![
            make_symbol(
                "helper",
                SymbolKind::Function,
                "rust",
                "src/utils/helper.rs",
            ),
            make_symbol(
                "helper",
                SymbolKind::Function,
                "rust",
                "src/services/helper.rs",
            ),
        ];
        let pending = make_pending("caller_1", "helper", "src/utils/main.rs");

        let result = select_best_candidate(&candidates, &pending, &ParentReferenceContext::empty());
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

        let result = select_best_candidate(&candidates, &pending, &ParentReferenceContext::empty());
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

        let result = select_best_candidate(&candidates, &pending, &ParentReferenceContext::empty());
        assert!(result.is_some());
        assert_eq!(result.unwrap().kind, SymbolKind::Function);
    }

    #[test]
    fn test_instantiates_prefers_class_over_constructor() {
        // For an Instantiates relationship (DI registration), prefer Class over Constructor
        // when both share the same name (which is always the case in C#)
        let candidates = vec![
            make_symbol(
                "SearchFilesTool",
                SymbolKind::Constructor,
                "csharp",
                "Tools/SearchFilesTool.cs",
            ),
            make_symbol(
                "SearchFilesTool",
                SymbolKind::Class,
                "csharp",
                "Tools/SearchFilesTool.cs",
            ),
        ];
        let pending = PendingRelationship {
            from_symbol_id: "startup_1".to_string(),
            callee_name: "SearchFilesTool".to_string(),
            kind: RelationshipKind::Instantiates,
            file_path: "Program.cs".to_string(),
            line_number: 10,
            confidence: 0.9,
        };

        let result = select_best_candidate(&candidates, &pending, &ParentReferenceContext::empty());
        assert!(result.is_some());
        assert_eq!(
            result.unwrap().kind,
            SymbolKind::Class,
            "Instantiates should prefer Class over Constructor"
        );
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

        let result = select_best_candidate(&candidates, &pending, &ParentReferenceContext::empty());
        assert!(result.is_some());
        // Rust candidate should win even though Python is in the same directory
        assert_eq!(result.unwrap().language, "rust");
    }

    #[test]
    fn test_no_candidates_returns_none() {
        let candidates: Vec<Symbol> = vec![];
        let pending = make_pending("caller_1", "nonexistent", "src/main.rs");

        let result = select_best_candidate(&candidates, &pending, &ParentReferenceContext::empty());
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

        let result = select_best_candidate(&candidates, &pending, &ParentReferenceContext::empty());
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

    // =========================================================================
    // Parent reference disambiguation (import-constrained call edges)
    // =========================================================================

    /// Helper to create a Symbol with a parent_id
    fn make_child_symbol(
        name: &str,
        kind: SymbolKind,
        language: &str,
        file_path: &str,
        parent_id: &str,
    ) -> Symbol {
        let mut sym = make_symbol(name, kind, language, file_path);
        sym.parent_id = Some(parent_id.to_string());
        sym
    }

    /// Build a ParentReferenceContext for testing.
    /// Derives files_with_identifiers from file_ref_entries (any file in refs has identifiers).
    fn make_parent_ctx(
        parent_entries: &[(&str, &str)],
        file_ref_entries: &[(&str, &str)],
    ) -> ParentReferenceContext {
        let parent_names: HashMap<String, String> = parent_entries
            .iter()
            .map(|(id, name)| (id.to_string(), name.to_string()))
            .collect();
        let file_refs: HashSet<(String, String)> = file_ref_entries
            .iter()
            .map(|(file, name)| (file.to_string(), name.to_string()))
            .collect();
        let files_with_identifiers: HashSet<String> = file_ref_entries
            .iter()
            .map(|(file, _)| file.to_string())
            .collect();
        ParentReferenceContext::new(parent_names, file_refs, files_with_identifiers)
    }

    /// Build a ParentReferenceContext with explicit files_with_identifiers.
    /// Use when testing negative filtering (file has identifiers but no parent match).
    fn make_parent_ctx_with_files(
        parent_entries: &[(&str, &str)],
        file_ref_entries: &[(&str, &str)],
        files_with_ids: &[&str],
    ) -> ParentReferenceContext {
        let parent_names: HashMap<String, String> = parent_entries
            .iter()
            .map(|(id, name)| (id.to_string(), name.to_string()))
            .collect();
        let file_refs: HashSet<(String, String)> = file_ref_entries
            .iter()
            .map(|(file, name)| (file.to_string(), name.to_string()))
            .collect();
        let files_with_identifiers: HashSet<String> =
            files_with_ids.iter().map(|f| f.to_string()).collect();
        ParentReferenceContext::new(parent_names, file_refs, files_with_identifiers)
    }

    #[test]
    fn test_parent_reference_boosts_correct_candidate() {
        // Two methods named "Success" — one on AuthenticateResult, one on ApiResponse.
        // Caller file references AuthenticateResult → that candidate should win.
        let candidates = vec![
            make_child_symbol(
                "Success",
                SymbolKind::Method,
                "csharp",
                "Auth/AuthenticateResult.cs",
                "auth_result_class",
            ),
            make_child_symbol(
                "Success",
                SymbolKind::Method,
                "csharp",
                "Api/ApiResponse.cs",
                "api_response_class",
            ),
        ];
        let pending = make_pending("caller_1", "Success", "Controllers/AuthController.cs");

        let parent_ctx = make_parent_ctx(
            &[
                ("auth_result_class", "AuthenticateResult"),
                ("api_response_class", "ApiResponse"),
            ],
            &[("Controllers/AuthController.cs", "AuthenticateResult")],
        );

        let result = select_best_candidate(&candidates, &pending, &parent_ctx);
        assert!(result.is_some());
        assert_eq!(
            result.unwrap().file_path,
            "Auth/AuthenticateResult.cs",
            "Should prefer the candidate whose parent type is referenced by the caller file"
        );
    }

    #[test]
    fn test_parent_reference_dominates_path_proximity() {
        // Import-backed candidate is in a distant directory.
        // Non-import candidate is in the same directory.
        // Import signal (+200) should still dominate path proximity (+50).
        let candidates = vec![
            make_child_symbol(
                "Parse",
                SymbolKind::Method,
                "csharp",
                "src/utils/Parser.cs",
                "wrong_parser_class",
            ),
            make_child_symbol(
                "Parse",
                SymbolKind::Method,
                "csharp",
                "lib/deep/nested/RealParser.cs",
                "real_parser_class",
            ),
        ];
        // Caller is in src/utils/ — same dir as the wrong candidate
        let pending = make_pending("caller_1", "Parse", "src/utils/handler.cs");

        let parent_ctx = make_parent_ctx(
            &[
                ("wrong_parser_class", "WrongParser"),
                ("real_parser_class", "RealParser"),
            ],
            // Caller references RealParser, not WrongParser
            &[("src/utils/handler.cs", "RealParser")],
        );

        let result = select_best_candidate(&candidates, &pending, &parent_ctx);
        assert!(result.is_some());
        assert_eq!(
            result.unwrap().file_path,
            "lib/deep/nested/RealParser.cs",
            "Import-backed candidate should win despite being in a distant directory"
        );
    }

    #[test]
    fn test_no_parent_reference_falls_back_to_normal() {
        // Empty parent context — scoring should work exactly as before
        let candidates = vec![
            make_child_symbol(
                "Run",
                SymbolKind::Method,
                "csharp",
                "src/Runner.cs",
                "runner_class",
            ),
            make_child_symbol(
                "Run",
                SymbolKind::Method,
                "csharp",
                "tests/TestRunner.cs",
                "test_runner_class",
            ),
        ];
        let pending = make_pending("caller_1", "Run", "src/main.cs");

        let result = select_best_candidate(&candidates, &pending, &ParentReferenceContext::empty());
        assert!(result.is_some());
        // With no import context, same-directory candidate should win via path proximity
        assert_eq!(result.unwrap().file_path, "src/Runner.cs");
    }

    #[test]
    fn test_parent_reference_no_parent_id_no_crash() {
        // Candidate has no parent_id — should score normally without panic
        let candidates = vec![make_symbol(
            "helper",
            SymbolKind::Function,
            "rust",
            "src/utils.rs",
        )];
        let pending = make_pending("caller_1", "helper", "src/main.rs");

        let parent_ctx = make_parent_ctx(
            &[("some_class", "SomeClass")],
            &[("src/main.rs", "SomeClass")],
        );

        let result = select_best_candidate(&candidates, &pending, &parent_ctx);
        assert!(
            result.is_some(),
            "Candidates without parent_id should still resolve normally"
        );
    }

    // =========================================================================
    // Negative filtering: reject phantom edges to unrelated types
    // =========================================================================

    #[test]
    fn test_unmatched_parent_rejected_when_caller_has_identifiers() {
        // The LabHandbookV2 phantom edge: caller file has identifiers (we have data)
        // but the only "Success" candidate's parent (ApiResponse) is NOT referenced.
        // The real target (AuthenticateResult.Success) is an external framework type.
        // Candidate should be rejected (score 0) to prevent a false edge.
        let candidates = vec![make_child_symbol(
            "Success",
            SymbolKind::Method,
            "csharp",
            "Api/ApiResponse.cs",
            "api_response_class",
        )];
        let pending = make_pending("caller_1", "Success", "Controllers/AuthController.cs");

        // Caller file HAS identifiers, but NONE match ApiResponse
        let parent_ctx = make_parent_ctx_with_files(
            &[("api_response_class", "ApiResponse")],
            &[], // no file_refs — no parent matches
            &["Controllers/AuthController.cs"], // but file has identifier data
        );

        let result = select_best_candidate(&candidates, &pending, &parent_ctx);
        assert!(
            result.is_none(),
            "Should reject candidate whose parent type the caller doesn't reference"
        );
    }

    #[test]
    fn test_unmatched_parent_allowed_when_no_identifier_data() {
        // Same setup but caller file has NO identifiers (we have no data).
        // Should resolve normally — we can't make negative judgments without evidence.
        let candidates = vec![make_child_symbol(
            "Success",
            SymbolKind::Method,
            "csharp",
            "Api/ApiResponse.cs",
            "api_response_class",
        )];
        let pending = make_pending("caller_1", "Success", "Controllers/AuthController.cs");

        // Caller file has NO identifier data (not in files_with_identifiers)
        let parent_ctx = make_parent_ctx_with_files(
            &[("api_response_class", "ApiResponse")],
            &[],
            &[], // empty — no identifier data for this file
        );

        let result = select_best_candidate(&candidates, &pending, &parent_ctx);
        assert!(
            result.is_some(),
            "Should allow resolution when we have no identifier data to make a judgment"
        );
    }

    // =========================================================================
    // Test-file penalty: production symbols beat test subclasses
    // =========================================================================

    #[test]
    fn test_select_best_candidate_prefers_production_over_test_subclass() {
        // Real-world scenario: Flask project has `class Flask` in src/flask/app.py
        // and `class Flask(flask.Flask)` in tests/test_config.py.
        // When a call from tests/test_views.py references "Flask", without the
        // test-file penalty the test subclass wins via path proximity (both in tests/).
        // The penalty should ensure the production class wins instead.
        let candidates = vec![
            make_symbol("Flask", SymbolKind::Class, "python", "src/flask/app.py"),
            make_symbol(
                "Flask",
                SymbolKind::Class,
                "python",
                "tests/test_config.py",
            ),
        ];
        let pending = PendingRelationship {
            from_symbol_id: "test_fn".to_string(),
            callee_name: "Flask".to_string(),
            kind: RelationshipKind::Calls,
            file_path: "tests/test_views.py".to_string(),
            line_number: 15,
            confidence: 0.8,
        };

        let result = select_best_candidate(&candidates, &pending, &ParentReferenceContext::empty());
        assert!(result.is_some());
        assert_eq!(
            result.unwrap().file_path,
            "src/flask/app.py",
            "Production class should win over test subclass even when caller is in tests/"
        );
    }

    #[test]
    fn test_select_best_candidate_test_only_still_resolves() {
        // Edge case: when the only candidate is in a test file, it should still
        // resolve (score > 0) rather than being rejected entirely.
        let candidates = vec![make_symbol(
            "TestHelper",
            SymbolKind::Function,
            "python",
            "tests/conftest.py",
        )];
        let pending = PendingRelationship {
            from_symbol_id: "caller_1".to_string(),
            callee_name: "TestHelper".to_string(),
            kind: RelationshipKind::Calls,
            file_path: "tests/test_main.py".to_string(),
            line_number: 5,
            confidence: 0.8,
        };

        let result = select_best_candidate(&candidates, &pending, &ParentReferenceContext::empty());
        assert!(
            result.is_some(),
            "Test-only candidate should still resolve when it's the only option"
        );
    }
}
