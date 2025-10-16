// Inline tests extracted from extractors/lua/mod.rs
//
// These tests verify the LuaExtractor initialization and core functionality.
// Original location: src/extractors/lua/mod.rs (lines 85-98)

#[cfg(test)]
mod tests {
    use crate::extractors::lua::LuaExtractor;

    #[test]
    fn test_lua_extractor_initialization() {
        let extractor = LuaExtractor::new(
            "lua".to_string(),
            "test.lua".to_string(),
            "function hello() end".to_string(),
        );
        assert_eq!(extractor.base().file_path, "test.lua");
    }
}
