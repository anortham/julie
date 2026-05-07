// C# Extractor Inline Tests
//
// This module contains tests extracted from src/extractors/csharp/mod.rs.
// Tests verify core functionality of the CSharpExtractor including:
// - Basic class extraction
// - Interface extraction
// - Property extraction
// - Enum extraction with members

use crate::base::{RelationshipKind, SymbolKind};
use crate::csharp::CSharpExtractor;
use std::path::PathBuf;

#[test]
fn test_basic_class_extraction() {
    let code = r#"
        public class MyClass {
            public void MyMethod() {
            }
        }
    "#;

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_c_sharp::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = CSharpExtractor::new(
        "csharp".to_string(),
        "test.cs".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);

    // Should extract namespace, class, and method
    assert!(!symbols.is_empty());
    assert!(symbols.iter().any(|s| s.name == "MyClass"));
    assert!(symbols.iter().any(|s| s.name == "MyMethod"));
}

#[test]
fn test_interface_extraction() {
    let code = r#"
        public interface IMyInterface {
            void DoSomething();
        }
    "#;

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_c_sharp::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = CSharpExtractor::new(
        "csharp".to_string(),
        "test.cs".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);

    assert!(symbols.iter().any(|s| s.name == "IMyInterface"));
}

#[test]
fn test_property_extraction() {
    let code = r#"
        public class MyClass {
            public string Name { get; set; }
        }
    "#;

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_c_sharp::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = CSharpExtractor::new(
        "csharp".to_string(),
        "test.cs".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);

    assert!(symbols.iter().any(|s| s.name == "Name"));
}

#[test]
fn test_enum_extraction() {
    let code = r#"
        public enum Status {
            Active = 1,
            Inactive = 2
        }
    "#;

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_c_sharp::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = CSharpExtractor::new(
        "csharp".to_string(),
        "test.cs".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);

    assert!(symbols.iter().any(|s| s.name == "Status"));
    assert!(symbols.iter().any(|s| s.name == "Active"));
    assert!(symbols.iter().any(|s| s.name == "Inactive"));
}

#[test]
fn test_xml_doc_comment_extraction() {
    let code = r#"
        /// <summary>
        /// Simple utility class to preview MJML email templates locally
        /// Run this to generate HTML files for testing without deployment
        /// </summary>
        public static class EmailTemplatePreview {
            /// <summary>
            /// Renders an email template to HTML
            /// </summary>
            /// <param name="templateName">Name of the template</param>
            public static void RenderTemplate(string templateName) {
            }
        }
    "#;

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_c_sharp::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = CSharpExtractor::new(
        "csharp".to_string(),
        "test.cs".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);

    // Find the class symbol
    let class_symbol = symbols
        .iter()
        .find(|s| s.name == "EmailTemplatePreview")
        .expect("Should find EmailTemplatePreview class");

    // Should have doc comment extracted
    assert!(
        class_symbol.doc_comment.is_some(),
        "Class should have doc_comment populated"
    );

    let doc = class_symbol.doc_comment.as_ref().unwrap();
    assert!(
        doc.contains("Simple utility class"),
        "Doc comment should contain summary text, got: {}",
        doc
    );
    assert!(
        doc.contains("preview MJML email templates"),
        "Doc comment should contain key description text, got: {}",
        doc
    );

    // Find the method symbol
    let method_symbol = symbols
        .iter()
        .find(|s| s.name == "RenderTemplate")
        .expect("Should find RenderTemplate method");

    // Method should also have doc comment
    assert!(
        method_symbol.doc_comment.is_some(),
        "Method should have doc_comment populated"
    );

    let method_doc = method_symbol.doc_comment.as_ref().unwrap();
    assert!(
        method_doc.contains("Renders an email template"),
        "Method doc comment should contain description, got: {}",
        method_doc
    );
}

#[test]
fn test_csharp_local_functions_lambdas_and_partial_classes_are_modeled() {
    let code = r#"
namespace N1
{
    public partial class Widget
    {
        public int Compute(int seed)
        {
            int AddOne(int x) => x + 1;
            Func<int, int> multiplier = x => x * seed;
            Func<int, int> anonymous = delegate(int y) { return y + seed; };
            Func<int, int> assigned;
            assigned = z => z - 1;
            var filtered = new[] { 1, 2, 3 }.Where(v => v > 1).ToList();
            return AddOne(multiplier(seed)) + anonymous(seed) + assigned(seed) + filtered.Count;
        }

        public Func<int, int> CreateMultiplier(int factor) => x => x * factor;
    }
}

namespace N1
{
    public partial class Widget
    {
        public int Other() => 42;
    }
}

namespace N2
{
    public class Widget
    {
        public int Different() => 0;
    }
}

namespace N2
{
    public partial class Widget
    {
        public int PartialOnly() => 1;
    }
}
"#;

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_c_sharp::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = CSharpExtractor::new(
        "csharp".to_string(),
        "test.cs".to_string(),
        code.to_string(),
        &workspace_root,
    );

    let symbols = extractor.extract_symbols(&tree);
    let relationships = extractor.extract_relationships(&tree, &symbols);

    let compute_method = symbols
        .iter()
        .find(|s| s.name == "Compute")
        .expect("Compute method should be extracted");
    let add_one_local = symbols
        .iter()
        .find(|s| s.name == "AddOne")
        .expect("AddOne local function should be extracted");
    assert_eq!(add_one_local.kind, SymbolKind::Function);
    assert_eq!(
        add_one_local.parent_id.as_deref(),
        Some(compute_method.id.as_str()),
        "Local function should be parented to containing method"
    );

    let expected_lambda_names = vec![
        "CreateMultiplier$lambda",
        "anonymous$lambda",
        "assigned$lambda",
        "multiplier$lambda",
    ];
    let mut actual_lambda_names: Vec<String> = symbols
        .iter()
        .filter(|s| s.name.ends_with("$lambda"))
        .map(|s| s.name.clone())
        .collect();
    actual_lambda_names.sort();
    assert_eq!(
        actual_lambda_names, expected_lambda_names,
        "Only stable/useful lambda names should be emitted"
    );
    assert!(
        symbols
            .iter()
            .filter(|s| s.name.ends_with("$lambda"))
            .all(|s| s.kind == SymbolKind::Function),
        "Lambda symbols should be emitted as Function kind"
    );

    let lambda_parent_ids: Vec<&str> = symbols
        .iter()
        .filter(|s| s.name.ends_with("$lambda"))
        .filter_map(|s| s.parent_id.as_deref())
        .collect();
    assert!(
        lambda_parent_ids.contains(&compute_method.id.as_str()),
        "At least one lambda should be parented to Compute"
    );
    assert!(
        !symbols.iter().any(|s| s.name == "v$lambda"),
        "Anonymous argument lambda should not create noisy symbol"
    );

    let other_method = symbols
        .iter()
        .find(|s| s.name == "Other")
        .expect("Other method should be extracted");
    let different_method = symbols
        .iter()
        .find(|s| s.name == "Different")
        .expect("Different method should be extracted");
    let partial_only_method = symbols
        .iter()
        .find(|s| s.name == "PartialOnly")
        .expect("PartialOnly method should be extracted");

    let widget_n1_a = symbols
        .iter()
        .find(|s| Some(s.id.as_str()) == compute_method.parent_id.as_deref())
        .expect("N1.Widget declaration containing Compute should exist");
    let widget_n1_b = symbols
        .iter()
        .find(|s| Some(s.id.as_str()) == other_method.parent_id.as_deref())
        .expect("N1.Widget declaration containing Other should exist");
    let widget_n2_nonpartial = symbols
        .iter()
        .find(|s| Some(s.id.as_str()) == different_method.parent_id.as_deref())
        .expect("N2 non-partial Widget should exist");
    let widget_n2_partial = symbols
        .iter()
        .find(|s| Some(s.id.as_str()) == partial_only_method.parent_id.as_deref())
        .expect("N2 partial Widget should exist");

    let partial_links: Vec<_> = relationships
        .iter()
        .filter(|r| r.kind == RelationshipKind::References)
        .filter(|r| {
            r.metadata
                .as_ref()
                .and_then(|metadata| metadata.get("linkage"))
                .and_then(|v| v.as_str())
                == Some("partial_class")
        })
        .collect();

    let n1_link = partial_links.iter().find(|r| {
        (r.from_symbol_id == widget_n1_a.id && r.to_symbol_id == widget_n1_b.id)
            || (r.from_symbol_id == widget_n1_b.id && r.to_symbol_id == widget_n1_a.id)
    });
    assert!(
        n1_link.is_some(),
        "N1 partial class declarations should be linked"
    );

    let n1_full_name = n1_link
        .and_then(|r| r.metadata.as_ref())
        .and_then(|metadata| metadata.get("partial_full_name"))
        .and_then(|v| v.as_str());
    assert_eq!(
        n1_full_name,
        Some("N1.Widget"),
        "Partial linkage must use full-name invariant"
    );

    let n2_cross_link = partial_links.iter().find(|r| {
        (r.from_symbol_id == widget_n2_nonpartial.id && r.to_symbol_id == widget_n2_partial.id)
            || (r.from_symbol_id == widget_n2_partial.id
                && r.to_symbol_id == widget_n2_nonpartial.id)
    });
    assert!(
        n2_cross_link.is_none(),
        "Non-partial and partial classes with same short name must not be linked"
    );
}
