// QML Basics Tests
// Tests for core QML features: imports, objects, basic properties

use super::*;
use crate::base::SymbolKind;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_qml_object_and_properties() {
        let qml_code = r#"
import QtQuick 2.15

Rectangle {
    id: root
    width: 400
    height: 300

    property int customValue: 42
    property string title: "Hello QML"

    signal buttonClicked(int x, int y)

    function calculateArea() {
        return width * height
    }
}
"#;

        let symbols = extract_symbols(qml_code);

        // Should extract the Rectangle component
        let components: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Class)
            .collect();
        assert_eq!(components.len(), 1, "Should extract Rectangle component");
        // In QML, the class name is the file-derived component name, not the base type.
        // Default test helper uses "test.qml", so component name is "test".
        assert_eq!(components[0].name, "test");
        // Base type preserved in signature
        assert!(
            components[0]
                .signature
                .as_deref()
                .unwrap_or("")
                .contains("Rectangle"),
            "Signature should contain base type 'Rectangle'"
        );

        // Should extract properties
        let properties: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Property)
            .collect();
        assert!(
            properties.len() >= 2,
            "Should extract customValue and title properties"
        );

        let custom_value = properties
            .iter()
            .find(|p| p.name == "customValue")
            .expect("Should find customValue property");
        assert_eq!(custom_value.name, "customValue");

        // Should extract signals
        let signals: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Event)
            .collect();
        assert_eq!(signals.len(), 1, "Should extract buttonClicked signal");
        assert_eq!(signals[0].name, "buttonClicked");

        // Should extract functions
        let functions: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();
        assert_eq!(functions.len(), 1, "Should extract calculateArea function");
        assert_eq!(functions[0].name, "calculateArea");
    }

    #[test]
    fn test_extract_qml_nested_components() {
        let qml_code = r#"
import QtQuick 2.15

Window {
    id: mainWindow
    width: 800
    height: 600

    Rectangle {
        id: header
        width: parent.width
        height: 60

        Text {
            id: titleText
            text: "My App"
        }
    }

    ListView {
        id: listView
        model: 10
        delegate: Text { text: "Item " + index }
    }
}
"#;

        let symbols = extract_symbols(qml_code);

        // Should extract only the root component (Window)
        let components: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Class)
            .collect();

        assert_eq!(
            components.len(),
            1,
            "Should extract only the root Window component"
        );
        // File-derived name from default "test.qml"
        assert_eq!(components[0].name, "test");
        assert!(
            components[0]
                .signature
                .as_deref()
                .unwrap_or("")
                .contains("Window"),
            "Signature should contain base type 'Window'"
        );
    }

    #[test]
    fn test_extract_multiple_imports() {
        let qml_code = r#"
import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.15
import org.kde.plasma.core 2.0 as PlasmaCore

Rectangle {
    width: 100
    height: 100
}
"#;

        let symbols = extract_symbols(qml_code);

        // Should extract imports as Import-kind symbols
        let imports: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Import)
            .collect();

        assert_eq!(
            imports.len(),
            4,
            "Should extract all 4 import statements. Got: {:?}",
            imports.iter().map(|s| &s.name).collect::<Vec<_>>()
        );

        // Verify import names are the module paths
        let import_names: Vec<&str> = imports.iter().map(|s| s.name.as_str()).collect();
        assert!(
            import_names.contains(&"QtQuick"),
            "Should find QtQuick import. Got: {:?}",
            import_names
        );
        assert!(
            import_names.contains(&"QtQuick.Controls"),
            "Should find QtQuick.Controls import. Got: {:?}",
            import_names
        );
        assert!(
            import_names.contains(&"QtQuick.Layouts"),
            "Should find QtQuick.Layouts import. Got: {:?}",
            import_names
        );
        assert!(
            import_names.contains(&"org.kde.plasma.core"),
            "Should find org.kde.plasma.core import. Got: {:?}",
            import_names
        );
    }

    #[test]
    fn test_extract_property_types() {
        let qml_code = r#"
import QtQuick 2.15

Item {
    property int intValue: 42
    property real realValue: 3.14
    property string stringValue: "hello"
    property bool boolValue: true
    property color colorValue: "red"
    property url urlValue: "http://example.com"
    property var varValue: null
    property list<Item> itemList
    property QtObject objectRef
}
"#;

        let symbols = extract_symbols(qml_code);

        // Should extract all property declarations
        let properties: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Property)
            .collect();

        assert!(
            properties.len() >= 5,
            "Should extract multiple property types"
        );

        // Verify some specific properties exist
        let property_names: Vec<&str> = properties.iter().map(|p| p.name.as_str()).collect();
        assert!(property_names.contains(&"intValue"), "Should find intValue");
        assert!(
            property_names.contains(&"stringValue"),
            "Should find stringValue"
        );
    }

    #[test]
    fn test_extract_readonly_properties() {
        let qml_code = r#"
import QtQuick 2.15

Item {
    readonly property int readOnlyValue: 100
    property int normalValue: 200
}
"#;

        let symbols = extract_symbols(qml_code);

        let properties: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Property)
            .collect();

        assert_eq!(
            properties.len(),
            2,
            "Should extract both readonly and normal properties"
        );
    }

    #[test]
    fn test_extract_default_properties() {
        let qml_code = r#"
import QtQuick 2.15

Item {
    default property Component defaultComponent

    property int normalProp: 42
}
"#;

        let symbols = extract_symbols(qml_code);

        let properties: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Property)
            .collect();

        assert!(properties.len() >= 1, "Should extract default property");
    }

    #[test]
    fn test_extract_alias_properties() {
        let qml_code = r#"
import QtQuick 2.15

Rectangle {
    id: root
    property alias contentWidth: content.width
    property alias contentHeight: content.height

    Item {
        id: content
        width: 100
        height: 100
    }
}
"#;

        let symbols = extract_symbols(qml_code);

        let properties: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Property)
            .collect();

        // Should extract alias properties
        let aliases: Vec<&&Symbol> = properties
            .iter()
            .filter(|p| p.name.starts_with("content"))
            .collect();

        assert!(aliases.len() >= 2, "Should extract both alias properties");

        // Alias properties should include the full declaration text as signature
        let content_width = properties
            .iter()
            .find(|p| p.name == "contentWidth")
            .expect("Should find contentWidth alias");

        assert!(
            content_width
                .signature
                .as_deref()
                .unwrap_or("")
                .contains("alias"),
            "Alias property signature should contain 'alias'. Got: {:?}",
            content_width.signature
        );
    }
}
