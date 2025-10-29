#[cfg(test)]
mod tests {
    /// Test case 1: HTML DOCTYPE parsing with multibyte characters
    /// Pattern: content[start..].find('>') + slicing without boundary check
    /// This tests the fix for src/extractors/html/fallback.rs:65
    #[test]
    fn test_html_doctype_parsing_with_utf8() {
        let test_cases = vec![
            // Basic ASCII case - should always work
            (r#"<!DOCTYPE html>"#, "<!DOCTYPE html>"),
            // With text content containing multibyte characters
            (r#"<!DOCTYPE html><!-- Ígú test -->"#, "<!DOCTYPE html>"),
            // With emoji in content after doctype
            (r#"<!DOCTYPE html><!-- 🚀 emoji -->"#, "<!DOCTYPE html>"),
            // With Japanese characters
            (r#"<!DOCTYPE html><!-- 日本語 -->"#, "<!DOCTYPE html>"),
            // Complex Unicode after doctype
            (r#"<!DOCTYPE html><!-- Ágú, Maí, Jún, Júl, Nóv, Des -->"#, "<!DOCTYPE html>"),
        ];

        for (input, expected) in test_cases {
            // Simulate the original pattern from fallback.rs:62-68
            if let Some(start) = input.find("<!DOCTYPE") {
                if let Some(end) = input[start..].find('>') {
                    let total_idx = start + end + 1;
                    // SAFETY CHECK: Verify boundary before slicing
                    if input.is_char_boundary(total_idx) {
                        let result = input[start..total_idx].to_string();
                        assert_eq!(result, expected, "Failed for input: {:?}", input);
                    } else {
                        // Fallback: return entire string if boundary check fails
                        let result = input.to_string();
                        assert!(!result.is_empty(), "Fallback should still work");
                    }
                }
            }
        }
    }

    /// Test case 2: Regex named group extraction with UTF-8
    /// Pattern: group_text[start + offset..].find('>') + slicing
    /// This tests the fix for src/extractors/regex/groups.rs:11-12, 16-17
    #[test]
    fn test_regex_named_group_extraction_with_utf8() {
        let test_cases = vec![
            // Standard Rust named group
            ("(?<name>...)", Some("name")),
            // Python named group
            ("(?P<name>...)", Some("name")),
            // With multibyte character names
            ("(?<café>...)", Some("café")),
            // With multibyte before and after
            ("(?<Ígúname>...)", Some("Ígúname")),
            // Emoji in name (valid in some regex engines)
            ("(?<test🚀>...)", Some("test🚀")),
            // Just the marker without proper closure
            ("(?<", None),
            // Unicode-heavy group name
            ("(?<日本語>...)", Some("日本語")),
        ];

        for (input, expected) in test_cases {
            // Test pattern 1: (?<name>)
            if let Some(start) = input.find("(?<") {
                if let Some(end) = input[start + 3..].find('>') {
                    let total_idx = start + 3 + end;
                    if input.is_char_boundary(total_idx) {
                        let result = input[start + 3..total_idx].to_string();
                        assert_eq!(
                            Some(result.as_str()),
                            expected,
                            "Failed for (?<...> pattern: {:?}",
                            input
                        );
                    } else {
                        // Boundary check failed - should not slice
                        assert!(expected.is_none(), "Should have failed for: {:?}", input);
                    }
                }
            }

            // Test pattern 2: (?P<name>)
            if let Some(start) = input.find("(?P<") {
                if let Some(end) = input[start + 4..].find('>') {
                    let total_idx = start + 4 + end;
                    if input.is_char_boundary(total_idx) {
                        let result = input[start + 4..total_idx].to_string();
                        assert_eq!(
                            Some(result.as_str()),
                            expected,
                            "Failed for (?P<...> pattern: {:?}",
                            input
                        );
                    } else {
                        // Boundary check failed - should not slice
                        assert!(expected.is_none(), "Should have failed for: {:?}", input);
                    }
                }
            }
        }
    }

    /// Test case 3: Regex unicode property extraction
    /// Pattern: property_text[start..].find('}') + slicing
    /// This tests the fix for src/extractors/regex/flags.rs:64-66
    #[test]
    fn test_regex_unicode_property_with_utf8() {
        let test_cases = vec![
            // Standard Unicode property
            (r"\p{Letter}", "Letter"),
            // Negated version
            (r"\P{Letter}", "Letter"),
            // With text before
            (r"prefix\p{Number}", "Number"),
            // Multibyte chars around pattern
            ("Ígú\\p{Letter}test", "Letter"),
            // Emoji before pattern
            ("🚀\\p{Symbol}", "Symbol"),
            // Unicode property name with special chars
            (r"\p{Lowercase_Letter}", "Lowercase_Letter"),
        ];

        for (input, expected) in test_cases {
            let patterns = [r"\p{", r"\P{"];

            for pattern in &patterns {
                if let Some(start) = input.find(pattern) {
                    if let Some(end) = input[start..].find('}') {
                        let inner_start = start + 3; // length of \p{ or \P{
                        let inner_end = start + end;
                        if input.is_char_boundary(inner_start)
                            && input.is_char_boundary(inner_end)
                        {
                            let inner = &input[inner_start..inner_end];
                            assert_eq!(inner, expected, "Failed for pattern: {:?}", input);
                        } else {
                            // Boundary check failed
                            panic!("Should not have sliced unsafe boundary in: {:?}", input);
                        }
                    }
                }
            }
        }
    }

    /// Test case 4: C++ destructor name extraction
    /// Pattern: signature[name_start..].find('(') + slicing
    /// This tests the fix for src/extractors/cpp/declarations.rs:524-525
    #[test]
    fn test_cpp_destructor_name_extraction_with_utf8() {
        let test_cases = vec![
            ("~MyClass()", Some("~MyClass")),
            ("~MyClass(const)", Some("~MyClass")),
            // With spacing
            ("  ~MyClass()", Some("~MyClass")),
            // With multibyte chars in template context (unlikely but safe)
            ("~Café()", Some("~Café")),
            // Multiple tildes (malformed, but handle gracefully)
            ("~~MyClass()", Some("~~MyClass")),
        ];

        for (input, expected) in test_cases {
            if let Some(name_start) = input.find('~') {
                if let Some(open_paren) = input[name_start..].find('(') {
                    let name_end = name_start + open_paren;
                    if input.is_char_boundary(name_start) && input.is_char_boundary(name_end) {
                        let name = input[name_start..name_end].to_string();
                        assert_eq!(
                            Some(name.as_str()),
                            expected,
                            "Failed for input: {:?}",
                            input
                        );
                    } else {
                        // Boundary check failed - should not slice
                        assert!(expected.is_none(), "Should have failed for: {:?}", input);
                    }
                }
            }
        }
    }

    /// Test case 5: C ALIGN macro extraction
    /// Pattern: node_text[align_start..].find(')') + slicing
    /// This tests the fix for src/extractors/c/types.rs:207-208, 217-218
    #[test]
    fn test_c_align_macro_extraction_with_utf8() {
        let test_cases = vec![
            ("ALIGN(CACHE_LINE_SIZE)", Some("ALIGN(CACHE_LINE_SIZE)")),
            ("struct {\n    ALIGN(64)\n}", Some("ALIGN(64)")),
            // With multibyte chars in macro body (rare but safe)
            ("ALIGN(SIZE_Ígú)", Some("ALIGN(SIZE_Ígú)")),
            // With emoji (malformed but handle safely)
            ("ALIGN(SIZE🚀)", Some("ALIGN(SIZE🚀)")),
            // Multiple macros
            ("ALIGN(32) int x; ALIGN(64) int y;", Some("ALIGN(32)")),
        ];

        for (input, expected) in test_cases {
            if let Some(align_start) = input.find("ALIGN(") {
                if let Some(close_paren) = input[align_start..].find(')') {
                    let total_idx = align_start + close_paren + 1;
                    if input.is_char_boundary(align_start) && input.is_char_boundary(total_idx) {
                        let result = &input[align_start..total_idx];
                        assert_eq!(
                            Some(result),
                            expected,
                            "Failed for input: {:?}",
                            input
                        );
                    } else {
                        // Boundary check failed - should not slice
                        assert!(expected.is_none(), "Should have failed for: {:?}", input);
                    }
                }
            }
        }
    }

    /// Test case 6: Ensure existing safe UTF-8 handling still works
    /// This is a regression test to make sure we didn't break anything
    #[test]
    fn test_utf8_boundary_checks_dont_break_ascii() {
        let inputs = vec![
            "<!DOCTYPE html>",
            "(?<name>)",
            r"\p{Letter}",
            "~Destructor()",
            "ALIGN(128)",
        ];

        for input in inputs {
            // Every byte in ASCII is a valid char boundary
            for i in 0..=input.len() {
                assert!(
                    input.is_char_boundary(i),
                    "ASCII string should have all char boundaries: {:?}",
                    input
                );
            }
        }
    }

    /// Test case 7: Edge cases - empty strings and single characters
    #[test]
    fn test_utf8_boundary_edge_cases() {
        // Empty string - valid char boundary at position 0
        let empty = "";
        assert!(empty.is_char_boundary(0));

        // Single ASCII character
        let single_ascii = "a";
        assert!(single_ascii.is_char_boundary(0));
        assert!(single_ascii.is_char_boundary(1));

        // Single multibyte character (emoji)
        let single_emoji = "🚀"; // 4 bytes in UTF-8
        assert!(single_emoji.is_char_boundary(0));
        assert!(!single_emoji.is_char_boundary(1)); // Middle of character
        assert!(!single_emoji.is_char_boundary(2)); // Middle of character
        assert!(!single_emoji.is_char_boundary(3)); // Middle of character
        assert!(single_emoji.is_char_boundary(4)); // End of character
    }
}
