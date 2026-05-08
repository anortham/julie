use crate::base::{Symbol, SymbolKind};

fn symbol_type(symbol: &Symbol) -> Option<&str> {
    symbol
        .metadata
        .as_ref()
        .and_then(|m| m.get("type"))
        .and_then(|v| v.as_str())
}

fn assert_exact_span(symbol: &Symbol, source: &str, expected_text: &str) {
    let start = source
        .find(expected_text)
        .unwrap_or_else(|| panic!("Expected to find '{expected_text}' in test source"));
    let end = start + expected_text.len();

    assert_eq!(
        symbol.start_byte as usize, start,
        "start_byte mismatch for {expected_text}"
    );
    assert_eq!(
        symbol.end_byte as usize, end,
        "end_byte mismatch for {expected_text}"
    );
    assert_eq!(
        symbol.start_column as usize, start,
        "start_column mismatch for {expected_text}"
    );
    assert_eq!(
        symbol.end_column as usize, end,
        "end_column mismatch for {expected_text}"
    );
    assert_eq!(
        symbol.start_line, 1,
        "start_line mismatch for {expected_text}"
    );
    assert_eq!(symbol.end_line, 1, "end_line mismatch for {expected_text}");
}

#[test]
fn test_regex_constructs_have_distinct_symbol_kinds() {
    let regex_code = r#"(?<capture>[A-Z]+)(?=foo)(?!bar)(?<tail>[^x]+)\p{Greek}\p{Letter}"#;
    let symbols = super::extract_symbols(regex_code);

    let captures: Vec<_> = symbols
        .iter()
        .filter(|s| symbol_type(s) == Some("group"))
        .collect();
    assert_eq!(captures.len(), 2, "Expected exactly 2 capture groups");
    assert_eq!(captures[0].name, "(?<capture>[A-Z]+)");
    assert_eq!(captures[1].name, "(?<tail>[^x]+)");
    assert!(captures.iter().all(|s| s.kind == SymbolKind::Function));
    assert!(
        captures
            .iter()
            .all(|s| s.metadata.as_ref().and_then(|m| m.get("named")).is_some())
    );

    let character_classes: Vec<_> = symbols
        .iter()
        .filter(|s| symbol_type(s) == Some("character-class"))
        .collect();
    assert_eq!(
        character_classes.len(),
        2,
        "Expected exactly 2 character classes"
    );
    assert_eq!(character_classes[0].name, "[A-Z]");
    assert_eq!(character_classes[1].name, "[^x]");
    assert!(
        character_classes
            .iter()
            .all(|s| s.kind == SymbolKind::Class)
    );

    let lookarounds: Vec<_> = symbols
        .iter()
        .filter(|s| symbol_type(s) == Some("lookaround"))
        .collect();
    assert_eq!(lookarounds.len(), 2, "Expected exactly 2 lookarounds");
    assert_eq!(lookarounds[0].name, "(?=foo)");
    assert_eq!(lookarounds[1].name, "(?!bar)");
    assert!(lookarounds.iter().all(|s| s.kind == SymbolKind::Method));
    assert_eq!(
        lookarounds[0]
            .metadata
            .as_ref()
            .and_then(|m| m.get("direction"))
            .and_then(|v| v.as_str()),
        Some("lookahead")
    );
    assert_eq!(
        lookarounds[0]
            .metadata
            .as_ref()
            .and_then(|m| m.get("positive"))
            .and_then(|v| v.as_str()),
        Some("true")
    );
    assert_eq!(
        lookarounds[1]
            .metadata
            .as_ref()
            .and_then(|m| m.get("positive"))
            .and_then(|v| v.as_str()),
        Some("false")
    );
    assert_exact_span(lookarounds[0], regex_code, "(?=foo)");
    assert_exact_span(lookarounds[1], regex_code, "(?!bar)");

    let unicode_properties: Vec<_> = symbols
        .iter()
        .filter(|s| symbol_type(s) == Some("unicode-property"))
        .collect();
    assert_eq!(
        unicode_properties.len(),
        2,
        "Expected exactly 2 unicode properties"
    );
    assert_eq!(unicode_properties[0].name, "\\p{Greek}");
    assert_eq!(unicode_properties[1].name, "\\p{Letter}");
    assert!(
        unicode_properties
            .iter()
            .all(|s| s.kind == SymbolKind::Constant)
    );
    assert_eq!(
        unicode_properties[0]
            .metadata
            .as_ref()
            .and_then(|m| m.get("property"))
            .and_then(|v| v.as_str()),
        Some("Greek")
    );
    assert_eq!(
        unicode_properties[1]
            .metadata
            .as_ref()
            .and_then(|m| m.get("property"))
            .and_then(|v| v.as_str()),
        Some("Letter")
    );
    assert_exact_span(unicode_properties[0], regex_code, "\\p{Greek}");
    assert_exact_span(unicode_properties[1], regex_code, "\\p{Letter}");
}
