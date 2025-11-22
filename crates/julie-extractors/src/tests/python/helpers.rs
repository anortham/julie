// Python helpers inline tests extracted from extractors/python/helpers.rs

use crate::python::helpers;

#[test]
fn test_strip_string_delimiters_triple_double() {
    let input = r#""""This is a docstring""""#;
    let result = helpers::strip_string_delimiters(input);
    assert_eq!(result, "This is a docstring");
}

#[test]
fn test_strip_string_delimiters_triple_single() {
    let input = "'''This is a docstring'''";
    let result = helpers::strip_string_delimiters(input);
    assert_eq!(result, "This is a docstring");
}

#[test]
fn test_strip_string_delimiters_double() {
    let input = r#""Hello World""#;
    let result = helpers::strip_string_delimiters(input);
    assert_eq!(result, "Hello World");
}

#[test]
fn test_strip_string_delimiters_single() {
    let input = "'Hello World'";
    let result = helpers::strip_string_delimiters(input);
    assert_eq!(result, "Hello World");
}

#[test]
fn test_strip_string_delimiters_no_delimiters() {
    let input = "Hello World";
    let result = helpers::strip_string_delimiters(input);
    assert_eq!(result, "Hello World");
}
