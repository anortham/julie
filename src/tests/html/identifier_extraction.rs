// HTML Identifier Extraction Tests (TDD RED phase)
//
// These tests validate the extract_identifiers() functionality which extracts:
// - Event handlers and data actions (onclick, data-action) as "calls"
// - id and class attribute references as "member access"
// - Proper containing symbol tracking (file-scoped)
//
// Following the Rust extractor reference implementation pattern

#![allow(unused_imports)]

use crate::extractors::base::{IdentifierKind, SymbolKind};
use crate::extractors::html::HTMLExtractor;
use crate::tests::html::init_parser;

#[cfg(test)]
mod identifier_extraction_tests {
    use super::*;

    #[test]
    fn test_html_function_calls() {
        // In HTML, "function calls" are event handlers and data-action attributes
        let html_code = r#"
<!DOCTYPE html>
<html>
<body>
    <button onclick="handleClick">Click Me</button>
    <button data-action="submitForm">Submit</button>
    <div onload="initWidget">Widget</div>
</body>
</html>
"#;

        let mut parser = init_parser();
        let tree = parser.parse(html_code, None).unwrap();

        let mut extractor = HTMLExtractor::new(
            "html".to_string(),
            "test.html".to_string(),
            html_code.to_string(),
        );

        // Extract symbols first
        let symbols = extractor.extract_symbols(&tree);

        // NOW extract identifiers (this will FAIL until we implement it)
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Verify we found the event handler "calls"
        let handle_click = identifiers.iter().find(|id| id.name == "handleClick");
        assert!(
            handle_click.is_some(),
            "Should extract 'handleClick' event handler as call identifier"
        );
        let handle_click = handle_click.unwrap();
        assert_eq!(handle_click.kind, IdentifierKind::Call);

        let submit_form = identifiers.iter().find(|id| id.name == "submitForm");
        assert!(
            submit_form.is_some(),
            "Should extract 'submitForm' data-action as call identifier"
        );
        let submit_form = submit_form.unwrap();
        assert_eq!(submit_form.kind, IdentifierKind::Call);

        // Verify containing symbol is set correctly
        assert!(
            handle_click.containing_symbol_id.is_some(),
            "Event handler should have containing symbol"
        );
    }

    #[test]
    fn test_html_member_access() {
        // In HTML, "member access" refers to id and class attributes
        let html_code = r#"
<!DOCTYPE html>
<html>
<body>
    <div id="main-content" class="container">
        <h1 id="page-title">Welcome</h1>
        <p class="text-large">Hello World</p>
    </div>
</body>
</html>
"#;

        let mut parser = init_parser();
        let tree = parser.parse(html_code, None).unwrap();

        let mut extractor = HTMLExtractor::new(
            "html".to_string(),
            "test.html".to_string(),
            html_code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Verify we found id references as member access
        let main_content = identifiers
            .iter()
            .filter(|id| id.name == "main-content" && id.kind == IdentifierKind::MemberAccess)
            .count();
        assert!(
            main_content > 0,
            "Should extract 'main-content' id as member access identifier"
        );

        let page_title = identifiers
            .iter()
            .filter(|id| id.name == "page-title" && id.kind == IdentifierKind::MemberAccess)
            .count();
        assert!(
            page_title > 0,
            "Should extract 'page-title' id as member access identifier"
        );

        // Verify class references are also extracted
        let container_class = identifiers
            .iter()
            .filter(|id| id.name == "container" && id.kind == IdentifierKind::MemberAccess)
            .count();
        assert!(
            container_class > 0,
            "Should extract 'container' class as member access identifier"
        );
    }

    #[test]
    fn test_html_identifiers_have_containing_symbol() {
        // This test ensures containing symbol tracking works correctly
        let html_code = r#"
<!DOCTYPE html>
<html>
<body>
    <div id="wrapper">
        <button onclick="clickHandler">Click</button>
    </div>
</body>
</html>
"#;

        let mut parser = init_parser();
        let tree = parser.parse(html_code, None).unwrap();

        let mut extractor = HTMLExtractor::new(
            "html".to_string(),
            "test.html".to_string(),
            html_code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Find the onclick handler
        let click_handler = identifiers.iter().find(|id| id.name == "clickHandler");
        assert!(click_handler.is_some());
        let click_handler = click_handler.unwrap();

        // Verify it has a containing symbol (the button element)
        assert!(
            click_handler.containing_symbol_id.is_some(),
            "Event handler should have containing symbol from same file"
        );

        // Find wrapper id
        let wrapper_id = identifiers.iter().find(|id| id.name == "wrapper");
        assert!(wrapper_id.is_some());
        let wrapper_id = wrapper_id.unwrap();

        // Verify it has a containing symbol
        assert!(
            wrapper_id.containing_symbol_id.is_some(),
            "ID reference should have containing symbol"
        );
    }

    #[test]
    fn test_html_chained_member_access() {
        // Test nested element references with classes
        let html_code = r#"
<!DOCTYPE html>
<html>
<body>
    <div class="outer-container">
        <div class="inner-container">
            <span class="nested-element">Content</span>
        </div>
    </div>
</body>
</html>
"#;

        let mut parser = init_parser();
        let tree = parser.parse(html_code, None).unwrap();

        let mut extractor = HTMLExtractor::new(
            "html".to_string(),
            "test.html".to_string(),
            html_code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Should extract all class identifiers
        let outer_container = identifiers
            .iter()
            .find(|id| id.name == "outer-container" && id.kind == IdentifierKind::MemberAccess);
        assert!(
            outer_container.is_some(),
            "Should extract 'outer-container' class identifier"
        );

        let inner_container = identifiers
            .iter()
            .find(|id| id.name == "inner-container" && id.kind == IdentifierKind::MemberAccess);
        assert!(
            inner_container.is_some(),
            "Should extract 'inner-container' class identifier"
        );

        let nested_element = identifiers
            .iter()
            .find(|id| id.name == "nested-element" && id.kind == IdentifierKind::MemberAccess);
        assert!(
            nested_element.is_some(),
            "Should extract 'nested-element' class identifier"
        );
    }

    #[test]
    fn test_html_duplicate_calls_at_different_locations() {
        let html_code = r#"
<!DOCTYPE html>
<html>
<body>
    <button onclick="handleAction">First</button>
    <button onclick="handleAction">Second</button>
</body>
</html>
"#;

        let mut parser = init_parser();
        let tree = parser.parse(html_code, None).unwrap();

        let mut extractor = HTMLExtractor::new(
            "html".to_string(),
            "test.html".to_string(),
            html_code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Should extract BOTH event handlers (they're at different locations)
        let handle_action_calls: Vec<_> = identifiers
            .iter()
            .filter(|id| id.name == "handleAction" && id.kind == IdentifierKind::Call)
            .collect();

        assert_eq!(
            handle_action_calls.len(),
            2,
            "Should extract both handleAction event handlers at different locations"
        );

        // Verify they have different line numbers
        assert_ne!(
            handle_action_calls[0].start_line, handle_action_calls[1].start_line,
            "Duplicate event handlers should have different line numbers"
        );
    }
}
