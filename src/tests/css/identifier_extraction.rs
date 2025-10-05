// CSS Identifier Extraction Tests (TDD RED phase)
//
// These tests validate the extract_identifiers() functionality which extracts:
// - CSS function calls (calc, var, rgb, etc.)
// - Class/ID selector references (member access for CSS-to-HTML tracking)
// - Proper containing symbol tracking (file-scoped)
//
// Following the Rust extractor reference implementation pattern

use crate::extractors::base::{IdentifierKind, SymbolKind};
use crate::extractors::css::CSSExtractor;
use crate::tests::css::init_parser;

#[cfg(test)]
mod identifier_extraction_tests {
    use super::*;

    #[test]
    fn test_css_function_calls() {
        let css_code = r#"
.container {
    width: calc(100% - 20px);
    color: var(--primary-color);
    background: rgb(255, 0, 0);
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(css_code, None).unwrap();

        let mut extractor = CSSExtractor::new(
            "css".to_string(),
            "test.css".to_string(),
            css_code.to_string(),
        );

        // Extract symbols first
        let symbols = extractor.extract_symbols(&tree);

        // NOW extract identifiers (this will FAIL until we implement it)
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Verify we found the CSS function calls
        let calc_call = identifiers.iter().find(|id| id.name == "calc");
        assert!(
            calc_call.is_some(),
            "Should extract 'calc' function call identifier"
        );
        let calc_call = calc_call.unwrap();
        assert_eq!(calc_call.kind, IdentifierKind::Call);

        let var_call = identifiers.iter().find(|id| id.name == "var");
        assert!(
            var_call.is_some(),
            "Should extract 'var' function call identifier"
        );
        let var_call = var_call.unwrap();
        assert_eq!(var_call.kind, IdentifierKind::Call);

        let rgb_call = identifiers.iter().find(|id| id.name == "rgb");
        assert!(
            rgb_call.is_some(),
            "Should extract 'rgb' function call identifier"
        );
        let rgb_call = rgb_call.unwrap();
        assert_eq!(rgb_call.kind, IdentifierKind::Call);

        // Verify containing symbol is set correctly (should be inside .container rule)
        assert!(
            calc_call.containing_symbol_id.is_some(),
            "CSS function call should have containing symbol"
        );
    }

    #[test]
    fn test_css_member_access() {
        let css_code = r#"
.button {
    padding: 10px;
}

#header {
    background: blue;
}

.nav-item {
    color: red;
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(css_code, None).unwrap();

        let mut extractor = CSSExtractor::new(
            "css".to_string(),
            "test.css".to_string(),
            css_code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Verify we found class selector identifiers (treated as member access)
        let button_access = identifiers
            .iter()
            .filter(|id| id.name == "button" && id.kind == IdentifierKind::MemberAccess)
            .count();
        assert!(
            button_access > 0,
            "Should extract 'button' class selector as member access"
        );

        // Verify ID selector
        let header_access = identifiers
            .iter()
            .filter(|id| id.name == "header" && id.kind == IdentifierKind::MemberAccess)
            .count();
        assert!(
            header_access > 0,
            "Should extract 'header' id selector as member access"
        );

        // Verify compound class selector
        let nav_item_access = identifiers
            .iter()
            .filter(|id| id.name == "nav-item" && id.kind == IdentifierKind::MemberAccess)
            .count();
        assert!(
            nav_item_access > 0,
            "Should extract 'nav-item' class selector as member access"
        );
    }

    #[test]
    fn test_css_identifiers_have_containing_symbol() {
        // This test ensures identifiers are properly linked to their containing symbols
        let css_code = r#"
.card {
    width: calc(100% - 20px);
    background: var(--card-bg);
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(css_code, None).unwrap();

        let mut extractor = CSSExtractor::new(
            "css".to_string(),
            "test.css".to_string(),
            css_code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Find the calc call
        let calc_call = identifiers.iter().find(|id| id.name == "calc");
        assert!(calc_call.is_some());
        let calc_call = calc_call.unwrap();

        // Verify it has a containing symbol (the .card rule)
        assert!(
            calc_call.containing_symbol_id.is_some(),
            "CSS function call should have containing symbol from same file"
        );

        // Verify the containing symbol is the .card rule
        let card_rule = symbols.iter().find(|s| s.name == ".card").unwrap();
        assert_eq!(
            calc_call.containing_symbol_id.as_ref(),
            Some(&card_rule.id),
            "calc() call should be contained within .card rule"
        );
    }

    #[test]
    fn test_css_chained_member_access() {
        // CSS "chained" access = descendant selectors like .parent .child
        let css_code = r#"
.parent .child {
    color: blue;
}

.nav .item .link {
    text-decoration: none;
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(css_code, None).unwrap();

        let mut extractor = CSSExtractor::new(
            "css".to_string(),
            "test.css".to_string(),
            css_code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Should extract all class selectors from descendant chains
        let parent_access = identifiers
            .iter()
            .find(|id| id.name == "parent" && id.kind == IdentifierKind::MemberAccess);
        assert!(
            parent_access.is_some(),
            "Should extract 'parent' from descendant selector"
        );

        let child_access = identifiers
            .iter()
            .find(|id| id.name == "child" && id.kind == IdentifierKind::MemberAccess);
        assert!(
            child_access.is_some(),
            "Should extract 'child' from descendant selector"
        );

        // Verify complex chain
        let link_access = identifiers
            .iter()
            .find(|id| id.name == "link" && id.kind == IdentifierKind::MemberAccess);
        assert!(
            link_access.is_some(),
            "Should extract 'link' from multi-level descendant selector"
        );
    }

    #[test]
    fn test_css_duplicate_calls_at_different_locations() {
        let css_code = r#"
.box1 {
    width: calc(50% - 10px);
}

.box2 {
    width: calc(50% - 20px);
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(css_code, None).unwrap();

        let mut extractor = CSSExtractor::new(
            "css".to_string(),
            "test.css".to_string(),
            css_code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Should extract BOTH calc calls (they're at different locations)
        let calc_calls: Vec<_> = identifiers
            .iter()
            .filter(|id| id.name == "calc" && id.kind == IdentifierKind::Call)
            .collect();

        assert_eq!(
            calc_calls.len(),
            2,
            "Should extract both calc() calls at different locations"
        );

        // Verify they have different line numbers
        assert_ne!(
            calc_calls[0].start_line, calc_calls[1].start_line,
            "Duplicate calls should have different line numbers"
        );
    }
}
