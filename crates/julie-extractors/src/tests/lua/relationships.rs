use crate::base::RelationshipKind;
use crate::lua::LuaExtractor;
use crate::tests::lua::init_parser;
use std::path::PathBuf;

#[test]
fn test_extract_function_call_relationships() {
    let code = r#"
local function helper()
    return 42
end

function main()
    return helper()
end
"#;

    let mut parser = init_parser();
    let tree = parser.parse(code, None).expect("Failed to parse Lua code");
    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = LuaExtractor::new(
        "lua".to_string(),
        "test.lua".to_string(),
        code.to_string(),
        &workspace_root,
    );

    let symbols = extractor.extract_symbols(&tree);
    let relationships = extractor.extract_relationships(&tree, &symbols);

    let main_symbol = symbols
        .iter()
        .find(|s| s.name == "main")
        .expect("missing main symbol");
    let helper_symbol = symbols
        .iter()
        .find(|s| s.name == "helper")
        .expect("missing helper symbol");

    assert!(
        relationships.iter().any(|rel| {
            rel.kind == RelationshipKind::Calls
                && rel.from_symbol_id == main_symbol.id
                && rel.to_symbol_id == helper_symbol.id
        }),
        "Expected main -> helper Calls relationship, got {:?}",
        relationships
    );
}

#[test]
fn test_lua_bare_require_emits_import_symbol() {
    let code = r#"
require("json")
"#;

    let mut parser = init_parser();
    let tree = parser.parse(code, None).expect("Failed to parse Lua code");
    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = LuaExtractor::new(
        "lua".to_string(),
        "test.lua".to_string(),
        code.to_string(),
        &workspace_root,
    );

    let symbols = extractor.extract_symbols(&tree);
    let relationships = extractor.extract_relationships(&tree, &symbols);

    let import_symbol = symbols
        .iter()
        .find(|symbol| symbol.name == "json" && symbol.kind == crate::base::SymbolKind::Import)
        .expect("bare require should create a real import symbol");

    assert_eq!(
        import_symbol.signature.as_deref(),
        Some("require(\"json\")")
    );
    assert_eq!(
        import_symbol
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("source"))
            .and_then(|value| value.as_str()),
        Some("json")
    );
    assert_eq!(
        import_symbol
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("importContext"))
            .and_then(|value| value.as_str()),
        Some("require(\"json\")")
    );

    assert!(
        relationships.is_empty(),
        "bare require should not create call relationships, got {:?}",
        relationships
    );

    let pending = extractor.get_structured_pending_relationships();
    let import_pending = pending
        .iter()
        .find(|pending| {
            pending.pending.kind == RelationshipKind::Imports
                && pending.target.display_name == "json"
        })
        .expect("bare require should create a structured import target");

    assert_eq!(import_pending.pending.from_symbol_id, import_symbol.id);
    assert_eq!(import_pending.target.terminal_name, "json");
    assert_eq!(
        import_pending.target.import_context.as_deref(),
        Some("require(\"json\")")
    );
}
