/// Tests for Bash type extraction through the factory

#[cfg(test)]
mod tests {
    use crate::extractors::factory::extract_symbols_and_relationships;
    use std::path::PathBuf;
    use tree_sitter::Parser;

    #[test]
    fn test_factory_extracts_bash_types() {
        let code = r#"
#!/bin/bash

# Function with type comment
# @param $1 string username
# @return string greeting
get_greeting() {
    local username="$1"
    echo "Hello, $username"
}

# @param $1 int count
# @return int doubled value
double_value() {
    local count=$1
    echo $((count * 2))
}
"#;

        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_bash::LANGUAGE.into())
            .expect("Error loading Bash grammar");
        let tree = parser.parse(code, None).expect("Error parsing code");

        let workspace_root = PathBuf::from("/tmp/test");
        let results = extract_symbols_and_relationships(
            &tree,
            "test.sh",
            code,
            "bash",
            &workspace_root,
        )
        .expect("Extraction failed");

        assert!(
            !results.types.is_empty(),
            "Bash type extraction returned EMPTY types HashMap!"
        );

        println!("Extracted {} types from Bash code", results.types.len());
        for (symbol_id, type_info) in &results.types {
            println!("  {} -> {} (inferred: {})", symbol_id, type_info.resolved_type, type_info.is_inferred);
        }

        assert!(results.types.len() >= 1);
        for type_info in results.types.values() {
            assert_eq!(type_info.language, "bash");
            assert!(type_info.is_inferred);
        }
    }
}
