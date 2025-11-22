// QML Identifiers Tests
// Tests for identifier extraction: function calls, member access, variable references

use super::*;
use crate::base::{IdentifierKind, SymbolKind};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_function_call_identifiers() {
        let qml_code = r#"
import QtQuick 2.15

Item {
    function processData(items) {
        let result = calculateSum(items)
        return formatResult(result)
    }

    function calculateSum(arr) { return 0 }
    function formatResult(val) { return val }
}
"#;

        let identifiers = extract_identifiers(qml_code);

        // Should extract identifiers for calculateSum and formatResult calls
        let call_identifiers: Vec<&Identifier> = identifiers
            .iter()
            .filter(|id| id.kind == IdentifierKind::Call)
            .collect();

        assert!(
            call_identifiers.len() >= 2,
            "Should extract at least 2 function call identifiers"
        );

        let call_names: Vec<&str> = call_identifiers.iter().map(|id| id.name.as_str()).collect();
        assert!(
            call_names.contains(&"calculateSum"),
            "Should extract calculateSum call identifier"
        );
        assert!(
            call_names.contains(&"formatResult"),
            "Should extract formatResult call identifier"
        );
    }

    #[test]
    fn test_extract_member_access_identifiers() {
        let qml_code = r#"
import QtQuick 2.15

Rectangle {
    id: container

    Rectangle {
        id: child
        width: parent.width
        height: container.height
        anchors.fill: parent
    }
}
"#;

        let identifiers = extract_identifiers(qml_code);

        // Should extract member access identifiers for parent.width, container.height, etc.
        let member_access_identifiers: Vec<&Identifier> = identifiers
            .iter()
            .filter(|id| id.kind == IdentifierKind::MemberAccess)
            .collect();

        assert!(
            member_access_identifiers.len() >= 2,
            "Should extract member access identifiers for property access"
        );

        let member_names: Vec<&str> = member_access_identifiers
            .iter()
            .map(|id| id.name.as_str())
            .collect();

        // Should find property access patterns
        assert!(
            member_names.iter().any(|&name| name == "width" || name == "height"),
            "Should extract property access identifiers"
        );
    }

    #[test]
    fn test_extract_variable_reference_identifiers() {
        let qml_code = r#"
import QtQuick 2.15

Item {
    function processItems(items) {
        let count = items.length
        let result = []

        for (let i = 0; i < count; i++) {
            result.push(items[i])
        }

        return result
    }
}
"#;

        let identifiers = extract_identifiers(qml_code);

        // Should extract variable references for items, count, result, i
        let var_ref_identifiers: Vec<&Identifier> = identifiers
            .iter()
            .filter(|id| id.kind == IdentifierKind::VariableRef)
            .collect();

        assert!(
            var_ref_identifiers.len() >= 2,
            "Should extract variable reference identifiers"
        );
    }

    #[test]
    fn test_extract_signal_handler_identifiers() {
        let qml_code = r#"
import QtQuick 2.15

Rectangle {
    signal customSignal()

    function myHandler() {
        console.log("Handler called")
    }

    MouseArea {
        onClicked: myHandler()
        onPressed: customSignal()
    }
}
"#;

        let identifiers = extract_identifiers(qml_code);

        // Should extract identifiers for myHandler and customSignal calls
        let call_identifiers: Vec<&Identifier> = identifiers
            .iter()
            .filter(|id| id.kind == IdentifierKind::Call)
            .collect();

        assert!(
            call_identifiers.len() >= 2,
            "Should extract call identifiers from signal handlers"
        );

        let call_names: Vec<&str> = call_identifiers.iter().map(|id| id.name.as_str()).collect();
        assert!(
            call_names.contains(&"myHandler"),
            "Should extract myHandler call"
        );
        assert!(
            call_names.contains(&"customSignal"),
            "Should extract customSignal call"
        );
    }

    #[test]
    fn test_extract_console_log_identifiers() {
        let qml_code = r#"
import QtQuick 2.15

Item {
    function debugInfo(message) {
        console.log(message)
        console.error("Error:", message)
        console.warn("Warning")
    }
}
"#;

        let identifiers = extract_identifiers(qml_code);

        // Should extract member access for console.log, console.error, console.warn
        let member_identifiers: Vec<&Identifier> = identifiers
            .iter()
            .filter(|id| id.kind == IdentifierKind::MemberAccess || id.kind == IdentifierKind::Call)
            .collect();

        assert!(
            member_identifiers.len() >= 3,
            "Should extract console method identifiers"
        );
    }

    #[test]
    fn test_identifier_location_accuracy() {
        let qml_code = r#"
import QtQuick 2.15

Item {
    function test() {
        calculateSum(10, 20)
    }
}
"#;

        let identifiers = extract_identifiers(qml_code);

        let calc_sum_id = identifiers
            .iter()
            .find(|id| id.name == "calculateSum")
            .expect("Should find calculateSum identifier");

        // Verify position information is captured
        assert!(calc_sum_id.start_line > 0, "Should have valid start_line");
        assert!(calc_sum_id.end_line > 0, "Should have valid end_line");
        assert!(
            calc_sum_id.start_line <= calc_sum_id.end_line,
            "start_line should be <= end_line"
        );
    }
}
