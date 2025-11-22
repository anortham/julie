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
<div>
    <p title="Simple title">Text</p>
    <p title='Single quotes'>Text</p>
    <p title="Mixed 'quotes' here">Text</p>
    <p title='Mixed "quotes" here'>Text</p>
    <p title="Escaped \"quotes\"">Text</p>
</div>
"#;

        let symbols = extract_symbols(html);

        // Should handle various quote scenarios
        assert!(!symbols.is_empty());
    }

    #[test]
    fn test_extract_case_insensitive_tags() {
        let html = r#"
<HTML>
<HEAD>
    <TITLE>Case Insensitive</TITLE>
</HEAD>
<BODY>
    <DIV>
        <P>Content</P>
    </DIV>
    <BR>
    <HR>
</BODY>
</HTML>
"#;

        let symbols = extract_symbols(html);

        // HTML is case-insensitive
        assert!(!symbols.is_empty());
    }

    #[test]
    fn test_extract_minimal_html() {
        let html = r#"<p>Minimal</p>"#;

        let symbols = extract_symbols(html);

        // Should handle minimal valid HTML
        assert!(!symbols.is_empty());
    }

    #[test]
    fn test_extract_empty_and_whitespace() {
        let html = r#"
<div>
    <p></p>
    <p>   </p>
    <p>
    </p>
    <br>
    <br/>
    <br />
</div>
"#;

        let symbols = extract_symbols(html);

        // Should handle empty elements and whitespace
        assert!(!symbols.is_empty());
    }
}
