// QML Functions Tests
// Tests for QML function declarations and JavaScript code

use super::*;
use crate::extractors::base::SymbolKind;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_simple_function() {
        let qml_code = r#"
import QtQuick 2.15

Item {
    function calculateSum(a, b) {
        return a + b
    }
}
"#;

        let symbols = extract_symbols(qml_code);

        let functions: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();

        assert_eq!(functions.len(), 1, "Should extract calculateSum function");
        assert_eq!(functions[0].name, "calculateSum");
    }

    #[test]
    fn test_extract_function_with_return_type() {
        let qml_code = r#"
import QtQuick 2.15

Item {
    function getName() : string {
        return "John Doe"
    }

    function getAge() : int {
        return 25
    }

    function isValid() : bool {
        return true
    }
}
"#;

        let symbols = extract_symbols(qml_code);

        let functions: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();

        assert_eq!(functions.len(), 3, "Should extract all three typed functions");
    }

    #[test]
    fn test_extract_function_with_complex_body() {
        let qml_code = r#"
import QtQuick 2.15

Item {
    function processData(items) {
        let result = []
        for (let i = 0; i < items.length; i++) {
            if (items[i] > 10) {
                result.push(items[i] * 2)
            }
        }
        return result
    }
}
"#;

        let symbols = extract_symbols(qml_code);

        let functions: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();

        assert_eq!(functions.len(), 1, "Should extract processData function");
    }

    #[test]
    fn test_extract_arrow_functions() {
        let qml_code = r#"
import QtQuick 2.15

Item {
    property var doubleValue: (x) => x * 2
    property var sum: (a, b) => a + b

    Component.onCompleted: {
        let filtered = items.filter(item => item.active)
        let mapped = items.map(item => item.value)
    }
}
"#;

        let symbols = extract_symbols(qml_code);

        // Arrow functions might be extracted as properties or not extracted separately
        // Document expected behavior
    }

    #[test]
    fn test_extract_nested_functions() {
        let qml_code = r#"
import QtQuick 2.15

Item {
    function outer() {
        function inner() {
            return 42
        }
        return inner()
    }
}
"#;

        let symbols = extract_symbols(qml_code);

        let functions: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();

        // Should extract outer function, inner might or might not be extracted
        assert!(functions.len() >= 1, "Should extract at least outer function");
    }

    #[test]
    fn test_extract_function_with_default_parameters() {
        let qml_code = r#"
import QtQuick 2.15

Item {
    function greet(name = "World") {
        return "Hello, " + name
    }

    function calculate(a, b = 10, c = 5) {
        return a + b + c
    }
}
"#;

        let symbols = extract_symbols(qml_code);

        let functions: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();

        assert_eq!(functions.len(), 2, "Should extract both functions with defaults");
    }

    #[test]
    fn test_extract_async_functions() {
        let qml_code = r#"
import QtQuick 2.15

Item {
    function async fetchData(url) {
        // Async function
        return fetch(url).then(response => response.json())
    }
}
"#;

        let symbols = extract_symbols(qml_code);

        let functions: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();

        assert_eq!(functions.len(), 1, "Should extract async function");
    }

    #[test]
    fn test_extract_javascript_blocks() {
        let qml_code = r#"
import QtQuick 2.15
import "utils.js" as Utils

Item {
    function useJavaScript() {
        let data = Utils.processData([1, 2, 3])
        console.log(JSON.stringify(data))
    }
}
"#;

        let symbols = extract_symbols(qml_code);

        let functions: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();

        assert_eq!(functions.len(), 1, "Should extract JavaScript function");
    }

    #[test]
    fn test_extract_multiple_functions() {
        let qml_code = r#"
import QtQuick 2.15

Item {
    function initialize() {
        loadData()
        setupUI()
    }

    function loadData() {
        // Load data
    }

    function setupUI() {
        // Setup UI
    }

    function cleanup() {
        // Cleanup
    }

    function validate() : bool {
        return true
    }
}
"#;

        let symbols = extract_symbols(qml_code);

        let functions: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();

        assert_eq!(functions.len(), 5, "Should extract all five functions");
    }
}
