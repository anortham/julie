/// Tests for PowerShell type extraction through the factory

#[cfg(test)]
mod tests {
    use crate::factory::extract_symbols_and_relationships;
    use std::path::PathBuf;
    use tree_sitter::Parser;

    #[test]
    fn test_factory_extracts_powershell_types() {
        let code = r#"
function Get-UserName {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory=$true)]
        [int]$UserId
    )

    return "User$UserId"
}

function Get-AllUsers {
    [CmdletBinding()]
    [OutputType([System.Collections.ArrayList])]
    param()

    return @()
}
"#;

        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_powershell::LANGUAGE.into())
            .expect("Error loading PowerShell grammar");
        let tree = parser.parse(code, None).expect("Error parsing code");

        let workspace_root = PathBuf::from("/tmp/test");
        let results = extract_symbols_and_relationships(
            &tree,
            "test.ps1",
            code,
            "powershell",
            &workspace_root,
        )
        .expect("Extraction failed");

        assert!(
            !results.types.is_empty(),
            "PowerShell type extraction returned EMPTY types HashMap!"
        );

        println!("Extracted {} types from PowerShell code", results.types.len());
        for (symbol_id, type_info) in &results.types {
            println!("  {} -> {} (inferred: {})", symbol_id, type_info.resolved_type, type_info.is_inferred);
        }

        assert!(results.types.len() >= 1);
        for type_info in results.types.values() {
            assert_eq!(type_info.language, "powershell");
            assert!(type_info.is_inferred);
        }
    }
}
