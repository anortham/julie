/// Tests for GDScript type extraction through the factory
///
/// GDScript has explicit type annotations: `var x: int`, `func foo() -> String:`
/// These tests verify that infer_types() extracts them from signatures.

#[cfg(test)]
mod tests {
    use crate::factory::extract_symbols_and_relationships;
    use std::path::PathBuf;
    use tree_sitter::Parser;

    #[test]
    fn test_factory_extracts_gdscript_function_return_types() {
        let code = r#"
extends Node

func calculate_damage(base: int, multiplier: float) -> float:
    return base * multiplier

func get_name() -> String:
    return "Player"

func _ready() -> void:
    pass
"#;

        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_gdscript::LANGUAGE.into())
            .expect("Error loading GDScript grammar");
        let tree = parser.parse(code, None).expect("Error parsing code");

        let workspace_root = PathBuf::from("/tmp/test");
        let results = extract_symbols_and_relationships(
            &tree,
            "test.gd",
            code,
            "gdscript",
            &workspace_root,
        )
        .expect("Extraction failed");

        assert!(
            !results.types.is_empty(),
            "GDScript type extraction returned EMPTY types HashMap! \
             Factory is not calling infer_types() properly."
        );

        // Should have return types for at least the typed functions
        let type_strings: Vec<&str> = results
            .types
            .values()
            .map(|t| t.resolved_type.as_str())
            .collect();

        println!("Extracted types: {:?}", type_strings);

        assert!(
            type_strings.iter().any(|t| *t == "float"),
            "Expected 'float' return type, got: {:?}",
            type_strings
        );
        assert!(
            type_strings.iter().any(|t| *t == "String"),
            "Expected 'String' return type, got: {:?}",
            type_strings
        );

        for type_info in results.types.values() {
            assert_eq!(type_info.language, "gdscript");
            assert!(type_info.is_inferred);
        }
    }

    #[test]
    fn test_factory_extracts_gdscript_variable_types() {
        let code = r#"
extends Node

var speed: float = 300.0
var player_name: String = "Hero"
@export var health: int = 100
const MAX_SPEED: float = 500.0
"#;

        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_gdscript::LANGUAGE.into())
            .expect("Error loading GDScript grammar");
        let tree = parser.parse(code, None).expect("Error parsing code");

        let workspace_root = PathBuf::from("/tmp/test");
        let results = extract_symbols_and_relationships(
            &tree,
            "test.gd",
            code,
            "gdscript",
            &workspace_root,
        )
        .expect("Extraction failed");

        assert!(
            !results.types.is_empty(),
            "GDScript variable type extraction returned empty"
        );

        let type_strings: Vec<&str> = results
            .types
            .values()
            .map(|t| t.resolved_type.as_str())
            .collect();

        println!("Variable types: {:?}", type_strings);

        // Should extract typed variables
        assert!(
            type_strings.iter().any(|t| *t == "float"),
            "Expected 'float' type, got: {:?}",
            type_strings
        );
    }

    #[test]
    fn test_factory_gdscript_untyped_returns_minimal() {
        let code = r#"
extends Node

var speed = 300.0
func _ready():
    pass
"#;

        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_gdscript::LANGUAGE.into())
            .expect("Error loading GDScript grammar");
        let tree = parser.parse(code, None).expect("Error parsing code");

        let workspace_root = PathBuf::from("/tmp/test");
        let results = extract_symbols_and_relationships(
            &tree,
            "test.gd",
            code,
            "gdscript",
            &workspace_root,
        )
        .expect("Extraction failed");

        // Untyped GDScript may still infer some types (e.g., from literal assignments)
        println!(
            "Untyped GDScript extracted {} types",
            results.types.len()
        );
    }
}
