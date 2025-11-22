/// Tests for Razor type extraction through the factory

#[cfg(test)]
mod tests {
    use crate::factory::extract_symbols_and_relationships;
    use std::path::PathBuf;
    use tree_sitter::Parser;

    #[test]
    fn test_factory_extracts_razor_types() {
        let code = r#"
@page "/users"
@using System.Collections.Generic

@code {
    private List<User> users;
    private string searchTerm = "";

    protected override async Task OnInitializedAsync()
    {
        users = await UserService.GetAllAsync();
    }

    private async Task<bool> DeleteUser(int userId)
    {
        return await UserService.DeleteAsync(userId);
    }
}
"#;

        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_c_sharp::LANGUAGE.into())
            .expect("Error loading Razor grammar");
        let tree = parser.parse(code, None).expect("Error parsing code");

        let workspace_root = PathBuf::from("/tmp/test");
        let results = extract_symbols_and_relationships(
            &tree,
            "test.razor",
            code,
            "razor",
            &workspace_root,
        )
        .expect("Extraction failed");

        assert!(
            !results.types.is_empty(),
            "Razor type extraction returned EMPTY types HashMap!"
        );

        println!("Extracted {} types from Razor code", results.types.len());
        for (symbol_id, type_info) in &results.types {
            println!("  {} -> {} (inferred: {})", symbol_id, type_info.resolved_type, type_info.is_inferred);
        }

        assert!(results.types.len() >= 1);
        for type_info in results.types.values() {
            assert_eq!(type_info.language, "razor");
            assert!(type_info.is_inferred);
        }
    }
}
