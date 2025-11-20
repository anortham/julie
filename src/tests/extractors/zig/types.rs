/// Tests for Zig type extraction through the factory

#[cfg(test)]
mod tests {
    use crate::extractors::factory::extract_symbols_and_relationships;
    use std::path::PathBuf;
    use tree_sitter::Parser;

    #[test]
    fn test_factory_extracts_zig_types() {
        let code = r#"
const std = @import("std");

pub fn getUserName(userId: i32) []const u8 {
    return "User";
}

pub fn getAllUsers() []User {
    return &[_]User{};
}

pub fn getUserScores() std.StringHashMap(i32) {
    return std.StringHashMap(i32).init(allocator);
}
"#;

        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_zig::LANGUAGE.into())
            .expect("Error loading Zig grammar");
        let tree = parser.parse(code, None).expect("Error parsing code");

        let workspace_root = PathBuf::from("/tmp/test");
        let results =
            extract_symbols_and_relationships(&tree, "test.zig", code, "zig", &workspace_root)
                .expect("Extraction failed");

        assert!(
            !results.types.is_empty(),
            "Zig type extraction returned EMPTY types HashMap!"
        );

        println!("Extracted {} types from Zig code", results.types.len());
        for (symbol_id, type_info) in &results.types {
            println!(
                "  {} -> {} (inferred: {})",
                symbol_id, type_info.resolved_type, type_info.is_inferred
            );
        }

        assert!(results.types.len() >= 1);
        for type_info in results.types.values() {
            assert_eq!(type_info.language, "zig");
            assert!(type_info.is_inferred);
        }
    }
}
