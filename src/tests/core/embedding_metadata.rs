//! Tests for symbol metadata formatting (embeddings::metadata).

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::embeddings::metadata::{
        VariableEmbeddingPolicy, format_symbol_metadata, has_simple_default_literal,
        is_embeddable_kind, is_embeddable_language, is_test_symbol_for_embedding,
        prepare_batch_for_embedding, select_budgeted_variables,
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

        let batch = prepare_batch_for_embedding(&symbols, None, &HashMap::new(), &HashMap::new());
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

        let batch = prepare_batch_for_embedding(&symbols, None, &HashMap::new(), &HashMap::new());
        assert_eq!(batch.len(), 3, "Should filter to 3 embeddable symbols");

        let ids: Vec<&str> = batch.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, vec!["s1", "s3", "s5"]);
    }

    #[test]
    fn test_prepare_batch_empty_input() {
        let batch = prepare_batch_for_embedding(&[], None, &HashMap::new(), &HashMap::new());
        assert!(batch.is_empty());
    }

    #[test]
    fn test_prepare_batch_all_non_embeddable() {
        let symbols = vec![
            make_symbol("s1", "x", SymbolKind::Variable, None, None),
            make_symbol("s2", "Y", SymbolKind::Constant, None, None),
            make_symbol("s3", "z", SymbolKind::Import, None, None),
        ];

        let batch = prepare_batch_for_embedding(&symbols, None, &HashMap::new(), &HashMap::new());
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
        let batch = prepare_batch_for_embedding(&symbols, None, &HashMap::new(), &HashMap::new());

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
        let batch = prepare_batch_for_embedding(&symbols, None, &HashMap::new(), &HashMap::new());

        let iface_entry = batch.iter().find(|(id, _)| id == "i1").unwrap();
        assert!(
            iface_entry.1.contains("Search"),
            "Interface embedding should include child method names: {}",
            iface_entry.1
        );
    }

    #[test]
    fn test_prepare_batch_no_enrichment_for_functions_without_callees() {
        let func = make_symbol("f1", "standalone_func", SymbolKind::Function, None, None);

        let symbols = vec![func];
        let batch = prepare_batch_for_embedding(&symbols, None, &HashMap::new(), &HashMap::new());

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

        // Create 50 child methods with long names to exceed the 1200-char budget
        let mut symbols = vec![class_sym];
        for i in 0..50 {
            let mut method = make_symbol_with_lang(
                &format!("m{i}"),
                &format!("VeryLongMethodNameNumbered{i}ForComprehensiveTesting"),
                SymbolKind::Method,
                "rust",
            );
            method.parent_id = Some("c1".to_string());
            symbols.push(method);
        }

        let batch = prepare_batch_for_embedding(&symbols, None, &HashMap::new(), &HashMap::new());
        let class_entry = batch.iter().find(|(id, _)| id == "c1").unwrap();

        assert!(
            class_entry.1.len() <= 1200,
            "Enriched text should be within 1200 chars, got {}",
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
        let batch = prepare_batch_for_embedding(&symbols, None, &HashMap::new(), &HashMap::new());

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
        let batch = prepare_batch_for_embedding(&symbols, None, &HashMap::new(), &HashMap::new());

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
        let batch = prepare_batch_for_embedding(&symbols, None, &HashMap::new(), &HashMap::new());

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
        let batch = prepare_batch_for_embedding(&symbols, None, &HashMap::new(), &HashMap::new());

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
        let batch = prepare_batch_for_embedding(&symbols, None, &HashMap::new(), &HashMap::new());

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
        let batch = prepare_batch_for_embedding(&symbols, None, &HashMap::new(), &HashMap::new());

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

        let batch = prepare_batch_for_embedding(&symbols, None, &HashMap::new(), &HashMap::new());
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
    fn test_format_includes_multi_sentence_doc_excerpt() {
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
            "Should include first sentence: {text}"
        );
        assert!(
            text.contains("Also does authorization"),
            "Should now include subsequent sentences: {text}"
        );
    }

    #[test]
    fn test_format_includes_multiple_doc_sentences() {
        let sym = make_symbol(
            "id_multi_doc",
            "record_tool_call",
            SymbolKind::Method,
            Some("pub(crate) fn record_tool_call(&self, tool_name: &str, duration: Duration, report: &ToolCallReport)"),
            Some("/// Record a completed tool call. Bumps in-memory atomics synchronously, then spawns async task for source_bytes lookup + SQLite write."),
        );
        let text = format_symbol_metadata(&sym);
        assert!(
            text.contains("SQLite write"),
            "Should include second sentence with database signal: {text}"
        );
        assert!(
            text.contains("Record a completed tool call"),
            "Should still include first sentence: {text}"
        );
    }

    // =========================================================================
    // select_budgeted_variables
    // =========================================================================

    #[test]
    fn test_select_budgeted_variables_returns_empty_when_policy_disabled_or_zero_cap() {
        let symbols = vec![make_symbol(
            "var_1",
            "customer_id",
            SymbolKind::Variable,
            Some("let customer_id = request.customer_id;"),
            None,
        )];
        let reference_scores = HashMap::from([("var_1".to_string(), 0.90_f64)]);

        let disabled = VariableEmbeddingPolicy {
            enabled: false,
            max_ratio: 1.0,
        };
        let zero_cap = VariableEmbeddingPolicy {
            enabled: true,
            max_ratio: 0.0,
        };

        assert!(
            select_budgeted_variables(&symbols, &reference_scores, 10, &disabled, None).is_empty(),
            "Disabled policy should return no variables"
        );
        assert!(
            select_budgeted_variables(&symbols, &reference_scores, 10, &zero_cap, None).is_empty(),
            "Zero cap policy should return no variables"
        );
    }

    #[test]
    fn test_select_budgeted_variables_only_considers_variable_symbols() {
        let symbols = vec![
            make_symbol("fn_1", "process_order", SymbolKind::Function, None, None),
            make_symbol(
                "var_1",
                "order_total",
                SymbolKind::Variable,
                Some("let order_total = line_items.sum();"),
                None,
            ),
            make_symbol("class_1", "OrderService", SymbolKind::Class, None, None),
        ];

        let reference_scores = HashMap::from([
            ("fn_1".to_string(), 0.99_f64),
            ("var_1".to_string(), 0.10_f64),
            ("class_1".to_string(), 0.99_f64),
        ]);

        let policy = VariableEmbeddingPolicy {
            enabled: true,
            max_ratio: 1.0,
        };

        let selected = select_budgeted_variables(&symbols, &reference_scores, 1, &policy, None);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].0, "var_1");
        assert!(selected[0].1.contains("order_total"));
    }

    #[test]
    fn test_select_budgeted_variables_includes_reference_score_contribution() {
        let symbols = vec![
            make_symbol(
                "var_low_ref",
                "customer_status",
                SymbolKind::Variable,
                Some("let customer_status = fetch_customer_status(user);"),
                None,
            ),
            make_symbol(
                "var_high_ref",
                "customer_status_cached",
                SymbolKind::Variable,
                Some("let customer_status_cached = fetch_customer_status(user);"),
                None,
            ),
        ];

        let reference_scores = HashMap::from([
            ("var_low_ref".to_string(), 0.10_f64),
            ("var_high_ref".to_string(), 0.95_f64),
        ]);

        let policy = VariableEmbeddingPolicy {
            enabled: true,
            max_ratio: 1.0,
        };

        let selected = select_budgeted_variables(&symbols, &reference_scores, 1, &policy, None);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].0, "var_high_ref");
    }

    #[test]
    fn test_select_budgeted_variables_boosts_descriptive_names() {
        // Descriptive names (snake_case or length >= 12) get a +0.15 boost
        // over short, non-descriptive identifiers.
        let symbols = vec![
            make_symbol(
                "var_descriptive",
                "order_total",
                SymbolKind::Variable,
                Some("let order_total = line_items.sum();"),
                None,
            ),
            make_symbol(
                "var_short",
                "state",
                SymbolKind::Variable,
                Some("let state = 0;"),
                None,
            ),
        ];

        let policy = VariableEmbeddingPolicy {
            enabled: true,
            max_ratio: 1.0,
        };

        let selected = select_budgeted_variables(&symbols, &HashMap::new(), 1, &policy, None);
        assert_eq!(selected.len(), 1);
        assert_eq!(
            selected[0].0, "var_descriptive",
            "Snake_case name should receive descriptiveness boost over short name with default penalty"
        );
    }

    #[test]
    fn test_select_budgeted_variables_penalizes_noise_variables() {
        let symbols = vec![
            make_symbol(
                "var_noise",
                "i",
                SymbolKind::Variable,
                Some("let i = 0;"),
                None,
            ),
            make_symbol(
                "var_signal",
                "customer_id",
                SymbolKind::Variable,
                Some("let customer_id = request.customer_id;"),
                None,
            ),
        ];

        let reference_scores = HashMap::from([
            ("var_noise".to_string(), 0.40_f64),
            ("var_signal".to_string(), 0.40_f64),
        ]);
        let policy = VariableEmbeddingPolicy {
            enabled: true,
            max_ratio: 1.0,
        };

        let selected = select_budgeted_variables(&symbols, &reference_scores, 1, &policy, None);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].0, "var_signal");
    }

    #[test]
    fn test_select_budgeted_variables_prefers_high_signal_over_local_low_signal() {
        let local_low_signal = make_symbol(
            "var_local",
            "i",
            SymbolKind::Variable,
            Some("let i = 0;"),
            None,
        );
        let high_signal = make_symbol(
            "var_high",
            "customer_credit_score",
            SymbolKind::Variable,
            Some("let customer_credit_score = risk_model.compute(user);"),
            None,
        );

        let symbols = vec![local_low_signal, high_signal];
        let reference_scores = HashMap::from([
            ("var_local".to_string(), 0.05_f64),
            ("var_high".to_string(), 0.95_f64),
        ]);
        let policy = VariableEmbeddingPolicy {
            enabled: true,
            max_ratio: 1.0,
        };

        let selected = select_budgeted_variables(&symbols, &reference_scores, 1, &policy, None);
        assert_eq!(selected.len(), 1, "Expected exactly one selected variable");
        assert_eq!(selected[0].0, "var_high");
    }

    #[test]
    fn test_select_budgeted_variables_enforces_budget_cap() {
        let symbols = vec![
            make_symbol("var_1", "alpha", SymbolKind::Variable, None, None),
            make_symbol("var_2", "beta", SymbolKind::Variable, None, None),
            make_symbol("var_3", "gamma", SymbolKind::Variable, None, None),
            make_symbol("var_4", "delta", SymbolKind::Variable, None, None),
            make_symbol("var_5", "epsilon", SymbolKind::Variable, None, None),
        ];
        let reference_scores = HashMap::from([
            ("var_1".to_string(), 0.90_f64),
            ("var_2".to_string(), 0.80_f64),
            ("var_3".to_string(), 0.70_f64),
            ("var_4".to_string(), 0.60_f64),
            ("var_5".to_string(), 0.50_f64),
        ]);
        let policy = VariableEmbeddingPolicy {
            enabled: true,
            max_ratio: 0.20,
        };

        let base_count = 11;
        let cap = ((base_count as f64) * policy.max_ratio).floor() as usize;
        let selected = select_budgeted_variables(&symbols, &reference_scores, base_count, &policy, None);

        assert_eq!(
            selected.len(),
            cap,
            "Expected selection to fill cap when enough candidates exist"
        );
        assert!(
            selected.len() <= cap,
            "Selected {} variables but cap is {}",
            selected.len(),
            cap
        );
    }

    #[test]
    fn test_select_budgeted_variables_tie_breaks_deterministically_by_score_then_id() {
        let symbols = vec![
            make_symbol("var_b", "beta", SymbolKind::Variable, None, None),
            make_symbol("var_top", "top", SymbolKind::Variable, None, None),
            make_symbol("var_a", "alpha", SymbolKind::Variable, None, None),
        ];
        let reference_scores = HashMap::from([
            ("var_b".to_string(), 0.50_f64),
            ("var_top".to_string(), 0.90_f64),
            ("var_a".to_string(), 0.50_f64),
        ]);
        let policy = VariableEmbeddingPolicy {
            enabled: true,
            max_ratio: 1.0,
        };

        let selected = select_budgeted_variables(&symbols, &reference_scores, 3, &policy, None);
        let selected_ids: Vec<&str> = selected.iter().map(|(id, _)| id.as_str()).collect();

        assert_eq!(selected_ids, vec!["var_top", "var_a", "var_b"]);
    }

    #[test]
    fn test_select_budgeted_variables_descriptiveness_uses_name_structure() {
        // The descriptiveness heuristic checks for `_` or length >= 12
        // in the *name* — it doesn't inspect content or tokens.
        let symbols = vec![
            make_symbol(
                "var_short",
                "rapidly",
                SymbolKind::Variable,
                Some("let rapidly = compute();"),
                None,
            ),
            make_symbol(
                "var_snake",
                "state_value",
                SymbolKind::Variable,
                Some("let state_value = load_state();"),
                None,
            ),
        ];

        let reference_scores = HashMap::from([
            ("var_short".to_string(), 0.40_f64),
            ("var_snake".to_string(), 0.40_f64),
        ]);

        let policy = VariableEmbeddingPolicy {
            enabled: true,
            max_ratio: 1.0,
        };

        let selected = select_budgeted_variables(&symbols, &reference_scores, 1, &policy, None);
        assert_eq!(selected.len(), 1);
        assert_eq!(
            selected[0].0, "var_snake",
            "Snake_case name gets descriptiveness boost; short single-word name does not"
        );
    }

    #[test]
    fn test_select_budgeted_variables_boosts_both_camel_and_snake_case_descriptive_names() {
        // Both naming conventions qualify for the descriptiveness boost:
        // camelCase via length >= 12, snake_case via `_` in name.
        let symbols = vec![
            make_symbol(
                "var_camel",
                "connectionPool",
                SymbolKind::Variable,
                Some("let connectionPool = create_pool();"),
                None,
            ),
            make_symbol(
                "var_snake",
                "connection_pool",
                SymbolKind::Variable,
                Some("let connection_pool = create_pool();"),
                None,
            ),
            make_symbol(
                "var_short",
                "pool",
                SymbolKind::Variable,
                Some("let pool = get();"),
                None,
            ),
        ];

        let reference_scores = HashMap::from([
            ("var_camel".to_string(), 0.30_f64),
            ("var_snake".to_string(), 0.30_f64),
            ("var_short".to_string(), 0.30_f64),
        ]);

        let policy = VariableEmbeddingPolicy {
            enabled: true,
            max_ratio: 1.0,
        };

        let selected = select_budgeted_variables(&symbols, &reference_scores, 2, &policy, None);
        let selected_ids: Vec<&str> = selected.iter().map(|(id, _)| id.as_str()).collect();

        assert_eq!(selected_ids.len(), 2);
        assert!(
            selected_ids.contains(&"var_camel"),
            "camelCase name (len>=12) should get descriptiveness boost"
        );
        assert!(
            selected_ids.contains(&"var_snake"),
            "snake_case name (has _) should get descriptiveness boost"
        );
    }

    #[test]
    fn test_select_budgeted_variables_handles_non_english_identifier_and_docs() {
        let symbols = vec![
            make_symbol(
                "var_non_english",
                "estadoUsuario",
                SymbolKind::Variable,
                Some("let estadoUsuario = obtener_estado(usuario);"),
                Some("/// Devuelve el estado actual del usuario."),
            ),
            make_symbol(
                "var_ascii",
                "state_cache",
                SymbolKind::Variable,
                Some("let state_cache = load_state();"),
                Some("/// Stores cached state."),
            ),
        ];

        let reference_scores = HashMap::from([
            ("var_non_english".to_string(), 0.50_f64),
            ("var_ascii".to_string(), 0.49_f64),
        ]);

        let policy = VariableEmbeddingPolicy {
            enabled: true,
            max_ratio: 1.0,
        };

        let selected = select_budgeted_variables(&symbols, &reference_scores, 1, &policy, None);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].0, "var_non_english");
    }

    #[test]
    fn test_select_budgeted_variables_penalizes_default_values_across_signature_styles() {
        let symbols = vec![
            make_symbol(
                "var_default_equals_spaced",
                "configValue",
                SymbolKind::Variable,
                Some("config_value = false"),
                None,
            ),
            make_symbol(
                "var_default_equals_compact",
                "limitValue",
                SymbolKind::Variable,
                Some("limit_value=false"),
                None,
            ),
            make_symbol(
                "var_default_colon_equals",
                "modeValue",
                SymbolKind::Variable,
                Some("mode_value:=0"),
                None,
            ),
            make_symbol(
                "var_no_default",
                "resultValue",
                SymbolKind::Variable,
                Some("result_value: bool"),
                None,
            ),
        ];

        let reference_scores = HashMap::from([
            ("var_default_equals_spaced".to_string(), 0.40_f64),
            ("var_default_equals_compact".to_string(), 0.40_f64),
            ("var_default_colon_equals".to_string(), 0.40_f64),
            ("var_no_default".to_string(), 0.40_f64),
        ]);

        let policy = VariableEmbeddingPolicy {
            enabled: true,
            max_ratio: 1.0,
        };

        let selected = select_budgeted_variables(&symbols, &reference_scores, 1, &policy, None);
        assert_eq!(selected.len(), 1);
        assert_eq!(
            selected[0].0, "var_no_default",
            "Mixed default styles should all receive the same penalty"
        );
    }

    #[test]
    fn test_select_budgeted_variables_noise_penalty_no_double_dip_with_short_name() {
        // Regression test: a known noise name like "i" is BOTH in NOISE_NAMES AND
        // has len <= 2.  The old code applied BOTH penalties (0.50 + 0.15 = 0.65),
        // but they should be mutually exclusive (just 0.50).
        //
        // With ref_score = 0.55:
        //   Fixed code:  0.55 - 0.50 = 0.05  → beats baseline (0.0) → selected
        //   Old code:    0.55 - 0.65 = -0.10  → loses to baseline  → NOT selected
        let symbols = vec![
            make_symbol(
                "var_noise",
                "i",
                SymbolKind::Variable,
                Some("let i = get_index();"), // non-default signature to avoid extra penalty
                None,
            ),
            make_symbol(
                "var_baseline",
                "count",
                SymbolKind::Variable,
                Some("let count = tally();"),
                None,
            ),
        ];

        let reference_scores = HashMap::from([
            ("var_noise".to_string(), 0.55_f64),
            ("var_baseline".to_string(), 0.0_f64),
        ]);
        let policy = VariableEmbeddingPolicy {
            enabled: true,
            max_ratio: 1.0,
        };

        let selected = select_budgeted_variables(&symbols, &reference_scores, 2, &policy, None);
        // Both should be selected, but "i" must rank FIRST (higher score).
        // Under the old double-dip bug, "i" would score -0.10 and rank below baseline.
        assert_eq!(selected.len(), 2);
        assert_eq!(
            selected[0].0, "var_noise",
            "Noise name 'i' with ref_score=0.55 should rank first (penalty=0.50, score=0.05); \
             double-dip would give penalty=0.65, score=-0.10 and push it below baseline"
        );
    }

    #[test]
    fn test_select_budgeted_variables_unknown_short_name_gets_smaller_penalty() {
        // A 2-character name NOT in NOISE_NAMES should get the short-name penalty
        // (0.20), not the noise-name penalty (0.50).
        //
        // "mx" with ref_score = 0.25:
        //   Correct (0.20 penalty): 0.25 - 0.20 = 0.05  → beats baseline
        //   Wrong (0.50 penalty):   0.25 - 0.50 = -0.25  → loses to baseline
        let symbols = vec![
            make_symbol(
                "var_short",
                "mx",
                SymbolKind::Variable,
                Some("let mx = compute_max();"),
                None,
            ),
            make_symbol(
                "var_baseline",
                "total",
                SymbolKind::Variable,
                Some("let total = sum();"),
                None,
            ),
        ];

        let reference_scores = HashMap::from([
            ("var_short".to_string(), 0.25_f64),
            ("var_baseline".to_string(), 0.0_f64),
        ]);
        let policy = VariableEmbeddingPolicy {
            enabled: true,
            max_ratio: 1.0,
        };

        let selected = select_budgeted_variables(&symbols, &reference_scores, 2, &policy, None);
        assert_eq!(selected.len(), 2);
        assert_eq!(
            selected[0].0, "var_short",
            "Unknown short name 'mx' should get -0.20 penalty (score=0.05), not -0.50 (score=-0.25); \
             it should rank above the zero-score baseline"
        );
    }

    // ---- has_simple_default_literal unit tests ----

    #[test]
    fn test_has_simple_default_literal_matches() {
        // Numeric defaults
        assert!(has_simple_default_literal("let x = 0"));
        assert!(has_simple_default_literal("let x = 1"));
        assert!(has_simple_default_literal("let x = 0;"));
        // Boolean / null-ish defaults
        assert!(has_simple_default_literal("x = true"));
        assert!(has_simple_default_literal("x = false"));
        assert!(has_simple_default_literal("x = None"));
        assert!(has_simple_default_literal("x = null"));
        assert!(has_simple_default_literal("x = nil"));
        assert!(has_simple_default_literal("x = True"));
        assert!(has_simple_default_literal("x = FALSE"));
        // Empty collection / string defaults
        assert!(has_simple_default_literal("x = \"\""));
        assert!(has_simple_default_literal("x = ''"));
        assert!(has_simple_default_literal("x = {}"));
        assert!(has_simple_default_literal("x = []"));
    }

    #[test]
    fn test_has_simple_default_literal_rejects_comparison_operators() {
        assert!(!has_simple_default_literal("x == 0"));
        assert!(!has_simple_default_literal("x != 0"));
        assert!(!has_simple_default_literal("x >= 0"));
        assert!(!has_simple_default_literal("x <= 0"));
        assert!(!has_simple_default_literal("if x == true"));
        assert!(!has_simple_default_literal("x != null"));
    }

    #[test]
    fn test_has_simple_default_literal_rejects_non_defaults() {
        assert!(!has_simple_default_literal("x = some_function()"));
        assert!(!has_simple_default_literal("x = truthy"));
        assert!(!has_simple_default_literal("x = none_value"));
        assert!(!has_simple_default_literal("x = 0x1234"));
        assert!(!has_simple_default_literal("x = 42"));
        assert!(!has_simple_default_literal("no assignment here"));
    }

    // =========================================================================
    // Test symbol exclusion from embeddings
    // =========================================================================

    #[test]
    fn test_is_test_symbol_by_metadata() {
        let mut sym = make_symbol("t1", "test_add", SymbolKind::Function, None, None);
        sym.metadata = Some(HashMap::from([(
            "is_test".to_string(),
            serde_json::Value::Bool(true),
        )]));

        assert!(is_test_symbol_for_embedding(&sym));
    }

    #[test]
    fn test_is_test_symbol_by_path() {
        let mut sym = make_symbol("t2", "MyHelper", SymbolKind::Class, None, None);
        sym.file_path = "test/helpers/my_helper.rb".to_string();

        assert!(is_test_symbol_for_embedding(&sym));
    }

    #[test]
    fn test_is_test_symbol_by_path_csharp_convention() {
        let mut sym = make_symbol("t3", "SerializerTests", SymbolKind::Class, None, None);
        sym.file_path = "MyProject.Tests/SerializerTests.cs".to_string();

        assert!(is_test_symbol_for_embedding(&sym));
    }

    #[test]
    fn test_is_not_test_symbol_for_source_code() {
        let sym = make_symbol("s1", "Router", SymbolKind::Module, None, None);
        assert!(!is_test_symbol_for_embedding(&sym));
    }

    #[test]
    fn test_is_not_test_symbol_metadata_false() {
        let mut sym = make_symbol("s2", "run", SymbolKind::Function, None, None);
        sym.metadata = Some(HashMap::from([(
            "is_test".to_string(),
            serde_json::Value::Bool(false),
        )]));

        assert!(!is_test_symbol_for_embedding(&sym));
    }

    #[test]
    fn test_prepare_batch_excludes_test_symbols() {
        let mut test_func = make_symbol("t1", "test_add", SymbolKind::Function, None, None);
        test_func.metadata = Some(HashMap::from([(
            "is_test".to_string(),
            serde_json::Value::Bool(true),
        )]));

        let mut test_class = make_symbol("t2", "RouterTest", SymbolKind::Class, None, None);
        test_class.file_path = "tests/router_test.rs".to_string();

        let source_func = make_symbol("s1", "add", SymbolKind::Function, None, None);
        let source_class = make_symbol("s2", "Router", SymbolKind::Class, None, None);

        let symbols = vec![test_func, test_class, source_func, source_class];
        let batch = prepare_batch_for_embedding(&symbols, None, &HashMap::new(), &HashMap::new());

        assert_eq!(batch.len(), 2, "Should exclude both test symbols");
        let ids: Vec<&str> = batch.iter().map(|(id, _)| id.as_str()).collect();
        assert!(ids.contains(&"s1"));
        assert!(ids.contains(&"s2"));
        assert!(!ids.contains(&"t1"));
        assert!(!ids.contains(&"t2"));
    }

    #[test]
    fn test_select_budgeted_variables_excludes_test_variables() {
        let source_var = make_symbol("v1", "config_path", SymbolKind::Variable, None, None);

        let mut test_var = make_symbol("v2", "test_config", SymbolKind::Variable, None, None);
        test_var.file_path = "tests/test_config.rs".to_string();

        let mut test_var_meta = make_symbol("v3", "mock_data", SymbolKind::Variable, None, None);
        test_var_meta.metadata = Some(HashMap::from([(
            "is_test".to_string(),
            serde_json::Value::Bool(true),
        )]));

        let symbols = vec![source_var, test_var, test_var_meta];
        let ref_scores = HashMap::new();
        let policy = VariableEmbeddingPolicy {
            enabled: true,
            max_ratio: 1.0, // generous budget to not cap
        };

        let selected = select_budgeted_variables(&symbols, &ref_scores, 10, &policy, None);

        assert_eq!(selected.len(), 1, "Should only include source variable");
        assert_eq!(selected[0].0, "v1");
    }

    // =========================================================================
    // Callee enrichment for functions/methods
    // =========================================================================

    #[test]
    fn test_prepare_batch_enriches_function_with_callees() {
        let func = make_symbol(
            "f1",
            "record_tool_call",
            SymbolKind::Function,
            Some("pub fn record_tool_call(&self, tool_name: &str)"),
            Some("/// Record a completed tool call."),
        );
        let callee_func = make_symbol(
            "f2",
            "insert_tool_call",
            SymbolKind::Function,
            None,
            None,
        );
        let callee_func2 = make_symbol(
            "f3",
            "get_total_file_sizes",
            SymbolKind::Function,
            None,
            None,
        );

        let symbols = vec![func, callee_func, callee_func2];

        let mut callees_by_symbol: HashMap<String, Vec<String>> = HashMap::new();
        callees_by_symbol.insert(
            "f1".to_string(),
            vec!["insert_tool_call".to_string(), "get_total_file_sizes".to_string()],
        );

        let batch = prepare_batch_for_embedding(&symbols, None, &callees_by_symbol, &HashMap::new());
        assert_eq!(batch.len(), 3);

        let (_, text) = batch.iter().find(|(id, _)| id == "f1").unwrap();
        assert!(
            text.contains("calls:"),
            "Function should have callee enrichment: {text}"
        );
        assert!(
            text.contains("insert_tool_call"),
            "Should contain callee name: {text}"
        );
        assert!(
            text.contains("get_total_file_sizes"),
            "Should contain second callee name: {text}"
        );
    }

    #[test]
    fn test_prepare_batch_enriches_method_with_callees() {
        let method = make_symbol(
            "m1",
            "process",
            SymbolKind::Method,
            Some("pub fn process(&self)"),
            None,
        );
        let symbols = vec![method];
        let mut callees = HashMap::new();
        callees.insert("m1".to_string(), vec!["save".to_string(), "validate".to_string()]);

        let batch = prepare_batch_for_embedding(&symbols, None, &callees, &HashMap::new());
        let (_, text) = &batch[0];
        assert!(
            text.contains("calls: save, validate"),
            "Method should have sorted callee enrichment: {text}"
        );
    }

    #[test]
    fn test_prepare_batch_container_no_callee_enrichment() {
        let class = make_symbol_with_lang("c1", "MyService", SymbolKind::Class, "csharp");
        let symbols = vec![class];
        let mut callees = HashMap::new();
        callees.insert("c1".to_string(), vec!["something".to_string()]);

        let batch = prepare_batch_for_embedding(&symbols, None, &callees, &HashMap::new());
        let (_, text) = &batch[0];
        assert!(
            !text.contains("calls:"),
            "Container symbols should NOT get callee enrichment: {text}"
        );
    }

    #[test]
    fn test_enriched_function_with_callees_uses_expanded_budget() {
        let long_doc = "/// Orchestrates a complex multi-stage data processing pipeline that coordinates extraction from multiple sources. Manages transformation rules, validates intermediate results against business constraints, and loads final output into the target database system. Implements comprehensive retry logic for transient failures with exponential backoff.";
        let func = make_symbol(
            "f1",
            "orchestrate_complex_pipeline",
            SymbolKind::Function,
            Some("pub async fn orchestrate_complex_pipeline(handler: &JulieServerHandler, config: &PipelineConfig, options: &ProcessingOptions) -> Result<PipelineOutput>"),
            Some(long_doc),
        );
        let symbols = vec![func];
        let mut callees = HashMap::new();
        callees.insert("f1".to_string(), vec![
            "connect_to_source_database".to_string(),
            "extract_source_records".to_string(),
            "transform_with_business_rules".to_string(),
            "validate_intermediate_output".to_string(),
            "load_into_target_database".to_string(),
            "retry_with_exponential_backoff".to_string(),
        ]);

        let batch = prepare_batch_for_embedding(&symbols, None, &callees, &HashMap::new());
        let (_, text) = &batch[0];

        // Verify the last callee is present (would be truncated at 600 chars)
        assert!(
            text.contains("retry_with_exponential_backoff"),
            "Last callee should not be truncated with expanded budget: {text}"
        );
        assert!(
            text.contains("loads final output"),
            "Multi-sentence doc should survive within budget: {text}"
        );
        assert!(
            text.len() > 600,
            "Text should exceed old 600-char limit: len={}, text: {text}",
            text.len()
        );
    }

    #[test]
    fn test_prepare_batch_enriches_function_with_field_accesses() {
        let func = make_symbol(
            "f1",
            "record_tool_call",
            SymbolKind::Function,
            Some("pub fn record_tool_call(&self, tool_name: &str)"),
            Some("/// Record a completed tool call."),
        );
        let symbols = vec![func];

        let callees_by_symbol: HashMap<String, Vec<String>> = HashMap::new();
        let mut fields_by_symbol: HashMap<String, Vec<String>> = HashMap::new();
        fields_by_symbol.insert(
            "f1".to_string(),
            vec![
                "session_metrics".to_string(),
                "db".to_string(),
                "output_bytes".to_string(),
            ],
        );

        let batch =
            prepare_batch_for_embedding(&symbols, None, &callees_by_symbol, &fields_by_symbol);
        assert_eq!(batch.len(), 1);

        let (_, text) = &batch[0];
        assert!(
            text.contains("fields:"),
            "Function should have field access enrichment: {text}"
        );
        assert!(
            text.contains("session_metrics"),
            "Should contain field name 'session_metrics': {text}"
        );
        assert!(
            text.contains("db"),
            "Should contain field name 'db': {text}"
        );
    }

    #[test]
    fn test_prepare_batch_no_field_enrichment_for_containers() {
        let class = make_symbol_with_lang("c1", "MyService", SymbolKind::Class, "csharp");
        let symbols = vec![class];

        let callees_by_symbol: HashMap<String, Vec<String>> = HashMap::new();
        let mut fields_by_symbol: HashMap<String, Vec<String>> = HashMap::new();
        fields_by_symbol.insert("c1".to_string(), vec!["some_field".to_string()]);

        let batch =
            prepare_batch_for_embedding(&symbols, None, &callees_by_symbol, &fields_by_symbol);
        let (_, text) = &batch[0];

        assert!(
            !text.contains("fields:"),
            "Containers should NOT get field access enrichment (they use properties:): {text}"
        );
    }

    #[test]
    fn test_prepare_batch_field_enrichment_combined_with_callees() {
        let func = make_symbol(
            "f1",
            "process_data",
            SymbolKind::Method,
            Some("pub fn process_data(&self)"),
            None,
        );
        let symbols = vec![func];

        let mut callees_by_symbol: HashMap<String, Vec<String>> = HashMap::new();
        callees_by_symbol.insert("f1".to_string(), vec!["save".to_string()]);

        let mut fields_by_symbol: HashMap<String, Vec<String>> = HashMap::new();
        fields_by_symbol.insert("f1".to_string(), vec!["config".to_string()]);

        let batch =
            prepare_batch_for_embedding(&symbols, None, &callees_by_symbol, &fields_by_symbol);
        let (_, text) = &batch[0];

        assert!(
            text.contains("calls:") && text.contains("fields:"),
            "Should have both callee and field enrichment: {text}"
        );
    }
}
