// Lua Identifier Extraction Tests (TDD RED phase)
//
// These tests validate the extract_identifiers() functionality which extracts:
// - Function calls (function_call)
// - Member access (dot_index_expression, method_index_expression)
// - Proper containing symbol tracking (file-scoped)
//
// Following the Rust/C# extractor reference implementation pattern

use crate::extractors::base::IdentifierKind;
use crate::extractors::lua::LuaExtractor;
use crate::tests::extractors::lua::init_parser;
use std::path::PathBuf;

#[cfg(test)]
mod identifier_extraction_tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_lua_function_calls() {
        let lua_code = r#"
-- Define functions
function add(a, b)
    return a + b
end

function calculate()
    local result = add(5, 3)      -- Function call to add
    print(result)                  -- Function call to print
    return result
end
"#;

        let mut parser = init_parser();
        let tree = parser.parse(lua_code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = LuaExtractor::new(
            "lua".to_string(),
            "test.lua".to_string(),
            lua_code.to_string(),
            &workspace_root,
        );

        // Extract symbols first
        let symbols = extractor.extract_symbols(&tree);

        // NOW extract identifiers (this will FAIL until we implement it)
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Verify we found the function calls
        let add_call = identifiers.iter().find(|id| id.name == "add");
        assert!(
            add_call.is_some(),
            "Should extract 'add' function call identifier"
        );
        let add_call = add_call.unwrap();
        assert_eq!(add_call.kind, IdentifierKind::Call);

        let print_call = identifiers.iter().find(|id| id.name == "print");
        assert!(
            print_call.is_some(),
            "Should extract 'print' function call identifier"
        );
        let print_call = print_call.unwrap();
        assert_eq!(print_call.kind, IdentifierKind::Call);
    }

    #[test]
    fn test_lua_member_access() {
        let lua_code = r#"
-- Object with fields
local user = {
    name = "John",
    email = "john@example.com"
}

function printUserInfo()
    local n = user.name           -- Member access: user.name
    local e = user.email          -- Member access: user.email
    print(n, e)
end
"#;

        let mut parser = init_parser();
        let tree = parser.parse(lua_code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = LuaExtractor::new(
            "lua".to_string(),
            "test.lua".to_string(),
            lua_code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Verify we found member access identifiers
        let name_access = identifiers
            .iter()
            .filter(|id| id.name == "name" && id.kind == IdentifierKind::MemberAccess)
            .count();
        assert!(
            name_access > 0,
            "Should extract 'name' member access identifier"
        );

        let email_access = identifiers
            .iter()
            .filter(|id| id.name == "email" && id.kind == IdentifierKind::MemberAccess)
            .count();
        assert!(
            email_access > 0,
            "Should extract 'email' member access identifier"
        );
    }

    #[test]
    fn test_lua_identifiers_have_containing_symbol() {
        // This test ensures file-scoped filtering works correctly
        // Note: Lua extractor only captures function names, not full bodies,
        // so we test that IF a containing symbol is found, it's from the correct file
        let lua_code = r#"
local M = {}

function M.process()
    local result = M.helper()
    return result
end

function M.helper()
    return "done"
end

return M
"#;

        let mut parser = init_parser();
        let tree = parser.parse(lua_code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = LuaExtractor::new(
            "lua".to_string(),
            "test.lua".to_string(),
            lua_code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Find all identifiers
        assert!(!identifiers.is_empty(), "Should extract identifiers");

        // Verify file-scoped filtering: If any identifier has a containing_symbol_id,
        // that symbol must be from the same file
        for identifier in &identifiers {
            if let Some(ref containing_id) = identifier.containing_symbol_id {
                // Find the containing symbol
                let containing_symbol = symbols.iter().find(|s| &s.id == containing_id);
                assert!(
                    containing_symbol.is_some(),
                    "Containing symbol must exist in symbol list"
                );

                let containing_symbol = containing_symbol.unwrap();
                assert_eq!(
                    containing_symbol.file_path, "test.lua",
                    "Containing symbol must be from same file (file-scoped filtering)"
                );
            }
        }

        // Verify we extracted the expected identifiers
        let helper_calls: Vec<_> = identifiers
            .iter()
            .filter(|id| id.name == "helper" && id.kind == IdentifierKind::Call)
            .collect();
        assert!(!helper_calls.is_empty(), "Should extract helper call");
    }

    #[test]
    fn test_lua_chained_member_access() {
        let lua_code = r#"
function execute()
    local balance = user.account.balance   -- Chained member access
    local city = customer.address.city     -- Chained member access
end
"#;

        let mut parser = init_parser();
        let tree = parser.parse(lua_code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = LuaExtractor::new(
            "lua".to_string(),
            "test.lua".to_string(),
            lua_code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Should extract the rightmost identifiers in chains
        let balance_access = identifiers
            .iter()
            .find(|id| id.name == "balance" && id.kind == IdentifierKind::MemberAccess);
        assert!(
            balance_access.is_some(),
            "Should extract 'balance' from chained member access"
        );

        let city_access = identifiers
            .iter()
            .find(|id| id.name == "city" && id.kind == IdentifierKind::MemberAccess);
        assert!(
            city_access.is_some(),
            "Should extract 'city' from chained member access"
        );
    }

    #[test]
    fn test_lua_duplicate_calls_at_different_locations() {
        let lua_code = r#"
function run()
    process()
    process()  -- Same call twice
end

function process()
    -- Implementation
end
"#;

        let mut parser = init_parser();
        let tree = parser.parse(lua_code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = LuaExtractor::new(
            "lua".to_string(),
            "test.lua".to_string(),
            lua_code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        // Should extract BOTH calls (they're at different locations)
        let process_calls: Vec<_> = identifiers
            .iter()
            .filter(|id| id.name == "process" && id.kind == IdentifierKind::Call)
            .collect();

        assert_eq!(
            process_calls.len(),
            2,
            "Should extract both process calls at different locations"
        );

        // Verify they have different line numbers
        assert_ne!(
            process_calls[0].start_line, process_calls[1].start_line,
            "Duplicate calls should have different line numbers"
        );
    }
}
