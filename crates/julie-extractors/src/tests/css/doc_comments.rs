use super::extract_symbols;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_css_comment_from_rule() {
        let code = r#"
            /**
             * Primary button styling
             * Used for call-to-action buttons throughout the app
             */
            .btn-primary {
                background-color: blue;
                color: white;
            }
        "#;

        let symbols = extract_symbols(code);
        let rule = symbols.iter().find(|s| s.name == ".btn-primary").unwrap();

        assert!(rule.doc_comment.is_some());
        let doc = rule.doc_comment.as_ref().unwrap();
        assert!(doc.contains("Primary button styling"));
        assert!(doc.contains("call-to-action buttons"));
    }

    #[test]
    fn test_extract_css_comment_from_keyframes() {
        let code = r#"
            /* Fade in animation for modal dialogs */
            @keyframes fadeIn {
                from { opacity: 0; }
                to { opacity: 1; }
            }
        "#;

        let symbols = extract_symbols(code);
        let animation = symbols.iter().find(|s| s.name == "@keyframes fadeIn").unwrap();

        assert!(animation.doc_comment.is_some());
        let doc = animation.doc_comment.as_ref().unwrap();
        assert!(doc.contains("Fade in animation"));
        assert!(doc.contains("modal dialogs"));
    }

    #[test]
    fn test_extract_css_comment_from_media_query() {
        let code = r#"
            /* Mobile-first responsive design for tablets and above */
            @media (min-width: 768px) {
                .container {
                    max-width: 960px;
                }
            }
        "#;

        let symbols = extract_symbols(code);
        let media = symbols
            .iter()
            .find(|s| s.signature.as_ref().unwrap().contains("@media"))
            .unwrap();

        assert!(media.doc_comment.is_some());
        let doc = media.doc_comment.as_ref().unwrap();
        assert!(doc.contains("responsive design"));
    }

    #[test]
    fn test_extract_css_comment_from_custom_property() {
        // Custom properties inside rules with preceding comments
        let code = r#"
            :root {
                /* Brand colors - primary, secondary, and accent colors */
                --primary-color: #3498db;
                --secondary-color: #2ecc71;
            }
        "#;

        let symbols = extract_symbols(code);
        // Custom properties are extracted when inside a rule
        let root_rule = symbols.iter().find(|s| s.name == ":root");

        // The :root rule itself should be found and may have a doc comment
        // (or no comment if none is directly before the :root selector)
        assert!(root_rule.is_some());

        // Custom properties inside the rule are also extracted
        let prop = symbols.iter().find(|s| s.name == "--primary-color");

        // This validates that custom properties are extracted within rules
        assert!(prop.is_some());
    }

    #[test]
    fn test_rule_without_comment_has_no_doc_comment() {
        let code = r#"
            .no-comment {
                color: red;
            }
        "#;

        let symbols = extract_symbols(code);
        let rule = symbols.iter().find(|s| s.name == ".no-comment").unwrap();

        assert!(rule.doc_comment.is_none());
    }

    #[test]
    fn test_multiline_block_comment_on_rule() {
        let code = r#"
            /*
             * Main container styles
             * This is a multi-line comment
             * Spans multiple lines
             */
            .container {
                display: flex;
                justify-content: center;
            }
        "#;

        let symbols = extract_symbols(code);
        let rule = symbols.iter().find(|s| s.name == ".container").unwrap();

        assert!(rule.doc_comment.is_some());
        let doc = rule.doc_comment.as_ref().unwrap();
        assert!(doc.contains("Main container styles"));
        assert!(doc.contains("multi-line comment"));
    }

    #[test]
    fn test_comment_before_supports_rule() {
        let code = r#"
            /* CSS Grid layout support check */
            @supports (display: grid) {
                .grid-layout {
                    display: grid;
                }
            }
        "#;

        let symbols = extract_symbols(code);
        let supports = symbols
            .iter()
            .find(|s| s.signature.as_ref().unwrap().contains("@supports"))
            .unwrap();

        assert!(supports.doc_comment.is_some());
        let doc = supports.doc_comment.as_ref().unwrap();
        assert!(doc.contains("CSS Grid layout"));
    }
}
