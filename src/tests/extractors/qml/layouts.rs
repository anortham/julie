// QML Layouts Tests
// Tests for anchors, layouts, and positioning

use super::*;
use crate::extractors::base::SymbolKind;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_anchor_layouts() {
        let qml_code = r#"
import QtQuick 2.15

Rectangle {
    id: container
    width: 400
    height: 300

    Rectangle {
        id: topBar
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        height: 50
    }

    Rectangle {
        id: content
        anchors.top: topBar.bottom
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.right: parent.right
    }
}
"#;

        let symbols = extract_symbols(qml_code);

        let components: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Class)
            .collect();

        assert_eq!(
            components.len(),
            3,
            "Should extract container and nested rectangles"
        );
    }

    #[test]
    fn test_extract_anchor_fill() {
        let qml_code = r#"
import QtQuick 2.15

Item {
    Rectangle {
        anchors.fill: parent
        anchors.margins: 10
    }

    Rectangle {
        anchors.centerIn: parent
        width: 100
        height: 100
    }
}
"#;

        let symbols = extract_symbols(qml_code);

        let components: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Class)
            .collect();

        assert!(
            components.len() >= 2,
            "Should extract Item with nested rectangles"
        );
    }

    #[test]
    fn test_extract_row_layout() {
        let qml_code = r#"
import QtQuick 2.15
import QtQuick.Layouts 1.15

RowLayout {
    spacing: 10

    Rectangle {
        width: 100
        height: 50
        color: "red"
    }

    Rectangle {
        width: 100
        height: 50
        color: "blue"
    }

    Rectangle {
        Layout.fillWidth: true
        height: 50
        color: "green"
    }
}
"#;

        let symbols = extract_symbols(qml_code);

        let components: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Class)
            .collect();

        assert!(
            components.len() >= 3,
            "Should extract RowLayout with rectangles"
        );
    }

    #[test]
    fn test_extract_column_layout() {
        let qml_code = r#"
import QtQuick 2.15
import QtQuick.Layouts 1.15

ColumnLayout {
    spacing: 5

    Text { text: "Header" }
    Text { text: "Content" }
    Text { text: "Footer" }
}
"#;

        let symbols = extract_symbols(qml_code);

        let components: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Class)
            .collect();

        assert!(
            components.len() >= 1,
            "Should extract ColumnLayout with text elements"
        );
    }

    #[test]
    fn test_extract_grid_layout() {
        let qml_code = r#"
import QtQuick 2.15
import QtQuick.Layouts 1.15

GridLayout {
    columns: 3
    rows: 2
    rowSpacing: 10
    columnSpacing: 10

    Rectangle { Layout.row: 0; Layout.column: 0; width: 50; height: 50 }
    Rectangle { Layout.row: 0; Layout.column: 1; width: 50; height: 50 }
    Rectangle { Layout.row: 0; Layout.column: 2; width: 50; height: 50 }
    Rectangle { Layout.row: 1; Layout.column: 0; width: 50; height: 50 }
    Rectangle { Layout.row: 1; Layout.column: 1; width: 50; height: 50 }
    Rectangle { Layout.row: 1; Layout.column: 2; width: 50; height: 50 }
}
"#;

        let symbols = extract_symbols(qml_code);

        let components: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Class)
            .collect();

        assert!(
            components.len() >= 1,
            "Should extract GridLayout with items"
        );
    }

    #[test]
    fn test_extract_stack_layout() {
        let qml_code = r#"
import QtQuick 2.15
import QtQuick.Layouts 1.15

StackLayout {
    id: stackLayout
    currentIndex: 0

    Rectangle {
        color: "red"
    }

    Rectangle {
        color: "blue"
    }

    Rectangle {
        color: "green"
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
            "Should extract StackLayout with pages"
        );
    }

    #[test]
    fn test_extract_flow_layout() {
        let qml_code = r#"
import QtQuick 2.15

Flow {
    spacing: 10
    width: 300

    Repeater {
        model: 20
        Rectangle {
            width: 50
            height: 50
            color: Qt.rgba(Math.random(), Math.random(), Math.random(), 1)
        }
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
            "Should extract Flow layout with repeater"
        );
    }

    #[test]
    fn test_extract_positioner_layouts() {
        let qml_code = r#"
import QtQuick 2.15

Column {
    spacing: 10

    Row {
        spacing: 5
        Rectangle { width: 50; height: 50 }
        Rectangle { width: 50; height: 50 }
    }

    Grid {
        columns: 3
        spacing: 5
        Repeater {
            model: 9
            Rectangle { width: 30; height: 30 }
        }
    }
}
"#;

        let symbols = extract_symbols(qml_code);

        let components: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Class)
            .collect();

        assert!(
            components.len() >= 3,
            "Should extract Column, Row, and Grid"
        );
    }
}
