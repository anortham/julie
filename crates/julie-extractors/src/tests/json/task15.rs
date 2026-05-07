use crate::extract_canonical;
use crate::json::JsonExtractor;
use crate::lua::LuaExtractor;
use crate::markdown::MarkdownExtractor;
use crate::r::RExtractor;
use crate::tests::helpers::init_parser;
use crate::toml::TomlExtractor;
use crate::yaml::YamlExtractor;
use std::path::PathBuf;

#[test]
fn test_missing_infer_types_languages_return_contract_consistent_results() {
    let workspace_root = PathBuf::from("/tmp/test");
    let canonical_cases = [
        ("test.lua", "local answer = 42"),
        ("test.md", "# Heading\n\nBody"),
        ("test.json", r#"{"answer": 42}"#),
        ("test.toml", "answer = 42"),
        ("test.yaml", "answer: 42"),
        ("test.R", "answer <- 42"),
    ];

    for (file_name, code) in canonical_cases {
        let results = extract_canonical(file_name, code, &workspace_root)
            .expect("Canonical extraction should succeed for contract test");
        assert!(
            results.types.is_empty(),
            "Expected empty types map for {file_name}, got {} entries",
            results.types.len()
        );
    }

    let lua_code = "local count = 1";
    let lua_tree = init_parser(lua_code, "lua");
    let mut lua_extractor = LuaExtractor::new(
        "lua".to_string(),
        "test.lua".to_string(),
        lua_code.to_string(),
        &workspace_root,
    );
    let lua_symbols = lua_extractor.extract_symbols(&lua_tree);
    let lua_types = lua_extractor.infer_types(&lua_symbols);
    assert!(
        lua_types.is_empty(),
        "Lua infer_types contract should be empty"
    );
    assert_eq!(lua_types, lua_extractor.infer_types(&lua_symbols));

    let markdown_code = "# Heading\n\nBody";
    let markdown_tree = init_parser(markdown_code, "markdown");
    let mut markdown_extractor = MarkdownExtractor::new(
        "markdown".to_string(),
        "test.md".to_string(),
        markdown_code.to_string(),
        &workspace_root,
    );
    let markdown_symbols = markdown_extractor.extract_symbols(&markdown_tree);
    let markdown_types = markdown_extractor.infer_types(&markdown_symbols);
    assert!(
        markdown_types.is_empty(),
        "Markdown infer_types contract should be empty"
    );
    assert_eq!(
        markdown_types,
        markdown_extractor.infer_types(&markdown_symbols)
    );

    let json_code = r#"{"answer": 42}"#;
    let json_tree = init_parser(json_code, "json");
    let mut json_extractor = JsonExtractor::new(
        "json".to_string(),
        "test.json".to_string(),
        json_code.to_string(),
        &workspace_root,
    );
    let json_symbols = json_extractor.extract_symbols(&json_tree);
    let json_types = json_extractor.infer_types(&json_symbols);
    assert!(
        json_types.is_empty(),
        "JSON infer_types contract should be empty"
    );
    assert_eq!(json_types, json_extractor.infer_types(&json_symbols));

    let toml_code = "answer = 42";
    let toml_tree = init_parser(toml_code, "toml");
    let mut toml_extractor = TomlExtractor::new(
        "toml".to_string(),
        "test.toml".to_string(),
        toml_code.to_string(),
        &workspace_root,
    );
    let toml_symbols = toml_extractor.extract_symbols(&toml_tree);
    let toml_types = toml_extractor.infer_types(&toml_symbols);
    assert!(
        toml_types.is_empty(),
        "TOML infer_types contract should be empty"
    );
    assert_eq!(toml_types, toml_extractor.infer_types(&toml_symbols));

    let yaml_code = "answer: 42";
    let yaml_tree = init_parser(yaml_code, "yaml");
    let mut yaml_extractor = YamlExtractor::new(
        "yaml".to_string(),
        "test.yaml".to_string(),
        yaml_code.to_string(),
        &workspace_root,
    );
    let yaml_symbols = yaml_extractor.extract_symbols(&yaml_tree);
    let yaml_types = yaml_extractor.infer_types(&yaml_symbols);
    assert!(
        yaml_types.is_empty(),
        "YAML infer_types contract should be empty"
    );
    assert_eq!(yaml_types, yaml_extractor.infer_types(&yaml_symbols));

    let r_code = "answer <- 42";
    let r_tree = init_parser(r_code, "r");
    let mut r_extractor = RExtractor::new(
        "r".to_string(),
        "test.R".to_string(),
        r_code.to_string(),
        &workspace_root,
    );
    let r_symbols = r_extractor.extract_symbols(&r_tree);
    let r_types = r_extractor.infer_types(&r_symbols);
    assert!(r_types.is_empty(), "R infer_types contract should be empty");
    assert_eq!(r_types, r_extractor.infer_types(&r_symbols));
}
