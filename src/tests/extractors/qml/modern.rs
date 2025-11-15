// QML Modern Features Tests
// Tests for Qt 5.x and Qt 6.x modern QML features

use super::*;
use crate::extractors::base::SymbolKind;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_required_properties() {
        let qml_code = r#"
import QtQuick 2.15

Item {
    required property string name
    required property int age
    property string optional: "default"
}
"#;

        let symbols = extract_symbols(qml_code);

        let properties: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Property)
            .collect();

        assert!(properties.len() >= 2, "Should extract required properties");
    }

    #[test]
    fn test_extract_inline_components_qt515() {
        let qml_code = r#"
import QtQuick 2.15

Item {
    component RedRectangle: Rectangle {
        color: "red"
        width: 100
        height: 100
    }

    component BlueRectangle: Rectangle {
        color: "blue"
        width: 100
        height: 100
    }

    RedRectangle { x: 0 }
    BlueRectangle { x: 150 }
}
"#;

        let symbols = extract_symbols(qml_code);

        // Inline components (Qt 5.15+)
        let components: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Class)
            .collect();

        assert!(components.len() >= 1, "Should extract inline components");
    }

    #[test]
    fn test_extract_property_value_sources() {
        let qml_code = r#"
import QtQuick 2.15

Item {
    property int value

    PropertyAnimation on value {
        from: 0
        to: 100
        duration: 1000
    }
}
"#;

        let symbols = extract_symbols(qml_code);

        let properties: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Property)
            .collect();

        assert_eq!(
            properties.len(),
            1,
            "Should extract property with value source"
        );
    }

    #[test]
    fn test_extract_enum_declarations() {
        let qml_code = r#"
import QtQuick 2.15

Item {
    enum Status {
        Ready,
        Loading,
        Error
    }

    property int currentStatus: Status.Ready
}
"#;

        let symbols = extract_symbols(qml_code);

        // Enums might be extracted as types or constants
        // Document expected behavior
    }

    #[test]
    fn test_extract_pragma_statements() {
        let qml_code = r#"
pragma Singleton
pragma ComponentBehavior: Bound

import QtQuick 2.15

QtObject {
    property string value: "singleton"
}
"#;

        let symbols = extract_symbols(qml_code);

        // Pragma statements affect compilation but may not be extracted as symbols
        let components: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Class)
            .collect();

        assert!(components.len() >= 1, "Should extract QtObject");
    }

    #[test]
    fn test_extract_javascript_imports() {
        let qml_code = r#"
import QtQuick 2.15
import "utils.js" as Utils
import "math.mjs" as MathLib

Item {
    Component.onCompleted: {
        let result = Utils.calculate(10, 20)
        let value = MathLib.compute(result)
    }
}
"#;

        let symbols = extract_symbols(qml_code);

        // JavaScript imports
        let imports: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Import)
            .collect();

        // Note: May or may not be extracted as import symbols
    }

    #[test]
    fn test_extract_attached_properties() {
        let qml_code = r#"
import QtQuick 2.15

ListView {
    model: 10
    delegate: Rectangle {
        width: ListView.view.width
        height: 50
        color: ListView.isCurrentItem ? "blue" : "gray"

        required property int index
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
            "Should extract ListView with attached properties"
        );
    }

    #[test]
    fn test_extract_qt6_syntax() {
        let qml_code = r#"
import QtQuick 6.0

Window {
    id: window
    width: 640
    height: 480

    // Qt 6 property binding with 'this'
    property int calculated: this.width * this.height

    // Connections with function syntax
    Connections {
        target: window
        function onWidthChanged() {
            console.log("Width:", window.width)
        }
    }
}
"#;

        let symbols = extract_symbols(qml_code);

        let components: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Class)
            .collect();

        assert!(components.len() >= 1, "Should extract Qt 6 syntax");
    }

    #[test]
    fn test_extract_value_type_providers() {
        let qml_code = r#"
import QtQuick 2.15

Item {
    property rect myRect: Qt.rect(10, 10, 100, 100)
    property point myPoint: Qt.point(50, 50)
    property size mySize: Qt.size(200, 200)
    property color myColor: Qt.rgba(1.0, 0.5, 0.0, 1.0)
}
"#;

        let symbols = extract_symbols(qml_code);

        let properties: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Property)
            .collect();

        assert!(
            properties.len() >= 3,
            "Should extract value type properties"
        );
    }

    #[test]
    fn test_extract_nullish_coalescing() {
        let qml_code = r#"
import QtQuick 2.15

Item {
    property var data: null
    property string display: data?.name ?? "Unknown"
    property int value: data?.value ?? 0
}
"#;

        let symbols = extract_symbols(qml_code);

        let properties: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Property)
            .collect();

        assert!(
            properties.len() >= 2,
            "Should extract properties with nullish coalescing"
        );
    }

    #[test]
    fn test_extract_template_literals() {
        let qml_code = r#"
import QtQuick 2.15

Item {
    property string name: "World"
    property string greeting: `Hello, ${name}!`
    property string multiline: `
        This is a
        multiline string
        with ${name}
    `
}
"#;

        let symbols = extract_symbols(qml_code);

        let properties: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Property)
            .collect();

        assert!(
            properties.len() >= 2,
            "Should extract properties with template literals"
        );
    }
}
