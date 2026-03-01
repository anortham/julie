//! Tests for symbol metadata formatting (embeddings::metadata).

#[cfg(test)]
mod tests {
    use crate::embeddings::metadata::{
        format_symbol_metadata, is_embeddable_kind, is_embeddable_language,
        prepare_batch_for_embedding,
    };
    use crate::extractors::{Symbol, SymbolKind};

    /// Helper: create a minimal test symbol.
    fn make_symbol(
        id: &str,
        name: &str,
        kind: SymbolKind,
        signature: Option<&str>,
        doc_comment: Option<&str>,
    ) -> Symbol {
        Symbol {
            id: id.to_string(),
            name: name.to_string(),
            kind,
            language: "rust".to_string(),
            file_path: "src/lib.rs".to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 10,
            end_column: 0,
            start_byte: 0,
            end_byte: 100,
            signature: signature.map(|s| s.to_string()),
            doc_comment: doc_comment.map(|s| s.to_string()),
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
            content_type: None,
        }
    }

    /// Helper: create a test symbol with a specific language.
    fn make_symbol_with_lang(id: &str, name: &str, kind: SymbolKind, language: &str) -> Symbol {
        let mut sym = make_symbol(id, name, kind, None, None);
        sym.language = language.to_string();
        sym
    }

    // =========================================================================
    // is_embeddable_language
    // =========================================================================

    #[test]
    fn test_embeddable_languages() {
        let code_languages = [
            "rust",
            "python",
            "csharp",
            "typescript",
            "javascript",
            "go",
            "java",
            "kotlin",
            "swift",
            "cpp",
            "c",
            "php",
            "ruby",
            "lua",
            "dart",
            "zig",
            "gdscript",
            "qml",
            "r",
            "vue",
            "bash",
            "powershell",
        ];
        for lang in &code_languages {
            assert!(is_embeddable_language(lang), "{lang} should be embeddable");
        }
    }

    #[test]
    fn test_non_embeddable_languages() {
        let non_code_languages = [
            "markdown", "json", "jsonl", "toml", "yaml", "css", "html", "regex", "sql",
        ];
        for lang in &non_code_languages {
            assert!(
                !is_embeddable_language(lang),
                "{lang} should NOT be embeddable"
            );
        }
    }

    #[test]
    fn test_prepare_batch_filters_non_code_languages() {
        let symbols = vec![
            make_symbol_with_lang("s1", "MyClass", SymbolKind::Class, "rust"),
            make_symbol_with_lang("s2", "Features", SymbolKind::Module, "markdown"),
            make_symbol_with_lang("s3", "search_impl", SymbolKind::Function, "python"),
            make_symbol_with_lang("s4", "config", SymbolKind::Module, "toml"),
            make_symbol_with_lang("s5", "SearchTool", SymbolKind::Class, "csharp"),
        ];

        let batch = prepare_batch_for_embedding(&symbols);
        assert_eq!(batch.len(), 3, "Should filter out markdown and toml");

        let ids: Vec<&str> = batch.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, vec!["s1", "s3", "s5"]);
    }

    // =========================================================================
    // is_embeddable_kind
    // =========================================================================

    #[test]
    fn test_embeddable_kinds() {
        let embeddable = [
            SymbolKind::Function,
            SymbolKind::Method,
            SymbolKind::Class,
            SymbolKind::Struct,
            SymbolKind::Interface,
            SymbolKind::Trait,
            SymbolKind::Enum,
            SymbolKind::Type,
            SymbolKind::Module,
            SymbolKind::Namespace,
            SymbolKind::Union,
        ];
        for kind in &embeddable {
            assert!(is_embeddable_kind(kind), "{kind:?} should be embeddable");
        }
    }

    #[test]
    fn test_non_embeddable_kinds() {
        let non_embeddable = [
            SymbolKind::Variable,
            SymbolKind::Constant,
            SymbolKind::Property,
            SymbolKind::EnumMember,
            SymbolKind::Field,
            SymbolKind::Constructor,
            SymbolKind::Destructor,
            SymbolKind::Operator,
            SymbolKind::Import,
            SymbolKind::Export,
            SymbolKind::Event,
            SymbolKind::Delegate,
        ];
        for kind in &non_embeddable {
            assert!(
                !is_embeddable_kind(kind),
                "{kind:?} should NOT be embeddable"
            );
        }
    }

    // =========================================================================
    // format_symbol_metadata
    // =========================================================================

    #[test]
    fn test_format_with_all_fields() {
        let sym = make_symbol(
            "id1",
            "process_payment",
            SymbolKind::Function,
            Some("fn process_payment(amount: f64, currency: &str) -> Result<Receipt>"),
            Some("/// Processes a payment transaction and returns a receipt."),
        );
        let text = format_symbol_metadata(&sym);
        assert!(text.starts_with("function process_payment"));
        assert!(text.contains("amount: f64"));
        assert!(text.contains("Processes a payment transaction"));
    }

    #[test]
    fn test_format_without_signature() {
        let sym = make_symbol(
            "id2",
            "UserService",
            SymbolKind::Class,
            None,
            Some("/// Manages user authentication and authorization."),
        );
        let text = format_symbol_metadata(&sym);
        assert_eq!(
            text,
            "class UserService Manages user authentication and authorization."
        );
    }

    #[test]
    fn test_format_without_doc_comment() {
        let sym = make_symbol(
            "id3",
            "DatabaseConnection",
            SymbolKind::Struct,
            Some("pub struct DatabaseConnection"),
            None,
        );
        let text = format_symbol_metadata(&sym);
        assert_eq!(
            text,
            "struct DatabaseConnection pub struct DatabaseConnection"
        );
    }

    #[test]
    fn test_format_name_only() {
        let sym = make_symbol("id4", "MyModule", SymbolKind::Module, None, None);
        let text = format_symbol_metadata(&sym);
        assert_eq!(text, "module MyModule");
    }

    #[test]
    fn test_format_strips_doc_comment_markers() {
        // Rust-style
        let sym = make_symbol(
            "id5",
            "foo",
            SymbolKind::Function,
            None,
            Some("/// Does something useful."),
        );
        let text = format_symbol_metadata(&sym);
        assert!(
            text.contains("Does something useful."),
            "Should strip /// prefix: {text}"
        );
        assert!(!text.contains("///"));

        // Python/markdown-style
        let sym2 = make_symbol(
            "id6",
            "Bar",
            SymbolKind::Class,
            None,
            Some("# A utility class for bar operations."),
        );
        let text2 = format_symbol_metadata(&sym2);
        assert!(
            text2.contains("A utility class"),
            "Should strip # prefix: {text2}"
        );
    }

    #[test]
    fn test_format_truncates_long_text() {
        let long_sig = "a".repeat(500);
        let sym = make_symbol("id7", "x", SymbolKind::Function, Some(&long_sig), None);
        let text = format_symbol_metadata(&sym);
        assert!(
            text.len() <= 600,
            "Should be truncated to ≤600 chars, got {}",
            text.len()
        );
    }

    #[test]
    fn test_format_no_double_spaces() {
        let sym = make_symbol(
            "id8",
            "test_func",
            SymbolKind::Function,
            Some("fn test_func()"),
            Some("/// A test function."),
        );
        let text = format_symbol_metadata(&sym);
        assert!(
            !text.contains("  "),
            "Should not contain double spaces: '{text}'"
        );
    }

    #[test]
    fn test_format_unicode_names() {
        let sym = make_symbol(
            "id9",
            "処理データ",
            SymbolKind::Function,
            Some("fn 処理データ()"),
            None,
        );
        let text = format_symbol_metadata(&sym);
        assert!(text.contains("処理データ"));
    }

    #[test]
    fn test_format_multiline_signature_uses_first_line() {
        let sym = make_symbol(
            "id10",
            "complex",
            SymbolKind::Function,
            Some("fn complex(\n    arg1: i32,\n    arg2: String,\n) -> Result<()>"),
            None,
        );
        let text = format_symbol_metadata(&sym);
        assert!(
            text.contains("fn complex("),
            "Should use first line: {text}"
        );
        assert!(
            !text.contains("arg1"),
            "Should not include subsequent lines"
        );
    }

    // =========================================================================
    // prepare_batch_for_embedding
    // =========================================================================

    #[test]
    fn test_prepare_batch_filters_non_embeddable() {
        let symbols = vec![
            make_symbol("s1", "MyClass", SymbolKind::Class, None, None),
            make_symbol("s2", "my_var", SymbolKind::Variable, None, None),
            make_symbol("s3", "my_func", SymbolKind::Function, None, None),
            make_symbol("s4", "os", SymbolKind::Import, None, None),
            make_symbol("s5", "MyTrait", SymbolKind::Trait, None, None),
        ];

        let batch = prepare_batch_for_embedding(&symbols);
        assert_eq!(batch.len(), 3, "Should filter to 3 embeddable symbols");

        let ids: Vec<&str> = batch.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, vec!["s1", "s3", "s5"]);
    }

    #[test]
    fn test_prepare_batch_empty_input() {
        let batch = prepare_batch_for_embedding(&[]);
        assert!(batch.is_empty());
    }

    #[test]
    fn test_prepare_batch_all_non_embeddable() {
        let symbols = vec![
            make_symbol("s1", "x", SymbolKind::Variable, None, None),
            make_symbol("s2", "Y", SymbolKind::Constant, None, None),
            make_symbol("s3", "z", SymbolKind::Import, None, None),
        ];

        let batch = prepare_batch_for_embedding(&symbols);
        assert!(
            batch.is_empty(),
            "All non-embeddable should produce empty batch"
        );
    }

    // =========================================================================
    // Child method enrichment for container symbols
    // =========================================================================

    #[test]
    fn test_prepare_batch_enriches_class_with_child_methods() {
        let mut class_sym = make_symbol(
            "c1",
            "LuceneIndexService",
            SymbolKind::Class,
            None,
            Some("/// Thread-safe Lucene index service"),
        );
        class_sym.language = "csharp".to_string();

        let mut method1 = make_symbol_with_lang("m1", "SearchAsync", SymbolKind::Method, "csharp");
        method1.parent_id = Some("c1".to_string());

        let mut method2 =
            make_symbol_with_lang("m2", "IndexDocumentAsync", SymbolKind::Method, "csharp");
        method2.parent_id = Some("c1".to_string());

        let mut method3 =
            make_symbol_with_lang("m3", "DeleteDocumentAsync", SymbolKind::Method, "csharp");
        method3.parent_id = Some("c1".to_string());

        let symbols = vec![class_sym, method1, method2, method3];
        let batch = prepare_batch_for_embedding(&symbols);

        // Class + 3 methods = 4 embeddable symbols
        assert_eq!(batch.len(), 4);

        // Find the class entry and check it contains method names
        let class_entry = batch.iter().find(|(id, _)| id == "c1").unwrap();
        assert!(
            class_entry.1.contains("SearchAsync"),
            "Class embedding should include child method name 'SearchAsync': {}",
            class_entry.1
        );
        assert!(
            class_entry.1.contains("IndexDocumentAsync"),
            "Class embedding should include child method name 'IndexDocumentAsync': {}",
            class_entry.1
        );
    }

    #[test]
    fn test_prepare_batch_enriches_interface_with_methods() {
        let iface = make_symbol_with_lang("i1", "ISearchService", SymbolKind::Interface, "csharp");

        let mut method1 = make_symbol_with_lang("m1", "Search", SymbolKind::Method, "csharp");
        method1.parent_id = Some("i1".to_string());

        let mut method2 = make_symbol_with_lang("m2", "Initialize", SymbolKind::Method, "csharp");
        method2.parent_id = Some("i1".to_string());

        let symbols = vec![iface, method1, method2];
        let batch = prepare_batch_for_embedding(&symbols);

        let iface_entry = batch.iter().find(|(id, _)| id == "i1").unwrap();
        assert!(
            iface_entry.1.contains("Search"),
            "Interface embedding should include child method names: {}",
            iface_entry.1
        );
    }

    #[test]
    fn test_prepare_batch_no_enrichment_for_functions() {
        let func = make_symbol("f1", "standalone_func", SymbolKind::Function, None, None);

        let symbols = vec![func];
        let batch = prepare_batch_for_embedding(&symbols);

        assert_eq!(batch.len(), 1);
        assert_eq!(batch[0].1, "function standalone_func");
    }

    #[test]
    fn test_child_enrichment_truncates_within_budget() {
        let class_sym = make_symbol(
            "c1",
            "HugeClass",
            SymbolKind::Class,
            Some("pub class HugeClass : BaseClass, IDisposable, IAsyncDisposable"),
            Some("/// A very large class with many methods for comprehensive testing"),
        );

        // Create 30 child methods with long names
        let mut symbols = vec![class_sym];
        for i in 0..30 {
            let mut method = make_symbol_with_lang(
                &format!("m{i}"),
                &format!("VeryLongMethodName{i}ForTesting"),
                SymbolKind::Method,
                "rust",
            );
            method.parent_id = Some("c1".to_string());
            symbols.push(method);
        }

        let batch = prepare_batch_for_embedding(&symbols);
        let class_entry = batch.iter().find(|(id, _)| id == "c1").unwrap();

        assert!(
            class_entry.1.len() <= 600,
            "Enriched text should be within 600 chars, got {}",
            class_entry.1.len()
        );
    }

    // =========================================================================
    // Property/field enrichment for container symbols
    // =========================================================================

    #[test]
    fn test_prepare_batch_enriches_class_with_child_properties() {
        // Simulates a C# DTO: class UserDto with property children
        let class_sym = make_symbol_with_lang("c1", "UserDto", SymbolKind::Class, "csharp");

        let mut prop1 = make_symbol_with_lang("p1", "Id", SymbolKind::Property, "csharp");
        prop1.parent_id = Some("c1".to_string());

        let mut prop2 =
            make_symbol_with_lang("p2", "SamAccountName", SymbolKind::Property, "csharp");
        prop2.parent_id = Some("c1".to_string());

        let mut prop3 = make_symbol_with_lang("p3", "Email", SymbolKind::Property, "csharp");
        prop3.parent_id = Some("c1".to_string());

        let mut prop4 = make_symbol_with_lang("p4", "Roles", SymbolKind::Property, "csharp");
        prop4.parent_id = Some("c1".to_string());

        let symbols = vec![class_sym, prop1, prop2, prop3, prop4];
        let batch = prepare_batch_for_embedding(&symbols);

        // Only the class should be embedded (properties are not embeddable kinds)
        assert_eq!(batch.len(), 1);

        let class_entry = &batch[0];
        assert!(
            class_entry.1.contains("SamAccountName"),
            "Class embedding should include child property name 'SamAccountName': {}",
            class_entry.1
        );
        assert!(
            class_entry.1.contains("Email"),
            "Class embedding should include child property name 'Email': {}",
            class_entry.1
        );
        assert!(
            class_entry.1.contains("properties:"),
            "Property enrichment should use 'properties:' label: {}",
            class_entry.1
        );
    }

    #[test]
    fn test_prepare_batch_enriches_interface_with_fields() {
        // Simulates a TypeScript interface: interface PageDto with field children
        let iface = make_symbol_with_lang("i1", "PageDto", SymbolKind::Interface, "typescript");

        let mut field1 = make_symbol_with_lang("f1", "id", SymbolKind::Field, "typescript");
        field1.parent_id = Some("i1".to_string());

        let mut field2 = make_symbol_with_lang("f2", "title", SymbolKind::Field, "typescript");
        field2.parent_id = Some("i1".to_string());

        let mut field3 = make_symbol_with_lang("f3", "slug", SymbolKind::Field, "typescript");
        field3.parent_id = Some("i1".to_string());

        let symbols = vec![iface, field1, field2, field3];
        let batch = prepare_batch_for_embedding(&symbols);

        assert_eq!(batch.len(), 1);

        let iface_entry = &batch[0];
        assert!(
            iface_entry.1.contains("title"),
            "Interface embedding should include child field name 'title': {}",
            iface_entry.1
        );
        assert!(
            iface_entry.1.contains("slug"),
            "Interface embedding should include child field name 'slug': {}",
            iface_entry.1
        );
    }

    #[test]
    fn test_prepare_batch_enriches_with_both_methods_and_properties() {
        // A class with both methods and properties should include both
        let class_sym = make_symbol_with_lang("c1", "UserService", SymbolKind::Class, "csharp");

        let mut method = make_symbol_with_lang("m1", "GetUserById", SymbolKind::Method, "csharp");
        method.parent_id = Some("c1".to_string());

        let mut prop = make_symbol_with_lang("p1", "DbContext", SymbolKind::Property, "csharp");
        prop.parent_id = Some("c1".to_string());

        let symbols = vec![class_sym, method, prop];
        let batch = prepare_batch_for_embedding(&symbols);

        let class_entry = batch.iter().find(|(id, _)| id == "c1").unwrap();
        assert!(
            class_entry.1.contains("methods:"),
            "Should have methods label: {}",
            class_entry.1
        );
        assert!(
            class_entry.1.contains("GetUserById"),
            "Should include method name: {}",
            class_entry.1
        );
        assert!(
            class_entry.1.contains("properties:"),
            "Should have properties label: {}",
            class_entry.1
        );
        assert!(
            class_entry.1.contains("DbContext"),
            "Should include property name: {}",
            class_entry.1
        );
    }

    #[test]
    fn test_prepare_batch_struct_enriched_with_fields() {
        // Rust struct with field children
        let struct_sym = make_symbol("s1", "SearchResult", SymbolKind::Struct, None, None);

        let mut field1 = make_symbol_with_lang("f1", "name", SymbolKind::Field, "rust");
        field1.parent_id = Some("s1".to_string());

        let mut field2 = make_symbol_with_lang("f2", "score", SymbolKind::Field, "rust");
        field2.parent_id = Some("s1".to_string());

        let mut field3 = make_symbol_with_lang("f3", "file_path", SymbolKind::Field, "rust");
        field3.parent_id = Some("s1".to_string());

        let symbols = vec![struct_sym, field1, field2, field3];
        let batch = prepare_batch_for_embedding(&symbols);

        let struct_entry = batch.iter().find(|(id, _)| id == "s1").unwrap();
        assert!(
            struct_entry.1.contains("score"),
            "Struct embedding should include child field name 'score': {}",
            struct_entry.1
        );
        assert!(
            struct_entry.1.contains("file_path"),
            "Struct embedding should include child field name 'file_path': {}",
            struct_entry.1
        );
    }

    // =========================================================================
    // Enum variant enrichment
    // =========================================================================

    #[test]
    fn test_prepare_batch_enriches_enum_with_variants() {
        // Enum with EnumMember children should be enriched with variant names
        let enum_sym = make_symbol_with_lang("e1", "SymbolKind", SymbolKind::Enum, "rust");

        let mut v1 = make_symbol_with_lang("v1", "Class", SymbolKind::EnumMember, "rust");
        v1.parent_id = Some("e1".to_string());

        let mut v2 = make_symbol_with_lang("v2", "Function", SymbolKind::EnumMember, "rust");
        v2.parent_id = Some("e1".to_string());

        let mut v3 = make_symbol_with_lang("v3", "Interface", SymbolKind::EnumMember, "rust");
        v3.parent_id = Some("e1".to_string());

        let symbols = vec![enum_sym, v1, v2, v3];
        let batch = prepare_batch_for_embedding(&symbols);

        // Only the enum itself should be embedded (EnumMember is not an embeddable kind)
        assert_eq!(batch.len(), 1);

        let enum_entry = &batch[0];
        assert!(
            enum_entry.1.contains("variants:"),
            "Enum embedding should use 'variants:' label: {}",
            enum_entry.1
        );
        assert!(
            enum_entry.1.contains("Class"),
            "Enum embedding should include variant 'Class': {}",
            enum_entry.1
        );
        assert!(
            enum_entry.1.contains("Function"),
            "Enum embedding should include variant 'Function': {}",
            enum_entry.1
        );
        assert!(
            enum_entry.1.contains("Interface"),
            "Enum embedding should include variant 'Interface': {}",
            enum_entry.1
        );
    }

    #[test]
    fn test_prepare_batch_csharp_enum_with_members() {
        // C# enum with EnumMember children
        let enum_sym = make_symbol_with_lang("e1", "UserRole", SymbolKind::Enum, "csharp");

        let mut v1 = make_symbol_with_lang("v1", "Admin", SymbolKind::EnumMember, "csharp");
        v1.parent_id = Some("e1".to_string());

        let mut v2 = make_symbol_with_lang("v2", "Editor", SymbolKind::EnumMember, "csharp");
        v2.parent_id = Some("e1".to_string());

        let mut v3 = make_symbol_with_lang("v3", "Viewer", SymbolKind::EnumMember, "csharp");
        v3.parent_id = Some("e1".to_string());

        let symbols = vec![enum_sym, v1, v2, v3];
        let batch = prepare_batch_for_embedding(&symbols);

        assert_eq!(batch.len(), 1);
        let enum_entry = &batch[0];
        assert!(
            enum_entry.1.contains("Admin"),
            "C# enum should include member 'Admin': {}",
            enum_entry.1
        );
        assert!(
            enum_entry.1.contains("Viewer"),
            "C# enum should include member 'Viewer': {}",
            enum_entry.1
        );
    }

    // =========================================================================
    // Truncation limit validation
    // =========================================================================

    #[test]
    fn test_enriched_container_preserves_more_content_within_limit() {
        // A class with many properties should retain most of them within the metadata limit.
        // This test verifies the limit is generous enough for enriched containers.
        let class_sym = make_symbol_with_lang("c1", "UserProfile", SymbolKind::Class, "csharp");

        let prop_names = [
            "Id",
            "FirstName",
            "LastName",
            "Email",
            "PhoneNumber",
            "Department",
            "Title",
            "IsActive",
            "CreatedAt",
            "UpdatedAt",
            "Manager",
            "TeamId",
        ];
        let mut symbols = vec![class_sym];
        for (i, name) in prop_names.iter().enumerate() {
            let mut prop =
                make_symbol_with_lang(&format!("p{i}"), name, SymbolKind::Property, "csharp");
            prop.parent_id = Some("c1".to_string());
            symbols.push(prop);
        }

        let batch = prepare_batch_for_embedding(&symbols);
        let class_entry = &batch[0];

        // With a reasonable limit, a class with 12 properties should retain most of them
        // (12 short property names ≈ 120 chars for "properties: Id, FirstName, ...")
        assert!(
            class_entry.1.contains("UpdatedAt"),
            "Should retain properties near end of list — limit should be generous enough: {}",
            class_entry.1
        );
    }

    // =========================================================================
    // first_sentence extraction
    // =========================================================================

    #[test]
    fn test_format_strips_xml_doc_tags_csharp() {
        // C# XML doc comments have <summary> tags on separate lines
        let sym = make_symbol(
            "id_xml",
            "LuceneIndexService",
            SymbolKind::Class,
            Some("public class LuceneIndexService : ILuceneIndexService"),
            Some(
                "/// <summary>\n/// Thread-safe Lucene index service with centralized architecture support\n/// </summary>",
            ),
        );
        let text = format_symbol_metadata(&sym);
        assert!(
            text.contains("Thread-safe Lucene index service"),
            "Should extract actual description, not XML tags: {text}"
        );
        assert!(
            !text.contains("<summary>"),
            "Should not contain XML tags: {text}"
        );
    }

    #[test]
    fn test_format_strips_inline_xml_tags() {
        // C# doc comment with inline <see cref="..."/> tags
        let sym = make_symbol(
            "id_xml2",
            "ProcessPayment",
            SymbolKind::Method,
            None,
            Some(
                "/// Processes a <see cref=\"Payment\"/> using the <see cref=\"IPaymentGateway\"/> service.",
            ),
        );
        let text = format_symbol_metadata(&sym);
        assert!(
            text.contains("Processes a"),
            "Should preserve text around XML tags: {text}"
        );
        assert!(
            !text.contains("<see"),
            "Should strip inline XML tags: {text}"
        );
    }

    #[test]
    fn test_first_sentence_extraction() {
        let sym = make_symbol(
            "id11",
            "foo",
            SymbolKind::Function,
            None,
            Some("/// Handles authentication. Also does authorization and logging."),
        );
        let text = format_symbol_metadata(&sym);
        assert!(
            text.contains("Handles authentication."),
            "Should extract first sentence: {text}"
        );
        assert!(
            !text.contains("Also does"),
            "Should not include second sentence: {text}"
        );
    }
}
