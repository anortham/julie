// Semantic Search Scoring Tests
//
// Tests for multi-factor scoring enhancements:
// - Doc comment boost
// - Language quality boost
// - Generic symbol detection and penalty
// - End-to-end scoring validation

use crate::extractors::base::{Symbol, SymbolKind, Visibility};
use std::collections::HashMap;

// ============================================================================
// Test Helpers
// ============================================================================

fn create_symbol_with_doc(doc_comment: Option<&str>) -> Symbol {
    Symbol {
        id: "test_id".to_string(),
        name: "TestSymbol".to_string(),
        file_path: "test.cs".to_string(),
        language: "csharp".to_string(),
        kind: SymbolKind::Class,
        signature: Some("class TestSymbol".to_string()),
        start_line: 1,
        start_column: 0,
        end_line: 10,
        end_column: 1,
        start_byte: 0,
        end_byte: 100,
        visibility: Some(Visibility::Public),
        parent_id: None,
        semantic_group: None,
        doc_comment: doc_comment.map(|s| s.to_string()),
        metadata: Some(HashMap::new()),
        confidence: None,
        code_context: None,
    }
}

fn create_symbol(name: &str, doc_comment: Option<&str>) -> Symbol {
    Symbol {
        id: format!("test_id_{}", name),
        name: name.to_string(),
        file_path: "test.cs".to_string(),
        language: "csharp".to_string(),
        kind: SymbolKind::Class,
        signature: Some(format!("class {}", name)),
        start_line: 1,
        start_column: 0,
        end_line: 10,
        end_column: 1,
        start_byte: 0,
        end_byte: 100,
        visibility: Some(Visibility::Public),
        parent_id: None,
        semantic_group: None,
        doc_comment: doc_comment.map(|s| s.to_string()),
        metadata: Some(HashMap::new()),
        confidence: None,
        code_context: None,
    }
}

fn create_html_element_symbol(name: &str) -> Symbol {
    let mut metadata = HashMap::new();
    metadata.insert("type".to_string(), serde_json::Value::String("html-element".to_string()));

    Symbol {
        id: format!("test_id_{}", name),
        name: name.to_string(),
        file_path: "test.razor".to_string(),
        language: "razor".to_string(),
        kind: SymbolKind::Class,
        signature: Some(format!("<{}>", name)),
        start_line: 1,
        start_column: 0,
        end_line: 10,
        end_column: 1,
        start_byte: 0,
        end_byte: 100,
        visibility: Some(Visibility::Public),
        parent_id: None,
        semantic_group: None,
        doc_comment: None,
        metadata: Some(metadata),
        confidence: None,
        code_context: None,
    }
}

// ============================================================================
// Phase 1 Tests - These will FAIL until Phase 2 implementation
// ============================================================================

#[test]
fn test_doc_comment_boost_calculation() {
    use crate::tools::search::semantic_search::get_doc_comment_boost;

    // Symbol with rich documentation (200+ chars)
    let rich_doc = "/// ".to_string() + &"a".repeat(250);
    let symbol_with_rich_docs = create_symbol_with_doc(Some(&rich_doc));
    assert_eq!(
        get_doc_comment_boost(&symbol_with_rich_docs),
        2.0,
        "Rich docs (200+ chars) should get 2.0x boost"
    );

    // Symbol with good documentation (100-200 chars)
    let good_doc = "/// ".to_string() + &"a".repeat(150);
    let symbol_with_good_docs = create_symbol_with_doc(Some(&good_doc));
    assert_eq!(
        get_doc_comment_boost(&symbol_with_good_docs),
        1.5,
        "Good docs (100-200 chars) should get 1.5x boost"
    );

    // Symbol with some documentation (<100 chars)
    let some_doc = "/// Short doc";
    let symbol_with_some_docs = create_symbol_with_doc(Some(some_doc));
    assert_eq!(
        get_doc_comment_boost(&symbol_with_some_docs),
        1.3,
        "Some docs (<100 chars) should get 1.3x boost"
    );

    // Symbol with no documentation
    let symbol_no_docs = create_symbol_with_doc(None);
    assert_eq!(
        get_doc_comment_boost(&symbol_no_docs),
        1.0,
        "No docs should get 1.0x (no boost)"
    );

    // Symbol with empty documentation
    let symbol_empty_docs = create_symbol_with_doc(Some(""));
    assert_eq!(
        get_doc_comment_boost(&symbol_empty_docs),
        1.0,
        "Empty docs should get 1.0x (no boost)"
    );
}

#[test]
fn test_language_quality_boost() {
    use crate::tools::search::semantic_search::get_language_quality_boost;

    // Real code languages - C#
    let csharp_symbol = create_symbol("TestClass", None);
    assert_eq!(
        get_language_quality_boost(&csharp_symbol),
        1.2,
        "C# should get 1.2x boost"
    );

    // Rust
    let mut rust_symbol = create_symbol("TestStruct", None);
    rust_symbol.language = "rust".to_string();
    assert_eq!(
        get_language_quality_boost(&rust_symbol),
        1.2,
        "Rust should get 1.2x boost"
    );

    // JavaScript (scripting language)
    let mut js_symbol = create_symbol("TestFunction", None);
    js_symbol.language = "javascript".to_string();
    assert_eq!(
        get_language_quality_boost(&js_symbol),
        1.1,
        "JavaScript should get 1.1x boost"
    );

    // HTML elements get penalty
    let html_element = create_html_element_symbol("Template");
    assert_eq!(
        get_language_quality_boost(&html_element),
        0.7,
        "HTML elements should get 0.7x penalty"
    );

    // Razor C# code (not HTML) is normal
    let mut razor_code = create_symbol("MyComponent", None);
    razor_code.language = "razor".to_string();
    // Not an HTML element (no metadata)
    assert_eq!(
        get_language_quality_boost(&razor_code),
        1.0,
        "Razor C# code should get 1.0x (neutral)"
    );
}

#[test]
fn test_generic_symbol_detection() {
    use crate::tools::search::semantic_search::{is_generic_symbol, get_generic_penalty};

    // Generic name + no docs = generic
    let template_no_docs = create_symbol("Template", None);
    assert!(
        is_generic_symbol(&template_no_docs),
        "Template with no docs should be detected as generic"
    );
    assert_eq!(
        get_generic_penalty(&template_no_docs),
        0.5,
        "Generic symbols should get 0.5x penalty"
    );

    // Generic name + HAS docs = NOT generic
    let template_with_docs = create_symbol("Template", Some("/// Custom template class for email processing"));
    assert!(
        !is_generic_symbol(&template_with_docs),
        "Template WITH docs should NOT be generic"
    );
    assert_eq!(
        get_generic_penalty(&template_with_docs),
        1.0,
        "Documented symbols should not be penalized"
    );

    // Non-generic name + no docs = NOT generic
    let specific_no_docs = create_symbol("EmailTemplatePreview", None);
    assert!(
        !is_generic_symbol(&specific_no_docs),
        "Specific name should NOT be generic even without docs"
    );
    assert_eq!(
        get_generic_penalty(&specific_no_docs),
        1.0,
        "Specific names should not be penalized"
    );

    // Test all generic names
    let generic_names = vec!["Container", "Wrapper", "Item", "Data", "Value", "Component", "Element"];
    for name in generic_names {
        let symbol = create_symbol(name, None);
        assert!(
            is_generic_symbol(&symbol),
            "{} with no docs should be generic",
            name
        );
    }
}

#[test]
fn test_documented_class_beats_generic_html() {
    use crate::tools::search::semantic_search::apply_all_boosts;

    // EmailTemplatePreview (C# class with docs)
    let documented_class = Symbol {
        id: "EmailTemplatePreview_cs".to_string(),
        name: "EmailTemplatePreview".to_string(),
        file_path: "EmailTemplatePreview.cs".to_string(),
        language: "csharp".to_string(),
        kind: SymbolKind::Class,
        signature: Some("public static class EmailTemplatePreview".to_string()),
        start_line: 11,
        start_column: 0,
        end_line: 96,
        end_column: 1,
        start_byte: 286,
        end_byte: 4249,
        visibility: Some(Visibility::Public),
        parent_id: None,
        semantic_group: None,
        doc_comment: Some("/// <summary>\n/// Simple utility class to preview MJML email templates locally\n/// Run this to generate HTML files for testing without deployment\n/// </summary>".to_string()),
        metadata: Some(HashMap::new()),
        confidence: None,
        code_context: None,
    };

    // Razor Template tag (HTML element, no docs)
    let generic_html = create_html_element_symbol("Template");

    // Both start with same base semantic score
    let base_score = 0.8;

    let class_final = apply_all_boosts(&documented_class, base_score);
    let html_final = apply_all_boosts(&generic_html, base_score);

    // Documented class should score significantly higher (4x+)
    assert!(
        class_final > html_final * 4.0,
        "Documented C# class (score: {:.2}) should beat generic HTML tag (score: {:.2}) by 4x+",
        class_final,
        html_final
    );

    println!("✅ EmailTemplatePreview score: {:.2}", class_final);
    println!("✅ Generic Template tag score: {:.2}", html_final);
    println!("✅ Ratio: {:.2}x", class_final / html_final);
}
