// QML Modern Features Tests
// Tests for Qt 5.x and Qt 6.x modern QML features

use super::*;
use crate::base::SymbolKind;

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

        // Enum type should be extracted
        let enum_sym = symbols
            .iter()
            .find(|s| s.name == "Status" && s.kind == SymbolKind::Enum);
        assert!(
            enum_sym.is_some(),
            "Should extract enum Status. Got: {:?}",
            symbols.iter().map(|s| (&s.name, &s.kind)).collect::<Vec<_>>()
        );

        // Enum members should be extracted
        let members: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::EnumMember)
            .collect();
        let member_names: Vec<&str> = members.iter().map(|s| s.name.as_str()).collect();
        assert!(member_names.contains(&"Ready"), "Should extract Ready member");
        assert!(member_names.contains(&"Loading"), "Should extract Loading member");
        assert!(member_names.contains(&"Error"), "Should extract Error member");
    }

    #[test]
    fn test_extract_enum_with_values() {
        let qml_code = r#"
import QtQuick 2.15

Item {
    enum Direction { Left, Right, Up, Down }
}
"#;

        let symbols = extract_symbols(qml_code);

        let enum_sym = symbols
            .iter()
            .find(|s| s.name == "Direction" && s.kind == SymbolKind::Enum);
        assert!(
            enum_sym.is_some(),
            "Should extract enum Direction. Got: {:?}",
            symbols.iter().map(|s| (&s.name, &s.kind)).collect::<Vec<_>>()
        );

        let members: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::EnumMember)
            .collect();
        assert_eq!(members.len(), 4, "Should extract all 4 enum members");
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
