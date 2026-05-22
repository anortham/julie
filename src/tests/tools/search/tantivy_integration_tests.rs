//! Comprehensive integration test for the full Tantivy search pipeline.
//!
//! Verifies the complete flow: language-aware tokenization (CamelCase/snake_case splitting,
//! affix stripping, variant generation) → indexing → search → scoring boost → results.

#[cfg(test)]
mod tests {
    use crate::search::index::{SearchDocument, SearchFilter, SearchIndex};
    use crate::search::language_config::LanguageConfigs;
    use tempfile::TempDir;

    fn create_test_index() -> (TempDir, SearchIndex) {
        let temp_dir = TempDir::new().unwrap();
        let configs = LanguageConfigs::load_embedded();
        let index = SearchIndex::create_with_language_configs(temp_dir.path(), &configs).unwrap();
        (temp_dir, index)
    }

    #[test]
    fn test_cross_convention_matching() {
        let (_dir, index) = create_test_index();

        index
            .add_search_doc(&SearchDocument::symbol_from_parts(
                "1",
                "getUserProfile",
                "async function getUserProfile(id: string): Promise<User>",
                "Fetches user profile from API",
                "const response = await fetch(`/api/users/${id}`);",
                "src/services/user.ts",
                "function",
                "typescript",
                15,
            ))
            .unwrap();

        index
            .add_search_doc(&SearchDocument::symbol_from_parts(
                "2",
                "get_user_profile",
                "pub async fn get_user_profile(id: &str) -> Result<User>",
                "Fetches user profile from database",
                "let user = db.query_one(\"SELECT * FROM users WHERE id = $1\", &[id]).await?;",
                "src/services/user.rs",
                "function",
                "rust",
                42,
            ))
            .unwrap();
        index.commit().unwrap();

        // Search "user profile" should find both TS camelCase and Rust snake_case
        let results = index
            .search_symbols("user profile", &SearchFilter::default(), 10)
            .unwrap()
            .results;
        assert_eq!(
            results.len(),
            2,
            "Should find both TS camelCase and Rust snake_case: {:?}",
            results.iter().map(|r| &r.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_language_filtering() {
        let (_dir, index) = create_test_index();

        index
            .add_search_doc(&SearchDocument::symbol_from_parts(
                "1",
                "getUserProfile",
                "function getUserProfile()",
                String::new(),
                String::new(),
                "src/user.ts",
                "function",
                "typescript",
                1,
            ))
            .unwrap();

        index
            .add_search_doc(&SearchDocument::symbol_from_parts(
                "2",
                "get_user_profile",
                "pub fn get_user_profile()",
                String::new(),
                String::new(),
                "src/user.rs",
                "function",
                "rust",
                1,
            ))
            .unwrap();
        index.commit().unwrap();

        let filter = SearchFilter {
            language: Some("rust".to_string()),
            ..Default::default()
        };
        let results = index.search_symbols("user", &filter, 10).unwrap().results;
        assert_eq!(
            results.len(),
            1,
            "Language filter should narrow to Rust only"
        );
        assert_eq!(results[0].language, "rust");
    }

    #[test]
    fn test_symbol_search_applies_file_pattern_filter() {
        let (_dir, index) = create_test_index();

        index
            .add_search_doc(&SearchDocument::symbol_from_parts(
                "1",
                "target_context_symbol",
                "pub fn target_context_symbol()",
                "shared filter token",
                String::new(),
                "src/target/context.rs",
                "function",
                "rust",
                1,
            ))
            .unwrap();

        index
            .add_search_doc(&SearchDocument::symbol_from_parts(
                "2",
                "outside_context_symbol",
                "pub fn outside_context_symbol()",
                "shared filter token",
                String::new(),
                "src/outside/context.rs",
                "function",
                "rust",
                1,
            ))
            .unwrap();
        index.commit().unwrap();

        let filter = SearchFilter {
            file_pattern: Some("src/target/**".to_string()),
            ..Default::default()
        };
        let results = index
            .search_symbols("shared filter token", &filter, 10)
            .unwrap()
            .results;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].file_path, "src/target/context.rs");
    }

    #[test]
    fn test_symbol_search_excludes_tests_when_requested() {
        let (_dir, index) = create_test_index();

        index
            .add_search_doc(&SearchDocument::symbol_from_parts(
                "1",
                "production_context_symbol",
                "pub fn production_context_symbol()",
                "shared production token",
                String::new(),
                "src/context.rs",
                "function",
                "rust",
                1,
            ))
            .unwrap();

        index
            .add_search_doc(&SearchDocument::symbol_from_parts(
                "2",
                "test_context_symbol",
                "fn test_context_symbol()",
                "shared production token",
                String::new(),
                "tests/context_test.rs",
                "function",
                "rust",
                1,
            ))
            .unwrap();
        index.commit().unwrap();

        let filter = SearchFilter {
            exclude_tests: true,
            ..Default::default()
        };
        let results = index
            .search_symbols("shared production token", &filter, 10)
            .unwrap()
            .results;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].file_path, "src/context.rs");
    }

    #[test]
    fn test_name_match_ranks_highest() {
        let (_dir, index) = create_test_index();

        // A symbol where "user" appears only in doc_comment
        index
            .add_search_doc(&SearchDocument::symbol_from_parts(
                "1",
                "fetchData",
                "fn fetchData()",
                "Gets user data from the API",
                String::new(),
                "src/api.rs",
                "function",
                "rust",
                1,
            ))
            .unwrap();

        // A symbol where "user" is in the name
        index
            .add_search_doc(&SearchDocument::symbol_from_parts(
                "2",
                "getUser",
                "fn getUser()",
                String::new(),
                String::new(),
                "src/api.rs",
                "function",
                "rust",
                10,
            ))
            .unwrap();
        index.commit().unwrap();

        let results = index
            .search_symbols("user", &SearchFilter::default(), 10)
            .unwrap()
            .results;
        assert!(results.len() >= 2, "Should find both results");
        assert_eq!(
            results[0].name, "getUser",
            "Name match should rank higher than doc_comment match"
        );
    }

    #[test]
    fn test_nl_query_prefers_code_over_docs() {
        use crate::tools::search::text_search::definition_search_with_index_for_test;

        let (_dir, index) = create_test_index();

        // Docs symbol intentionally inserted first so equal-score ties prefer docs
        // before reranking.
        index
            .add_search_doc(&SearchDocument::symbol_from_parts(
                "1",
                "workspace_routing_handler",
                "fn workspace_routing_handler()",
                "Handles workspace routing for requests.",
                "workspace routing handler",
                "docs/workspace/routing.md",
                "function",
                "markdown",
                1,
            ))
            .unwrap();

        // Test symbol with identical textual relevance.
        index
            .add_search_doc(&SearchDocument::symbol_from_parts(
                "2",
                "workspace_routing_handler",
                "fn workspace_routing_handler()",
                "Handles workspace routing for requests.",
                "workspace routing handler",
                "src/tests/search/workspace_routing.rs",
                "function",
                "rust",
                5,
            ))
            .unwrap();

        // Production symbol with identical textual relevance should win after NL path prior.
        // Path contains "routing" so the reranker's per-term path boost matches
        // the docs candidate's — leaving the NL path prior as the differentiator.
        index
            .add_search_doc(&SearchDocument::symbol_from_parts(
                "3",
                "workspace_routing_handler",
                "fn workspace_routing_handler()",
                "Handles workspace routing for requests.",
                "workspace routing handler",
                "src/workspace/routing.rs",
                "function",
                "rust",
                32,
            ))
            .unwrap();

        index.commit().unwrap();

        // Path prior is owned by the assembly layer, so run the full pipeline.
        let (symbols, _relaxed, _total) = definition_search_with_index_for_test(
            "workspace routing",
            &SearchFilter::default(),
            2,
            &index,
            None,
        )
        .unwrap();

        assert_eq!(
            symbols[0].file_path,
            "src/workspace/routing.rs",
            "Production code should be preferred over docs/tests for NL query: {:?}",
            symbols
                .iter()
                .map(|s| (&s.file_path, s.confidence))
                .collect::<Vec<_>>()
        );
        let docs_rank = symbols
            .iter()
            .position(|s| s.file_path == "docs/workspace/routing.md");
        assert_ne!(
            docs_rank,
            Some(0),
            "Docs result must not outrank production code for NL query: {:?}",
            symbols
                .iter()
                .map(|s| (&s.file_path, s.confidence))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_nl_path_prior_can_change_top1_with_small_limit() {
        use crate::tools::search::text_search::definition_search_with_index_for_test;

        let (_dir, index) = create_test_index();

        // Insert docs first so pre-rerank TopDocs(limit=1) would otherwise return docs.
        index
            .add_search_doc(&SearchDocument::symbol_from_parts(
                "1",
                "workspace_routing_handler",
                "fn workspace_routing_handler()",
                "Handles workspace routing for requests.",
                "workspace routing handler",
                "docs/workspace/routing.md",
                "function",
                "markdown",
                1,
            ))
            .unwrap();

        index
            .add_search_doc(&SearchDocument::symbol_from_parts(
                "2",
                "workspace_routing_handler",
                "fn workspace_routing_handler()",
                "Handles workspace routing for requests.",
                "workspace routing handler",
                "src/workspace/routing.rs",
                "function",
                "rust",
                1,
            ))
            .unwrap();
        index.commit().unwrap();

        // The NL path prior lives at the assembly layer, so the assertion
        // must run through `definition_search_with_index` — that's where
        // the over-fetch + prior interaction now lives.
        let (symbols, _relaxed, _total) = definition_search_with_index_for_test(
            "workspace routing",
            &SearchFilter::default(),
            1,
            &index,
            None,
        )
        .unwrap();

        assert_eq!(symbols.len(), 1);
        assert_eq!(
            symbols[0].file_path, "src/workspace/routing.rs",
            "With over-fetch + assembly-layer NL path prior, src/ should outrank docs/ at top-1"
        );
    }

    #[test]
    fn test_non_adjacent_duplicate_terms_do_not_change_relaxed_scores() {
        let (_dir, index) = create_test_index();

        index
            .add_search_doc(&SearchDocument::symbol_from_parts(
                "1",
                "workspace_router_registry",
                "fn workspace_router_registry()",
                "router registry for workspace routing",
                "workspace router registry",
                "src/workspace/router.rs",
                "function",
                "rust",
                1,
            ))
            .unwrap();
        index.commit().unwrap();

        let deduped = index
            .search_symbols("workspace missing", &SearchFilter::default(), 5)
            .unwrap();
        let with_non_adj_dup = index
            .search_symbols("workspace missing workspace", &SearchFilter::default(), 5)
            .unwrap();

        assert!(deduped.relaxed && with_non_adj_dup.relaxed);
        assert_eq!(deduped.results.len(), with_non_adj_dup.results.len());
        assert_eq!(deduped.results[0].id, with_non_adj_dup.results[0].id);
        assert!(
            (deduped.results[0].score - with_non_adj_dup.results[0].score).abs() < 1e-6,
            "Non-adjacent duplicate terms should be removed deterministically before query build"
        );
    }

    #[test]
    fn test_important_patterns_boost_in_search() {
        let (_dir, index) = create_test_index();

        // Private function
        index
            .add_search_doc(&SearchDocument::symbol_from_parts(
                "1",
                "process_data",
                "fn process_data()",
                String::new(),
                String::new(),
                "src/internal.rs",
                "function",
                "rust",
                1,
            ))
            .unwrap();

        // Public function — should get important_patterns boost
        index
            .add_search_doc(&SearchDocument::symbol_from_parts(
                "2",
                "process_data",
                "pub fn process_data()",
                String::new(),
                String::new(),
                "src/lib.rs",
                "function",
                "rust",
                1,
            ))
            .unwrap();
        index.commit().unwrap();

        let results = index
            .search_symbols("process_data", &SearchFilter::default(), 10)
            .unwrap()
            .results;
        assert!(results.len() >= 2, "Should find both results");
        assert!(
            results[0].signature.contains("pub fn"),
            "pub fn should rank first due to important_patterns boost: {:?}",
            results
                .iter()
                .map(|r| (&r.signature, r.score))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_affix_stripping_improves_search() {
        let (_dir, index) = create_test_index();

        // Index a function with "is_" prefix affix
        index
            .add_search_doc(&SearchDocument::symbol_from_parts(
                "1",
                "is_empty",
                "pub fn is_empty(&self) -> bool",
                "Check if collection is empty",
                "self.len() == 0",
                "src/collection.rs",
                "function",
                "rust",
                1,
            ))
            .unwrap();
        index.commit().unwrap();

        // Search just "empty" — should find is_empty via affix stripping
        let results = index
            .search_symbols("empty", &SearchFilter::default(), 10)
            .unwrap()
            .results;
        assert!(
            !results.is_empty(),
            "Searching 'empty' should find 'is_empty' via affix stripping"
        );
        assert_eq!(results[0].name, "is_empty");
    }

    #[test]
    fn test_variant_stripping_improves_search() {
        let (_dir, index) = create_test_index();

        // Index a C#-style interface
        index
            .add_search_doc(&SearchDocument::symbol_from_parts(
                "1",
                "IPaymentService",
                "public interface IPaymentService",
                "Payment processing contract",
                String::new(),
                "src/IPaymentService.cs",
                "interface",
                "csharp",
                1,
            ))
            .unwrap();
        index.commit().unwrap();

        // Search "PaymentService" without I prefix — should find IPaymentService
        let results = index
            .search_symbols("PaymentService", &SearchFilter::default(), 10)
            .unwrap()
            .results;
        assert!(
            !results.is_empty(),
            "Searching 'PaymentService' should find 'IPaymentService' via prefix stripping"
        );
        assert_eq!(results[0].name, "IPaymentService");
    }

    #[test]
    fn test_backfill_from_existing_symbols() {
        // Simulates the v1→v2 upgrade backfill: an empty Tantivy index gets
        // populated from symbols that already exist in SQLite.
        let (_dir, index) = create_test_index();

        // Verify index starts empty
        assert_eq!(index.num_docs(), 0, "Fresh index should have 0 docs");

        // Simulate backfill: add symbols as if reading from SQLite
        let symbols = vec![
            SearchDocument::symbol_from_parts(
                "1",
                "getUserProfile",
                "async function getUserProfile(id: string): Promise<User>",
                "Fetches user profile",
                "return await fetch(`/api/users/${id}`)",
                "src/services/user.ts",
                "function",
                "typescript",
                15,
            ),
            SearchDocument::symbol_from_parts(
                "2",
                "process_payment",
                "pub async fn process_payment(amount: f64) -> Result<Receipt>",
                "Process a payment transaction",
                "let receipt = gateway.charge(amount).await?;",
                "src/billing/payment.rs",
                "function",
                "rust",
                42,
            ),
            SearchDocument::symbol_from_parts(
                "3",
                "IPaymentGateway",
                "public interface IPaymentGateway",
                "Payment gateway contract",
                "",
                "src/IPaymentGateway.cs",
                "interface",
                "csharp",
                1,
            ),
        ];

        for doc in &symbols {
            index.add_search_doc(doc).unwrap();
        }
        index.commit().unwrap();

        // Verify all symbols are now searchable
        assert!(
            index.num_docs() >= 3,
            "Should have at least 3 docs after backfill"
        );

        // Cross-convention matching still works after backfill
        let results = index
            .search_symbols("user profile", &SearchFilter::default(), 10)
            .unwrap()
            .results;
        assert!(
            !results.is_empty(),
            "Should find getUserProfile after backfill"
        );

        // Language filter works after backfill
        let filter = SearchFilter {
            language: Some("rust".to_string()),
            ..Default::default()
        };
        let results = index
            .search_symbols("payment", &filter, 10)
            .unwrap()
            .results;
        assert_eq!(
            results.len(),
            1,
            "Language filter should work after backfill"
        );
        assert_eq!(results[0].name, "process_payment");

        // Variant stripping works after backfill
        let results = index
            .search_symbols("PaymentGateway", &SearchFilter::default(), 10)
            .unwrap()
            .results;
        assert!(
            !results.is_empty(),
            "Should find IPaymentGateway via prefix stripping after backfill"
        );
    }

    #[test]
    fn test_backfill_file_content() {
        // Verifies that file content (for line-level search) can also be
        // backfilled alongside symbols.

        let (_dir, index) = create_test_index();

        // Simulate backfilling file content from SQLite
        index
            .add_search_doc(&SearchDocument::file_from_parts(
                "src/main.rs",
                "fn main() {\n    println!(\"hello world\");\n}",
                "rust",
            ))
            .unwrap();
        index.commit().unwrap();

        // Search for content
        let results = index
            .search_content("hello world", &SearchFilter::default(), 10)
            .unwrap()
            .results;
        assert!(
            !results.is_empty(),
            "Should find file content after backfill"
        );
        assert_eq!(results[0].file_path, "src/main.rs");
    }
}
