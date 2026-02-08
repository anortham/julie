// Bash Control Flow Noise Verification Test
//
// Verifies that control flow blocks (for, while, if, case) are NOT extracted as symbols.
// This test confirms the fix for the extractor audit issue #4.

#[cfg(test)]
mod control_flow_verification {
    use crate::base::SymbolKind;
    use crate::bash::BashExtractor;
    use std::path::PathBuf;
    use tree_sitter::Parser;

    fn init_parser() -> Parser {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_bash::LANGUAGE.into())
            .expect("Error loading Bash grammar");
        parser
    }

    fn extract_symbols(code: &str) -> Vec<crate::base::Symbol> {
        let workspace_root = PathBuf::from("/tmp/test");
        let mut parser = init_parser();
        let tree = parser.parse(code, None).expect("Failed to parse code");
        let mut extractor = BashExtractor::new(
            "bash".to_string(),
            "test.sh".to_string(),
            code.to_string(),
            &workspace_root,
        );
        extractor.extract_symbols(&tree)
    }

    #[test]
    fn test_control_flow_not_extracted_as_symbols() {
        let code = r#"
#!/bin/bash

# Test for loops
for item in "$@"; do
    echo "$item"
done

# Test while loops
while true; do
    sleep 1
done

# Test if statements
if [ -f "$1" ]; then
    cat "$1"
fi

# Test case statements
case "$1" in
    start) echo "starting" ;;
    stop) echo "stopping" ;;
esac

# Test nested control flow
for i in {1..10}; do
    if [ "$i" -gt 5 ]; then
        while [ "$i" -lt 8 ]; do
            echo "$i"
            i=$((i+1))
        done
    fi
done
"#;

        let symbols = extract_symbols(code);

        // Control flow should NOT produce Method symbols with synthetic names
        assert!(
            !symbols.iter().any(|s| s.name.contains("for") && s.kind == SymbolKind::Method),
            "for loops should not be Method symbols"
        );
        assert!(
            !symbols.iter().any(|s| s.name.contains("while") && s.kind == SymbolKind::Method),
            "while loops should not be Method symbols"
        );
        assert!(
            !symbols.iter().any(|s| s.name.contains("if") && s.kind == SymbolKind::Method),
            "if statements should not be Method symbols"
        );
        assert!(
            !symbols.iter().any(|s| s.name.contains("case") && s.kind == SymbolKind::Method),
            "case statements should not be Method symbols"
        );

        // No synthetic block names should be present
        assert!(
            !symbols.iter().any(|s| s.name.contains("block")),
            "No synthetic 'block' names should be present"
        );

        // We should only have extracted actual symbols (like 'echo', 'sleep', 'cat')
        let function_symbols: Vec<_> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();

        // All function symbols should be actual commands, not control flow
        for sym in function_symbols.iter() {
            assert!(
                !["for", "while", "if", "case", "block"].iter().any(|keyword| sym.name.contains(keyword)),
                "Symbol '{}' should not be a control flow keyword",
                sym.name
            );
        }
    }

    #[test]
    fn test_control_flow_with_functions() {
        // Verify that control flow inside functions doesn't produce spurious symbols
        let code = r#"
#!/bin/bash

deploy_app() {
    # This function has control flow but should only produce one symbol
    for service in api worker frontend; do
        echo "Deploying $service"

        if [ -f "config/$service.conf" ]; then
            while IFS= read -r line; do
                echo "$line"
            done < "config/$service.conf"
        fi
    done

    case "$1" in
        production)
            echo "Production deployment"
            ;;
        staging)
            echo "Staging deployment"
            ;;
    esac
}
"#;

        let symbols = extract_symbols(code);

        // Should extract the function
        let functions: Vec<_> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function && s.name == "deploy_app")
            .collect();
        assert_eq!(functions.len(), 1, "Should extract exactly one deploy_app function");

        // Should not extract control flow as symbols
        let control_flow_symbols: Vec<_> = symbols
            .iter()
            .filter(|s| {
                s.kind == SymbolKind::Method
                    && (s.name.contains("for")
                        || s.name.contains("while")
                        || s.name.contains("if")
                        || s.name.contains("case")
                        || s.name.contains("block"))
            })
            .collect();
        assert_eq!(
            control_flow_symbols.len(),
            0,
            "Control flow should not produce Method symbols"
        );
    }

    #[test]
    fn test_case_statement_details() {
        // Specific test for case statements which were not explicitly handled in original code
        let code = r#"
#!/bin/bash

# Test various case patterns
case "$1" in
    start|begin)
        echo "Starting service"
        ;;
    stop|end)
        echo "Stopping service"
        ;;
    restart)
        echo "Restarting service"
        ;;
    *)
        echo "Unknown command"
        ;;
esac

# Nested case in function
handle_command() {
    case "$1" in
        deploy)
            case "$2" in
                production) echo "prod" ;;
                staging) echo "stage" ;;
            esac
            ;;
        test)
            echo "Testing"
            ;;
    esac
}
"#;

        let symbols = extract_symbols(code);

        // Should extract the function
        let functions: Vec<_> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function && s.name == "handle_command")
            .collect();
        assert_eq!(functions.len(), 1, "Should extract handle_command function");

        // Should NOT extract case statements as symbols
        let case_symbols: Vec<_> = symbols
            .iter()
            .filter(|s| {
                s.kind == SymbolKind::Method
                    && (s.name.contains("case") || s.name.contains("pattern"))
            })
            .collect();
        assert_eq!(
            case_symbols.len(),
            0,
            "case statements should not produce Method symbols"
        );
    }
}
