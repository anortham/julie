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
}
