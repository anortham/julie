//! Comprehensive integration test for the full Tantivy search pipeline.
//!
//! Verifies the complete flow: language-aware tokenization (CamelCase/snake_case splitting,
//! affix stripping, variant generation) → indexing → search → scoring boost → results.

#[cfg(test)]
mod tests {
    use crate::search::index::{SearchFilter, SearchIndex, SymbolDocument};
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
            .add_symbol(&SymbolDocument {
                id: "1".into(),
                name: "getUserProfile".into(),
                signature: "async function getUserProfile(id: string): Promise<User>".into(),
                doc_comment: "Fetches user profile from API".into(),
                code_body: "const response = await fetch(`/api/users/${id}`);".into(),
                file_path: "src/services/user.ts".into(),
                kind: "function".into(),
                language: "typescript".into(),
                start_line: 15,
            })
            .unwrap();

        index
            .add_symbol(&SymbolDocument {
                id: "2".into(),
                name: "get_user_profile".into(),
                signature: "pub async fn get_user_profile(id: &str) -> Result<User>".into(),
                doc_comment: "Fetches user profile from database".into(),
                code_body: "let user = db.query_one(\"SELECT * FROM users WHERE id = $1\", &[id]).await?;".into(),
                file_path: "src/services/user.rs".into(),
                kind: "function".into(),
                language: "rust".into(),
                start_line: 42,
            })
            .unwrap();
        index.commit().unwrap();

        // Search "user profile" should find both TS camelCase and Rust snake_case
        let results = index
            .search_symbols("user profile", &SearchFilter::default(), 10)
            .unwrap();
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
            .add_symbol(&SymbolDocument {
                id: "1".into(),
                name: "getUserProfile".into(),
                signature: "function getUserProfile()".into(),
                doc_comment: String::new(),
                code_body: String::new(),
                file_path: "src/user.ts".into(),
                kind: "function".into(),
                language: "typescript".into(),
                start_line: 1,
            })
            .unwrap();

        index
            .add_symbol(&SymbolDocument {
                id: "2".into(),
                name: "get_user_profile".into(),
                signature: "pub fn get_user_profile()".into(),
                doc_comment: String::new(),
                code_body: String::new(),
                file_path: "src/user.rs".into(),
                kind: "function".into(),
                language: "rust".into(),
                start_line: 1,
            })
            .unwrap();
        index.commit().unwrap();

        let filter = SearchFilter {
            language: Some("rust".into()),
            ..Default::default()
        };
        let results = index.search_symbols("user", &filter, 10).unwrap();
        assert_eq!(results.len(), 1, "Language filter should narrow to Rust only");
        assert_eq!(results[0].language, "rust");
    }

    #[test]
    fn test_name_match_ranks_highest() {
        let (_dir, index) = create_test_index();

        // A symbol where "user" appears only in doc_comment
        index
            .add_symbol(&SymbolDocument {
                id: "1".into(),
                name: "fetchData".into(),
                signature: "fn fetchData()".into(),
                doc_comment: "Gets user data from the API".into(),
                code_body: String::new(),
                file_path: "src/api.rs".into(),
                kind: "function".into(),
                language: "rust".into(),
                start_line: 1,
            })
            .unwrap();

        // A symbol where "user" is in the name
        index
            .add_symbol(&SymbolDocument {
                id: "2".into(),
                name: "getUser".into(),
                signature: "fn getUser()".into(),
                doc_comment: String::new(),
                code_body: String::new(),
                file_path: "src/api.rs".into(),
                kind: "function".into(),
                language: "rust".into(),
                start_line: 10,
            })
            .unwrap();
        index.commit().unwrap();

        let results = index
            .search_symbols("user", &SearchFilter::default(), 10)
            .unwrap();
        assert!(results.len() >= 2, "Should find both results");
        assert_eq!(
            results[0].name, "getUser",
            "Name match should rank higher than doc_comment match"
        );
    }

    #[test]
    fn test_important_patterns_boost_in_search() {
        let (_dir, index) = create_test_index();

        // Private function
        index
            .add_symbol(&SymbolDocument {
                id: "1".into(),
                name: "process_data".into(),
                signature: "fn process_data()".into(),
                doc_comment: String::new(),
                code_body: String::new(),
                file_path: "src/internal.rs".into(),
                kind: "function".into(),
                language: "rust".into(),
                start_line: 1,
            })
            .unwrap();

        // Public function — should get important_patterns boost
        index
            .add_symbol(&SymbolDocument {
                id: "2".into(),
                name: "process_data".into(),
                signature: "pub fn process_data()".into(),
                doc_comment: String::new(),
                code_body: String::new(),
                file_path: "src/lib.rs".into(),
                kind: "function".into(),
                language: "rust".into(),
                start_line: 1,
            })
            .unwrap();
        index.commit().unwrap();

        let results = index
            .search_symbols("process_data", &SearchFilter::default(), 10)
            .unwrap();
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
            .add_symbol(&SymbolDocument {
                id: "1".into(),
                name: "is_empty".into(),
                signature: "pub fn is_empty(&self) -> bool".into(),
                doc_comment: "Check if collection is empty".into(),
                code_body: "self.len() == 0".into(),
                file_path: "src/collection.rs".into(),
                kind: "function".into(),
                language: "rust".into(),
                start_line: 1,
            })
            .unwrap();
        index.commit().unwrap();

        // Search just "empty" — should find is_empty via affix stripping
        let results = index
            .search_symbols("empty", &SearchFilter::default(), 10)
            .unwrap();
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
            .add_symbol(&SymbolDocument {
                id: "1".into(),
                name: "IPaymentService".into(),
                signature: "public interface IPaymentService".into(),
                doc_comment: "Payment processing contract".into(),
                code_body: String::new(),
                file_path: "src/IPaymentService.cs".into(),
                kind: "interface".into(),
                language: "csharp".into(),
                start_line: 1,
            })
            .unwrap();
        index.commit().unwrap();

        // Search "PaymentService" without I prefix — should find IPaymentService
        let results = index
            .search_symbols("PaymentService", &SearchFilter::default(), 10)
            .unwrap();
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
            SymbolDocument {
                id: "1".into(),
                name: "getUserProfile".into(),
                signature: "async function getUserProfile(id: string): Promise<User>".into(),
                doc_comment: "Fetches user profile".into(),
                code_body: "return await fetch(`/api/users/${id}`)".into(),
                file_path: "src/services/user.ts".into(),
                kind: "function".into(),
                language: "typescript".into(),
                start_line: 15,
            },
            SymbolDocument {
                id: "2".into(),
                name: "process_payment".into(),
                signature: "pub async fn process_payment(amount: f64) -> Result<Receipt>".into(),
                doc_comment: "Process a payment transaction".into(),
                code_body: "let receipt = gateway.charge(amount).await?;".into(),
                file_path: "src/billing/payment.rs".into(),
                kind: "function".into(),
                language: "rust".into(),
                start_line: 42,
            },
            SymbolDocument {
                id: "3".into(),
                name: "IPaymentGateway".into(),
                signature: "public interface IPaymentGateway".into(),
                doc_comment: "Payment gateway contract".into(),
                code_body: String::new(),
                file_path: "src/IPaymentGateway.cs".into(),
                kind: "interface".into(),
                language: "csharp".into(),
                start_line: 1,
            },
        ];

        for doc in &symbols {
            index.add_symbol(doc).unwrap();
        }
        index.commit().unwrap();

        // Verify all symbols are now searchable
        assert!(index.num_docs() >= 3, "Should have at least 3 docs after backfill");

        // Cross-convention matching still works after backfill
        let results = index
            .search_symbols("user profile", &SearchFilter::default(), 10)
            .unwrap();
        assert!(!results.is_empty(), "Should find getUserProfile after backfill");

        // Language filter works after backfill
        let filter = SearchFilter {
            language: Some("rust".into()),
            ..Default::default()
        };
        let results = index.search_symbols("payment", &filter, 10).unwrap();
        assert_eq!(results.len(), 1, "Language filter should work after backfill");
        assert_eq!(results[0].name, "process_payment");

        // Variant stripping works after backfill
        let results = index
            .search_symbols("PaymentGateway", &SearchFilter::default(), 10)
            .unwrap();
        assert!(!results.is_empty(), "Should find IPaymentGateway via prefix stripping after backfill");
    }

    #[test]
    fn test_backfill_file_content() {
        // Verifies that file content (for line-level search) can also be
        // backfilled alongside symbols.
        use crate::search::index::FileDocument;

        let (_dir, index) = create_test_index();

        // Simulate backfilling file content from SQLite
        let file_doc = FileDocument {
            file_path: "src/main.rs".into(),
            content: "fn main() {\n    println!(\"hello world\");\n}".into(),
            language: "rust".into(),
        };
        index.add_file_content(&file_doc).unwrap();
        index.commit().unwrap();

        // Search for content
        let results = index
            .search_content("hello world", &SearchFilter::default(), 10)
            .unwrap();
        assert!(!results.is_empty(), "Should find file content after backfill");
        assert_eq!(results[0].file_path, "src/main.rs");
    }
}
