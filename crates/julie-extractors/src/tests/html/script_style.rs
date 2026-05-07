// Script and Style Tag Extraction Tests
//
// Tests for HTML <script> and <style> tag content extraction

use super::*;

#[cfg(test)]
mod script_style_tests {
    use super::*;

    #[test]
    fn test_extract_inline_script_tags() {
        let html = r#"
<!DOCTYPE html>
<html>
<head>
    <script>
        function greet(name) {
            console.log("Hello, " + name);
        }

        // Call the function
        greet("World");
    </script>

    <script type="text/javascript">
        var counter = 0;
        function increment() {
            counter++;
            return counter;
        }
    </script>
</head>
<body>
    <script>
        // Inline script in body
        document.addEventListener('DOMContentLoaded', function() {
            console.log('Page loaded');
        });
    </script>
</body>
</html>
"#;

        let symbols = extract_symbols(html);

        // Verify that script content is being processed
        // The HTML extractor may extract functions or variables from script tags
        // For now, just verify the HTML structure is parsed
        assert!(!symbols.is_empty());
    }

    #[test]
    fn test_extract_inline_style_tags() {
        let html = r#"
<!DOCTYPE html>
<html>
<head>
    <style>
        .header {
            background-color: #333;
            color: white;
            padding: 10px;
        }

        .content {
            font-size: 14px;
            line-height: 1.5;
        }

        @media (max-width: 600px) {
            .content {
                font-size: 12px;
            }
        }
    </style>

    <style type="text/css">
        body {
            margin: 0;
            padding: 0;
            font-family: Arial, sans-serif;
        }
    </style>
</head>
<body>
    <div class="header">Header</div>
    <div class="content">Content</div>
</body>
</html>
"#;

        let symbols = extract_symbols(html);

        // Verify that style content is being processed
        assert!(!symbols.is_empty());
    }

    #[test]
    fn test_extract_external_script_and_style_references() {
        let html = r#"
<!DOCTYPE html>
<html>
<head>
    <link rel="stylesheet" href="styles.css">
    <link rel="stylesheet" href="theme.css">

    <script src="jquery.js"></script>
    <script src="app.js"></script>
    <script src="utils.js" async></script>
</head>
<body>
    <h1>External Resources</h1>
</body>
</html>
"#;

        let symbols = extract_symbols(html);

        // Verify external references are extracted
        assert!(!symbols.is_empty());
    }

    #[test]
    fn test_extract_mixed_script_and_style_content() {
        let html = r#"
<!DOCTYPE html>
<html>
<head>
    <style>
        .button { background: blue; }
    </style>
    <script>
        function styleButton() {
            document.querySelector('.button').style.background = 'red';
        }
    </script>
</head>
<body>
    <button class="button" onclick="styleButton()">Click me</button>
</body>
</html>
"#;

        let symbols = extract_symbols(html);

        // Verify mixed content is handled
        assert!(!symbols.is_empty());
    }

    #[test]
    fn test_html_script_and_style_ranges_delegate_to_js_and_css_extractors() {
        let html = r#"<html>
<head>
  <script>
    function greet(name) {
      return `Hello ${name}`;
    }
    function farewell(name) {
      return `Bye ${name}`;
    }
  </script>
  <style>.inline {
      display: block;
    }
    .card {
      color: var(--brand);
    }
  </style>
</head>
</html>"#;

        let symbols = extract_symbols(html);
        let greet = symbols
            .iter()
            .find(|symbol| symbol.name == "greet")
            .expect("inline script should delegate to JavaScript extractor");
        assert_eq!(greet.kind, SymbolKind::Function);
        assert!(greet.start_byte > html.find("<script>").unwrap() as u32);
        let greet_slice = &html[greet.start_byte as usize..greet.end_byte as usize];
        assert!(greet_slice.starts_with("function greet"));
        assert!(greet_slice.contains("Hello"));

        let farewell = symbols
            .iter()
            .find(|symbol| symbol.name == "farewell")
            .expect("inline script should contribute all JavaScript symbols");
        assert_eq!(farewell.kind, SymbolKind::Function);
        let farewell_offset = html.find("function farewell").unwrap() as u32;
        assert_eq!(farewell.start_byte, farewell_offset);
        let (farewell_line, farewell_column) = line_column_for_byte(html, farewell_offset as usize);
        assert_eq!(farewell.start_line, farewell_line);
        assert_eq!(farewell.start_column, farewell_column);

        let card = symbols
            .iter()
            .find(|symbol| symbol.name == ".card")
            .expect("inline style should delegate to CSS extractor");
        assert_eq!(card.kind, SymbolKind::Property);
        assert!(card.start_byte > html.find("<style>").unwrap() as u32);

        let inline = symbols
            .iter()
            .find(|symbol| symbol.name == ".inline")
            .expect("inline style should contribute all CSS symbols");
        assert_eq!(inline.kind, SymbolKind::Property);
        let inline_offset = html.find(".inline").unwrap() as u32;
        assert_eq!(inline.start_byte, inline_offset);
        let (inline_line, inline_column) = line_column_for_byte(html, inline_offset as usize);
        assert_eq!(inline.start_line, inline_line);
        assert_eq!(inline.start_column, inline_column);
    }

    #[test]
    fn test_html_script_import_relationship_uses_matching_script_symbol() {
        let html = r#"<html>
<head>
  <script src="first.js"></script>
  <script src="second.js"></script>
</head>
</html>"#;

        let (symbols, relationships) = extract_symbols_and_relationships(html);
        let second_script = symbols
            .iter()
            .find(|symbol| {
                symbol
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("attributes"))
                    .and_then(|attributes| attributes.get("src"))
                    .and_then(|src| src.as_str())
                    == Some("second.js")
            })
            .expect("second script symbol should be extracted");

        let relationship = relationships
            .iter()
            .find(|relationship| relationship.to_symbol_id == "script:second.js")
            .expect("second script import relationship should be extracted");
        assert_eq!(relationship.from_symbol_id, second_script.id);
        assert_eq!(
            relationship
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("src"))
                .and_then(|src| src.as_str()),
            Some("second.js")
        );
    }
}

fn line_column_for_byte(content: &str, target: usize) -> (u32, u32) {
    let prefix = &content[..target];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() as u32 + 1;
    let column = prefix
        .rsplit_once('\n')
        .map(|(_, tail)| tail.len())
        .unwrap_or(prefix.len()) as u32;
    (line, column)
}
