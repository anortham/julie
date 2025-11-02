// QML Components Tests
// Tests for custom components, loaders, repeaters, and delegates

use super::*;
use crate::extractors::base::SymbolKind;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_custom_component_definition() {
        let qml_code = r#"
import QtQuick 2.15

Item {
    Component {
        id: customButton
        Rectangle {
            width: 100
            height: 40
            color: "blue"

            Text {
                anchors.centerIn: parent
                text: "Click Me"
            }
        }
    }
}
"#;

        let symbols = extract_symbols(qml_code);

        let components: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Class)
            .collect();

        assert!(components.len() >= 2, "Should extract Item, Component, and nested components");
    }

    #[test]
    fn test_extract_loader_component() {
        let qml_code = r#"
import QtQuick 2.15

Item {
    Loader {
        id: dynamicLoader
        source: "CustomComponent.qml"
        asynchronous: true
        onLoaded: {
            item.initialize()
        }
    }

    Loader {
        id: inlineLoader
        sourceComponent: Rectangle {
            width: 100
            height: 100
        }
    }
}
"#;

        let symbols = extract_symbols(qml_code);

        let components: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Class)
            .collect();

        assert!(components.len() >= 2, "Should extract Item with Loader components");
    }

    #[test]
    fn test_extract_repeater_component() {
        let qml_code = r#"
import QtQuick 2.15

Column {
    Repeater {
        model: 10
        delegate: Rectangle {
            width: 100
            height: 30
            color: index % 2 === 0 ? "lightblue" : "lightgray"

            Text {
                text: "Item " + index
            }
        }
    }
}
"#;

        let symbols = extract_symbols(qml_code);

        let components: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Class)
            .collect();

        assert!(components.len() >= 2, "Should extract Column, Repeater, and delegate");
    }

    #[test]
    fn test_extract_listview_with_delegate() {
        let qml_code = r#"
import QtQuick 2.15

ListView {
    id: listView
    model: myModel

    delegate: Item {
        width: listView.width
        height: 50

        Row {
            Text { text: model.name }
            Text { text: model.value }
        }
    }

    header: Rectangle {
        width: parent.width
        height: 40
        color: "lightgray"
    }

    footer: Rectangle {
        width: parent.width
        height: 30
        color: "darkgray"
    }
}
"#;

        let symbols = extract_symbols(qml_code);

        let components: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Class)
            .collect();

        assert!(components.len() >= 3, "Should extract ListView, delegate, header, and footer");
    }

    #[test]
    fn test_extract_gridview_component() {
        let qml_code = r#"
import QtQuick 2.15

GridView {
    cellWidth: 100
    cellHeight: 100
    model: 20

    delegate: Rectangle {
        width: GridView.view.cellWidth
        height: GridView.view.cellHeight
        color: Qt.rgba(Math.random(), Math.random(), Math.random(), 1)
    }
}
"#;

        let symbols = extract_symbols(qml_code);

        let components: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Class)
            .collect();

        assert!(components.len() >= 1, "Should extract GridView with delegate");
    }

    #[test]
    fn test_extract_inline_component() {
        let qml_code = r#"
import QtQuick 2.15

Item {
    component CustomButton: Rectangle {
        width: 100
        height: 40
        radius: 5

        signal clicked()

        property alias text: label.text

        Text {
            id: label
            anchors.centerIn: parent
        }
    }

    CustomButton {
        text: "Click Me"
    }
}
"#;

        let symbols = extract_symbols(qml_code);

        // Inline components (Qt 5.15+) might have different extraction behavior
        let components: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Class)
            .collect();

        assert!(components.len() >= 1, "Should extract inline component");
    }

    #[test]
    fn test_extract_instantiator_component() {
        let qml_code = r#"
import QtQuick 2.15

Item {
    Instantiator {
        model: 5
        delegate: Rectangle {
            width: 100
            height: 100
        }
        onObjectAdded: parent.children.push(object)
    }
}
"#;

        let symbols = extract_symbols(qml_code);

        let components: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Class)
            .collect();

        assert!(components.len() >= 1, "Should extract Instantiator with delegate");
    }

    #[test]
    fn test_extract_pathview_component() {
        let qml_code = r#"
import QtQuick 2.15

PathView {
    model: 10
    delegate: Rectangle {
        width: 80
        height: 80
        color: "lightblue"
        scale: PathView.iconScale
        z: PathView.z
    }

    path: Path {
        startX: 0
        startY: height / 2

        PathQuad {
            x: width
            y: height / 2
            controlX: width / 2
            controlY: 0
        }
    }
}
"#;

        let symbols = extract_symbols(qml_code);

        let components: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Class)
            .collect();

        assert!(components.len() >= 2, "Should extract PathView with delegate and path");
    }
}
