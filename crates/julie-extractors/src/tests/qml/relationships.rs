// QML Relationships Tests
// Tests for relationship extraction: function calls, signal connections, component instantiation

use super::*;
use crate::base::{RelationshipKind, SymbolKind};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_function_call_relationship() {
        let qml_code = r#"
import QtQuick 2.15

Item {
    function calculateTotal(items) {
        return sumValues(items)
    }

    function sumValues(arr) {
        let total = 0
        for (let i = 0; i < arr.length; i++) {
            total += arr[i]
        }
        return total
    }
}
"#;

        let (symbols, relationships) = extract_symbols_and_relationships(qml_code);

        // Verify we have both functions
        let functions: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();
        assert_eq!(functions.len(), 2, "Should extract both functions");

        // Verify call relationship: calculateTotal calls sumValues
        let call_relationships: Vec<&Relationship> = relationships
            .iter()
            .filter(|r| r.kind == RelationshipKind::Calls)
            .collect();

        assert!(
            call_relationships.len() >= 1,
            "Should extract at least one call relationship"
        );

        // Find the specific call from calculateTotal to sumValues
        let calculate_total = functions
            .iter()
            .find(|f| f.name == "calculateTotal")
            .expect("Should find calculateTotal function");
        let sum_values = functions
            .iter()
            .find(|f| f.name == "sumValues")
            .expect("Should find sumValues function");

        let call_rel = call_relationships
            .iter()
            .find(|r| {
                r.from_symbol_id == calculate_total.id && r.to_symbol_id == sum_values.id
            })
            .expect("Should find call relationship from calculateTotal to sumValues");

        assert_eq!(call_rel.kind, RelationshipKind::Calls);
    }

    #[test]
    fn test_extract_signal_handler_call_relationship() {
        let qml_code = r#"
import QtQuick 2.15

Rectangle {
    id: button

    signal clicked()

    function handleClick() {
        console.log("Button clicked")
    }

    MouseArea {
        anchors.fill: parent
        onClicked: button.handleClick()
    }
}
"#;

        let (symbols, relationships) = extract_symbols_and_relationships(qml_code);

        // Should have relationships for signal handler calling the function
        let call_relationships: Vec<&Relationship> = relationships
            .iter()
            .filter(|r| r.kind == RelationshipKind::Calls)
            .collect();

        assert!(
            call_relationships.len() >= 1,
            "Should extract call relationship from signal handler to handleClick"
        );
    }

    #[test]
    fn test_extract_component_instantiation_relationship() {
        let qml_code = r#"
import QtQuick 2.15

Item {
    Rectangle {
        id: rect1
        width: 100
        height: 100
    }

    Text {
        id: label
        text: "Hello"
    }
}
"#;

        let (symbols, relationships) = extract_symbols_and_relationships(qml_code);

        // Only the root component (Item) is extracted as a Class symbol.
        // Nested components (Rectangle, Text) are no longer extracted,
        // so there are no instantiation relationships for them.
        let components: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Class)
            .collect();

        assert_eq!(
            components.len(),
            1,
            "Should extract only the root Item component"
        );
        assert_eq!(components[0].name, "Item");
    }

    #[test]
    fn test_extract_nested_function_calls() {
        let qml_code = r#"
import QtQuick 2.15

Item {
    function processData(data) {
        let cleaned = cleanData(data)
        let validated = validateData(cleaned)
        return saveData(validated)
    }

    function cleanData(data) { return data }
    function validateData(data) { return data }
    function saveData(data) { return true }
}
"#;

        let (symbols, relationships) = extract_symbols_and_relationships(qml_code);

        let functions: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();
        assert_eq!(functions.len(), 4, "Should extract all four functions");

        // processData should call cleanData, validateData, and saveData
        let call_relationships: Vec<&Relationship> = relationships
            .iter()
            .filter(|r| r.kind == RelationshipKind::Calls)
            .collect();

        assert!(
            call_relationships.len() >= 3,
            "Should extract at least 3 call relationships from processData"
        );

        let process_data = functions
            .iter()
            .find(|f| f.name == "processData")
            .expect("Should find processData function");

        // Verify calls from processData
        let calls_from_process = call_relationships
            .iter()
            .filter(|r| r.from_symbol_id == process_data.id)
            .count();

        assert_eq!(
            calls_from_process, 3,
            "processData should make 3 function calls"
        );
    }

    #[test]
    fn test_extract_property_binding_relationship() {
        let qml_code = r#"
import QtQuick 2.15

Rectangle {
    id: container
    width: 200
    height: 200

    Rectangle {
        id: child
        width: parent.width / 2
        height: container.height / 2
    }
}
"#;

        let (symbols, relationships) = extract_symbols_and_relationships(qml_code);

        // Property bindings create "Uses" relationships
        let uses_relationships: Vec<&Relationship> = relationships
            .iter()
            .filter(|r| r.kind == RelationshipKind::Uses)
            .collect();

        assert!(
            uses_relationships.len() >= 1,
            "Should extract property binding relationships"
        );
    }
}
