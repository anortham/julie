// QML Bindings Tests
// Tests for property bindings and JavaScript expressions

use super::*;
use crate::base::{RelationshipKind, SymbolKind};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_simple_property_binding() {
        let qml_code = r#"
import QtQuick 2.15

Rectangle {
    id: rect
    width: 200

    Rectangle {
        id: child
        width: parent.width
        height: parent.height / 2
    }
}
"#;

        let symbols = extract_symbols(qml_code);

        // Bindings are typically part of property assignments
        let components: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Class)
            .collect();

        assert_eq!(
            components.len(),
            1,
            "Should extract only the root Rectangle component"
        );
    }

    #[test]
    fn test_extract_complex_bindings() {
        let qml_code = r#"
import QtQuick 2.15

Item {
    property int value: 100
    property real percentage: value / 100.0
    property string display: "Value: " + value + " (" + percentage + "%)"
    property bool isValid: value > 0 && value < 1000
}
"#;

        let symbols = extract_symbols(qml_code);

        let properties: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Property)
            .collect();

        assert!(
            properties.len() >= 4,
            "Should extract all properties with bindings"
        );
    }

    #[test]
    fn test_extract_conditional_bindings() {
        let qml_code = r#"
import QtQuick 2.15

Rectangle {
    property bool isLarge: width > 500
    property string size: isLarge ? "large" : "small"
    color: isLarge ? "blue" : "red"
    opacity: visible ? 1.0 : 0.0
}
"#;

        let symbols = extract_symbols(qml_code);

        let properties: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Property)
            .collect();

        assert!(
            properties.len() >= 2,
            "Should extract properties with conditional bindings"
        );
    }

    #[test]
    fn test_extract_binding_to_functions() {
        let qml_code = r#"
import QtQuick 2.15

Item {
    property int result: calculateValue()
    property string formatted: formatText(result)

    function calculateValue() {
        return 42
    }

    function formatText(value) {
        return "Result: " + value
    }
}
"#;

        let symbols = extract_symbols(qml_code);

        let properties: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Property)
            .collect();

        let functions: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();

        assert!(
            properties.len() >= 2,
            "Should extract properties bound to functions"
        );
        assert_eq!(functions.len(), 2, "Should extract both functions");
    }

    #[test]
    fn test_extract_binding_loops() {
        let qml_code = r#"
import QtQuick 2.15

Item {
    id: item1
    property int value1: item2.value2 + 1

    Item {
        id: item2
        property int value2: 10
    }
}
"#;

        let symbols = extract_symbols(qml_code);

        let properties: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Property)
            .collect();

        assert!(
            properties.len() >= 2,
            "Should extract properties with cross-references"
        );
    }

    #[test]
    fn test_extract_binding_with_javascript_expressions() {
        let qml_code = r#"
import QtQuick 2.15

Item {
    property var items: [1, 2, 3, 4, 5]
    property int sum: items.reduce((a, b) => a + b, 0)
    property var filtered: items.filter(x => x > 2)
    property var mapped: items.map(x => x * 2)
}
"#;

        let symbols = extract_symbols(qml_code);

        let properties: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Property)
            .collect();

        assert!(
            properties.len() >= 3,
            "Should extract properties with array operations"
        );
    }

    #[test]
    fn test_property_binding_relationship_targets_real_property_symbol() {
        let qml_code = r#"
import QtQuick 2.15

Item {
    id: root
    property int sourceValue: 42
    property int doubled: root.sourceValue * 2
}
"#;

        let (symbols, relationships) = extract_symbols_and_relationships(qml_code);
        let source_value = symbols
            .iter()
            .find(|symbol| symbol.name == "sourceValue" && symbol.kind == SymbolKind::Property)
            .expect("sourceValue property should be extracted");

        assert!(
            relationships.iter().any(|relationship| {
                relationship.kind == RelationshipKind::Uses
                    && relationship.to_symbol_id == source_value.id
            }),
            "root.sourceValue should target the real sourceValue property symbol, got: {:?}",
            relationships
                .iter()
                .map(|relationship| (&relationship.kind, &relationship.to_symbol_id))
                .collect::<Vec<_>>()
        );
        assert!(
            relationships
                .iter()
                .all(|relationship| !relationship.to_symbol_id.starts_with("property_")),
            "QML relationships must not target synthetic property IDs: {:?}",
            relationships
                .iter()
                .map(|relationship| &relationship.to_symbol_id)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_extract_qt_binding() {
        let qml_code = r#"
import QtQuick 2.15

Item {
    property int value: 100

    Component.onCompleted: {
        value = 200  // Breaks binding
    }

    function restoreBinding() {
        value = Qt.binding(function() { return width / 2 })
    }
}
"#;

        let symbols = extract_symbols(qml_code);

        let functions: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();

        assert!(
            functions.len() >= 1,
            "Should extract function with Qt.binding"
        );
    }

    #[test]
    fn test_extract_binding_object() {
        let qml_code = r#"
import QtQuick 2.15

Rectangle {
    width: 200

    Binding {
        target: parent
        property: "height"
        value: width * 2
    }
}
"#;

        let symbols = extract_symbols(qml_code);

        let components: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Class)
            .collect();

        assert!(
            components.len() >= 1,
            "Should extract Rectangle with Binding object"
        );
    }

    #[test]
    fn test_extract_id_binding() {
        let qml_code = r#"
import QtQuick 2.15

Rectangle {
    id: root
    width: 400
    height: 300
}
"#;

        let symbols = extract_symbols(qml_code);

        let id_sym = symbols
            .iter()
            .find(|s| s.name == "root" && s.kind == SymbolKind::Property);
        assert!(
            id_sym.is_some(),
            "id: root should be extracted as Property. Got: {:?}",
            symbols
                .iter()
                .map(|s| (&s.name, &s.kind))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_extract_multiple_id_bindings() {
        let qml_code = r#"
import QtQuick 2.15

Rectangle {
    id: root

    Text {
        id: titleText
        text: "Hello"
    }

    Item {
        id: content
        width: 100
    }
}
"#;

        let symbols = extract_symbols(qml_code);

        let id_symbols: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| {
                s.kind == SymbolKind::Property
                    && ["root", "titleText", "content"].contains(&s.name.as_str())
            })
            .collect();

        assert_eq!(
            id_symbols.len(),
            3,
            "Should extract all three id bindings. Got: {:?}",
            symbols
                .iter()
                .map(|s| (&s.name, &s.kind))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_id_binding_signature() {
        let qml_code = r#"
import QtQuick 2.15

Rectangle {
    id: myComponent
}
"#;

        let symbols = extract_symbols(qml_code);

        let id_sym = symbols
            .iter()
            .find(|s| s.name == "myComponent" && s.kind == SymbolKind::Property)
            .expect("Should find id: myComponent");

        assert_eq!(
            id_sym.signature.as_deref(),
            Some("id: myComponent"),
            "id binding should have 'id: <name>' as signature"
        );
    }
}
