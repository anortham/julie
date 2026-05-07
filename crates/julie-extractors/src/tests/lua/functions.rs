use crate::lua::LuaExtractor;
use crate::tests::lua::init_parser;
use std::path::PathBuf;

fn extract_symbols(code: &str) -> Vec<crate::base::Symbol> {
    let mut parser = init_parser();
    let tree = parser.parse(code, None).expect("Failed to parse Lua code");
    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = LuaExtractor::new(
        "lua".to_string(),
        "test.lua".to_string(),
        code.to_string(),
        &workspace_root,
    );

    extractor.extract_symbols(&tree)
}

#[test]
fn test_lua_function_signature_does_not_include_body() {
    let code = r#"
local function helper(name)
    print(name)
    return name
end
"#;

    let symbols = extract_symbols(code);
    let helper = symbols
        .iter()
        .find(|symbol| symbol.name == "helper")
        .expect("missing helper symbol");

    assert_eq!(
        helper.signature.as_deref(),
        Some("local function helper(name)")
    );
}
