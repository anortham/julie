// LuaDoc comment extraction tests
#[cfg(test)]
mod tests {
    use crate::extractors::base::SymbolKind;
    use crate::extractors::lua::LuaExtractor;
    use std::path::PathBuf;

    fn extract_symbols(code: &str) -> Vec<crate::extractors::base::Symbol> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_lua::LANGUAGE.into())
            .expect("Error loading Lua grammar");
        let tree = parser.parse(code, None).expect("Failed to parse code");
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
    fn test_extract_luadoc_single_line_comment_from_function() {
        let code = r#"--- Validates user credentials
-- Checks username and password against database
function validate_credentials(username)
    return true
end"#;
        let symbols = extract_symbols(code);
        let func = symbols
            .iter()
            .find(|s| s.name == "validate_credentials")
            .expect("Function not found");
        assert_eq!(func.kind, SymbolKind::Function);
        assert!(func.doc_comment.is_some());
        let doc = func.doc_comment.as_ref().unwrap();
        assert!(doc.contains("Validates user credentials"));
        assert!(doc.contains("Checks username and password"));
    }

    #[test]
    fn test_extract_luadoc_block_comment_from_table() {
        let code = r#"--[[
    UserService manages user authentication
    Provides login and logout functionality
]]
UserService = {}"#;
        let symbols = extract_symbols(code);
        let table = symbols
            .iter()
            .find(|s| s.name == "UserService")
            .expect("Table not found");
        assert!(table.doc_comment.is_some());
        let doc = table.doc_comment.as_ref().unwrap();
        assert!(doc.contains("manages user authentication"));
    }

    #[test]
    fn test_extract_luadoc_from_local_function() {
        let code = r#"--- Internal helper function
-- Not exposed to public API
local function process_data(data)
    return data
end"#;
        let symbols = extract_symbols(code);
        let func = symbols
            .iter()
            .find(|s| s.name == "process_data")
            .expect("Local function not found");
        assert!(func.doc_comment.is_some());
        let doc = func.doc_comment.as_ref().unwrap();
        assert!(doc.contains("Internal helper function"));
    }

    #[test]
    fn test_extract_luadoc_from_variable() {
        let code = r#"--- Configuration table for the application
-- Contains all runtime settings
config = {
    port = 8080,
    debug = false
}"#;
        let symbols = extract_symbols(code);
        let var = symbols
            .iter()
            .find(|s| s.name == "config")
            .expect("Variable not found");
        assert!(var.doc_comment.is_some());
        let doc = var.doc_comment.as_ref().unwrap();
        assert!(doc.contains("Configuration table"));
    }

    #[test]
    fn test_no_doc_comment_for_undocumented_function() {
        let code = r#"function undocumented()
    return 42
end"#;
        let symbols = extract_symbols(code);
        let func = symbols
            .iter()
            .find(|s| s.name == "undocumented")
            .expect("Function not found");
        assert!(func.doc_comment.is_none());
    }

    #[test]
    fn test_luadoc_block_comment_multiline() {
        let code = r#"--[[
    Complex algorithm implementation
    Time complexity: O(n log n)
    Space complexity: O(n)
]]
function complex_sort(array)
    return array
end"#;
        let symbols = extract_symbols(code);
        let func = symbols
            .iter()
            .find(|s| s.name == "complex_sort")
            .expect("Function not found");
        assert!(func.doc_comment.is_some());
        let doc = func.doc_comment.as_ref().unwrap();
        assert!(doc.contains("Complex algorithm"));
        assert!(doc.contains("Time complexity"));
    }

    #[test]
    fn test_extract_luadoc_with_param_tags() {
        let code = r#"--- Adds two numbers
-- @param a number The first number
-- @param b number The second number
-- @return number The sum of a and b
function add(a, b)
    return a + b
end"#;
        let symbols = extract_symbols(code);
        let func = symbols
            .iter()
            .find(|s| s.name == "add")
            .expect("Function not found");
        assert!(func.doc_comment.is_some());
        let doc = func.doc_comment.as_ref().unwrap();
        assert!(doc.contains("Adds two numbers"));
        assert!(doc.contains("@param a"));
        assert!(doc.contains("@return number"));
    }
}
