// HTML Edge Cases and Malformed Content Tests
//
// Tests for handling malformed HTML, unclosed tags, and edge cases

use super::*;

#[cfg(test)]
mod edge_cases_tests {
    use super::*;

    #[test]
    fn test_extract_unclosed_tags() {
        let html = r#"
<html>
<head>
    <title>Unclosed Tags Test</title>
<body>
    <div>
        <p>Unclosed paragraph
        <span>Unclosed span
        <img src="test.jpg" alt="Test image">
    </div>
    <br>
    <hr>
"#;

        let symbols = extract_symbols(html);

        // Should handle unclosed tags gracefully
        assert!(!symbols.is_empty());
    }

    #[test]
    fn test_extract_malformed_attributes() {
        let html = r#"
<div class="valid">
    <input type="text" name="field" value="test">
    <input type=invalid name="bad" value=test>
    <img src="image.jpg" alt="Description" height=100 width="200">
    <a href=missing-quotes.com>Link</a>
</div>
"#;

        let symbols = extract_symbols(html);

        // Should handle malformed attributes
        assert!(!symbols.is_empty());
    }

    #[test]
    fn test_extract_nested_quotes_and_escaping() {
        let html = r#"
<header>
    <a href="simple" title="Simple title">Text</a>
    <a href='single' title='Single quotes'>Text</a>
    <a href="mixed" title="Mixed 'quotes' here">Text</a>
    <a href='mixed2' title='Mixed "quotes" here'>Text</a>
    <img src="image.jpg" alt="Escaped \"quotes\"">
</header>
"#;

        let symbols = extract_symbols(html);

        // Should handle various quote scenarios with meaningful elements
        assert!(!symbols.is_empty());
        // header, links, and img are all meaningful elements
        assert!(symbols.iter().any(|s| s.name == "header"));
        assert!(symbols.iter().any(|s| s.name == "a"));
        assert!(symbols.iter().any(|s| s.name == "img"));
    }

    #[test]
    fn test_extract_case_insensitive_tags() {
        let html = r#"
<HTML>
<HEAD>
    <TITLE>Case Insensitive</TITLE>
</HEAD>
<BODY>
    <HEADER>
        <NAV>Navigation</NAV>
    </HEADER>
    <DIV>
        <P>Content</P>
    </DIV>
</BODY>
</HTML>
"#;

        let symbols = extract_symbols(html);

        // HTML is case-insensitive — semantic tags should be extracted
        // regardless of case, generic containers (DIV, P) should be filtered
        assert!(!symbols.is_empty());
        // Verify no generic containers leaked through
        assert!(
            !symbols.iter().any(|s| s.name == "DIV" || s.name == "div"),
            "generic <div> should be filtered"
        );
        assert!(
            !symbols.iter().any(|s| s.name == "P" || s.name == "p"),
            "generic <p> should be filtered"
        );
    }

    #[test]
    fn test_extract_minimal_html() {
        let html = r#"<header>Minimal</header>"#;

        let symbols = extract_symbols(html);

        // Should handle minimal valid HTML with a semantic element
        assert!(!symbols.is_empty());
        assert!(symbols.iter().any(|s| s.name == "header"));
    }

    #[test]
    fn test_extract_generic_containers_filtered() {
        let html = r#"<p>Minimal</p>"#;

        let symbols = extract_symbols(html);

        // Generic containers without id/name should produce no symbols
        assert!(
            symbols.is_empty(),
            "generic <p> without id/name should be filtered out"
        );
    }

    #[test]
    fn test_generic_container_with_id_allowed() {
        let html = r#"
<div id="app">
    <div>
        <span>ignored</span>
        <p id="intro">Hello</p>
    </div>
</div>
"#;

        let symbols = extract_symbols(html);

        // Generic containers WITH id are meaningful (referenceable)
        assert!(symbols.iter().any(|s| s.name == "div"), "div#app should be extracted");
        assert!(symbols.iter().any(|s| s.name == "p"), "p#intro should be extracted");

        // Generic containers WITHOUT id should be filtered
        let div_count = symbols.iter().filter(|s| s.name == "div").count();
        assert_eq!(div_count, 1, "only div with id should be extracted, not bare divs");
        assert!(
            !symbols.iter().any(|s| s.name == "span"),
            "bare <span> should be filtered"
        );
    }

    #[test]
    fn test_noise_filter_applies_to_all_generic_containers() {
        let html = r#"
<html>
<body>
    <div><span>text</span></div>
    <p>paragraph</p>
    <ul><li>item</li></ul>
    <ol><li>item</li></ol>
    <table><tr><td>cell</td></tr></table>
    <dl><dt>term</dt><dd>definition</dd></dl>
    <header><nav>links</nav></header>
</body>
</html>
"#;

        let symbols = extract_symbols(html);
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();

        // Generic containers should all be filtered
        let generic_tags = ["div", "span", "p", "ul", "ol", "li", "table", "tr", "td", "dl", "dt", "dd"];
        for tag in &generic_tags {
            assert!(
                !names.contains(tag),
                "generic <{tag}> should be filtered, but found in: {names:?}"
            );
        }

        // Semantic elements should be extracted
        assert!(names.contains(&"html"), "html should be extracted");
        assert!(names.contains(&"body"), "body should be extracted");
        assert!(names.contains(&"header"), "header should be extracted");
        assert!(names.contains(&"nav"), "nav should be extracted");
    }

    #[test]
    fn test_extract_empty_and_whitespace() {
        let html = r#"
<section>
    <h1></h1>
    <h2>   </h2>
    <h3>
    </h3>
    <img src="" alt="">
    <input type="text">
</section>
"#;

        let symbols = extract_symbols(html);

        // Should handle empty and whitespace-only meaningful elements
        assert!(!symbols.is_empty());
        assert!(symbols.iter().any(|s| s.name == "section"));
    }
}
