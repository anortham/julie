use crate::tools::deep_dive::DeepDiveTool;

#[test]
fn test_deep_dive_accepts_symbol_name_alias() {
    // Some MCP clients send "symbol_name" instead of "symbol".
    // Verify serde accepts both field names.
    let json = r#"{"symbol_name": "MyFunction"}"#;
    let tool: DeepDiveTool = serde_json::from_str(json).unwrap();
    assert_eq!(tool.symbol, "MyFunction");
}

#[test]
fn test_deep_dive_accepts_canonical_symbol_field() {
    let json = r#"{"symbol": "MyFunction"}"#;
    let tool: DeepDiveTool = serde_json::from_str(json).unwrap();
    assert_eq!(tool.symbol, "MyFunction");
}
