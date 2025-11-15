use crate::extractors::base::RelationshipKind;
use crate::extractors::lua::LuaExtractor;
use crate::tests::extractors::lua::init_parser;
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
