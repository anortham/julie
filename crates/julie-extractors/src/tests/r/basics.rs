// R Basics Tests
// Tests for core R features: functions, assignments, basic syntax

use super::*;
use crate::base::SymbolKind;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // Debug test to inspect AST
    fn debug_r_ast() {
        let r_code = r#"
x <- 42
y = 100
200 -> z
global_var <<- 500
"#;
        let tree = crate::tests::test_utils::init_parser(r_code, "r");
        let root = tree.root_node();

        fn print_ast(node: &tree_sitter::Node, depth: usize, code: &str) {
            let indent = "  ".repeat(depth);
            let node_text = node.utf8_text(code.as_bytes()).unwrap_or("<error>");
            let node_text_truncated = if node_text.len() > 50 {
                format!("{}...", &node_text[..50])
            } else {
                node_text.to_string()
            };

            println!(
                "{}{} [{}:{}] '{}'",
                indent,
                node.kind(),
                node.start_position().row,
                node.end_position().row,
                node_text_truncated.replace('\n', "\\n")
            );

            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                print_ast(&child, depth + 1, code);
            }
        }

        print_ast(&root, 0, r_code);
    }

    #[test]
    fn test_simple_function_definition() {
        let r_code = r#"
# Simple function definition
getUserData <- function(user_id) {
    data <- fetch_data(user_id)
    return(data)
}
"#;

        let symbols = extract_symbols(r_code);

        // Should extract the function
        let functions: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();

        assert_eq!(functions.len(), 1, "Should extract getUserData function");
        assert_eq!(functions[0].name, "getUserData");
    }

    #[test]
    fn test_assignment_operators() {
        let r_code = r#"
# R has multiple assignment operators
x <- 42              # Left assignment (most common)
y = 100              # Equals assignment
200 -> z             # Right assignment
global_var <<- 500   # Global assignment
"#;

        let symbols = extract_symbols(r_code);

        // Debug: print all symbols
        println!("Extracted {} symbols:", symbols.len());
        for sym in &symbols {
            println!("  {} ({:?})", sym.name, sym.kind);
        }

        // Should extract all variable assignments
        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();

        assert!(
            variables.len() >= 4,
            "Should extract all assignment operators (found {})",
            variables.len()
        );

        // Check for specific variables
        let var_names: Vec<&str> = variables.iter().map(|v| v.name.as_str()).collect();
        assert!(var_names.contains(&"x"), "Should find x variable");
        assert!(var_names.contains(&"y"), "Should find y variable");
        assert!(var_names.contains(&"z"), "Should find z variable");
        assert!(
            var_names.contains(&"global_var"),
            "Should find global_var variable"
        );
    }

    #[test]
    fn test_package_loading() {
        let r_code = r#"
# Loading packages
library(dplyr)
library(ggplot2)
require(tidyr)

# Using package namespace
result <- dplyr::filter(data, value > 10)
plot <- ggplot2::ggplot(data, aes(x, y))
"#;

        let symbols = extract_symbols(r_code);

        // Should extract library/require calls as imports
        let imports: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Import)
            .collect();

        assert!(
            imports.len() >= 3,
            "Should extract library() and require() calls as imports (found {})",
            imports.len()
        );

        let import_names: Vec<&str> = imports.iter().map(|s| s.name.as_str()).collect();
        assert!(import_names.contains(&"dplyr"), "Should find dplyr import");
        assert!(
            import_names.contains(&"ggplot2"),
            "Should find ggplot2 import"
        );
        assert!(import_names.contains(&"tidyr"), "Should find tidyr import");

        // Should also extract variables from namespace usage
        let variables: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .collect();
        assert!(variables.len() >= 2, "Should extract result and plot variables");
    }
}
