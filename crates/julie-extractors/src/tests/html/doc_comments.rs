use super::extract_symbols;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_html_comment_from_element() {
        let code = r#"
            <!-- Primary navigation component
                 Handles site-wide navigation links -->
            <nav class="navbar">
                <ul></ul>
            </nav>
        "#;

        let symbols = extract_symbols(code);
        let nav = symbols.iter().find(|s| s.name == "nav").unwrap();

        assert!(nav.doc_comment.is_some());
        let doc = nav.doc_comment.as_ref().unwrap();
        assert!(doc.contains("Primary navigation"));
        assert!(doc.contains("site-wide navigation"));
    }

    #[test]
    fn test_extract_html_comment_from_custom_element() {
        let code = r#"
            <!-- User profile card component
                 Displays user information in a card layout -->
            <user-profile data-id="123">
            </user-profile>
        "#;

        let symbols = extract_symbols(code);
        let element = symbols.iter().find(|s| s.name == "user-profile").unwrap();

        assert!(element.doc_comment.is_some());
        let doc = element.doc_comment.as_ref().unwrap();
        assert!(doc.contains("User profile card"));
    }

    #[test]
    fn test_extract_html_comment_from_script_element() {
        let code = r#"
            <!-- Main application bundle -->
            <script src="app.js"></script>
        "#;

        let symbols = extract_symbols(code);
        let script = symbols.iter().find(|s| s.name == "script").unwrap();

        assert!(script.doc_comment.is_some());
        let doc = script.doc_comment.as_ref().unwrap();
        assert!(doc.contains("Main application bundle"));
    }

    #[test]
    fn test_extract_html_comment_from_style_element() {
        let code = r#"
            <!-- Theme overrides for dark mode -->
            <style>
                body { color: white; }
            </style>
        "#;

        let symbols = extract_symbols(code);
        let style = symbols.iter().find(|s| s.name == "style").unwrap();

        assert!(style.doc_comment.is_some());
        let doc = style.doc_comment.as_ref().unwrap();
        assert!(doc.contains("Theme overrides"));
    }

    #[test]
    fn test_element_without_comment_has_no_doc_comment() {
        let code = r#"
            <div class="no-comment">Content</div>
        "#;

        let symbols = extract_symbols(code);
        let div = symbols.iter().find(|s| s.name == "div").unwrap();

        assert!(div.doc_comment.is_none());
    }

    #[test]
    fn test_multiline_block_comment_on_element() {
        let code = r#"
            <!--
             Header navigation section
             Contains primary and secondary navigation
             Updated: 2025-10-24
            -->
            <header role="banner">
                <nav></nav>
            </header>
        "#;

        let symbols = extract_symbols(code);
        let header = symbols.iter().find(|s| s.name == "header").unwrap();

        assert!(header.doc_comment.is_some());
        let doc = header.doc_comment.as_ref().unwrap();
        assert!(doc.contains("Header navigation"));
        assert!(doc.contains("primary and secondary"));
    }

    #[test]
    fn test_single_line_html_comment() {
        let code = r#"
            <!-- Main footer section -->
            <footer>
                <p>Copyright 2025</p>
            </footer>
        "#;

        let symbols = extract_symbols(code);
        let footer = symbols.iter().find(|s| s.name == "footer").unwrap();

        assert!(footer.doc_comment.is_some());
        let doc = footer.doc_comment.as_ref().unwrap();
        assert!(doc.contains("Main footer section"));
    }

    #[test]
    fn test_comment_on_meta_element() {
        let code = r#"
            <!-- Open Graph metadata for social sharing -->
            <meta property="og:title" content="My Site">
        "#;

        let symbols = extract_symbols(code);
        let meta = symbols.iter().find(|s| s.name == "meta").unwrap();

        assert!(meta.doc_comment.is_some());
        let doc = meta.doc_comment.as_ref().unwrap();
        assert!(doc.contains("Open Graph"));
    }
}
