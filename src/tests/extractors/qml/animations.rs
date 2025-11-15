// QML Animations Tests
// Tests for states, transitions, and animations

use super::*;
use crate::extractors::base::SymbolKind;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_number_animation() {
        let qml_code = r#"
import QtQuick 2.15

Rectangle {
    id: rect
    width: 100
    height: 100

    NumberAnimation on x {
        from: 0
        to: 200
        duration: 1000
        easing.type: Easing.InOutQuad
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
            "Should extract Rectangle with NumberAnimation"
        );
    }

    #[test]
    fn test_extract_property_animation() {
        let qml_code = r#"
import QtQuick 2.15

Item {
    PropertyAnimation {
        id: fadeIn
        target: myItem
        property: "opacity"
        from: 0
        to: 1
        duration: 500
    }

    PropertyAnimation {
        id: fadeOut
        target: myItem
        property: "opacity"
        from: 1
        to: 0
        duration: 500
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
            "Should extract Item with PropertyAnimations"
        );
    }

    #[test]
    fn test_extract_states() {
        let qml_code = r#"
import QtQuick 2.15

Rectangle {
    id: button
    width: 100
    height: 40

    states: [
        State {
            name: "pressed"
            PropertyChanges {
                target: button
                color: "blue"
                scale: 0.9
            }
        },
        State {
            name: "hovered"
            PropertyChanges {
                target: button
                color: "lightblue"
            }
        }
    ]
}
"#;

        let symbols = extract_symbols(qml_code);

        let components: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Class)
            .collect();

        assert!(
            components.len() >= 1,
            "Should extract Rectangle with states"
        );
    }

    #[test]
    fn test_extract_transitions() {
        let qml_code = r#"
import QtQuick 2.15

Rectangle {
    width: 100
    height: 100

    state: "normal"

    states: [
        State {
            name: "normal"
            PropertyChanges { target: rect; color: "red" }
        },
        State {
            name: "pressed"
            PropertyChanges { target: rect; color: "blue" }
        }
    ]

    transitions: [
        Transition {
            from: "normal"
            to: "pressed"
            ColorAnimation { duration: 200 }
        },
        Transition {
            from: "pressed"
            to: "normal"
            ColorAnimation { duration: 200 }
        }
    ]
}
"#;

        let symbols = extract_symbols(qml_code);

        let components: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Class)
            .collect();

        assert!(
            components.len() >= 1,
            "Should extract Rectangle with transitions"
        );
    }

    #[test]
    fn test_extract_sequential_animation() {
        let qml_code = r#"
import QtQuick 2.15

Item {
    SequentialAnimation {
        NumberAnimation { target: box; property: "x"; to: 200; duration: 500 }
        NumberAnimation { target: box; property: "y"; to: 200; duration: 500 }
        NumberAnimation { target: box; property: "x"; to: 0; duration: 500 }
        NumberAnimation { target: box; property: "y"; to: 0; duration: 500 }
        loops: Animation.Infinite
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
            "Should extract Item with SequentialAnimation"
        );
    }

    #[test]
    fn test_extract_parallel_animation() {
        let qml_code = r#"
import QtQuick 2.15

Item {
    ParallelAnimation {
        id: moveAndFade
        NumberAnimation { target: box; property: "x"; to: 200; duration: 1000 }
        NumberAnimation { target: box; property: "opacity"; to: 0; duration: 1000 }
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
            "Should extract Item with ParallelAnimation"
        );
    }

    #[test]
    fn test_extract_behavior_animation() {
        let qml_code = r#"
import QtQuick 2.15

Rectangle {
    width: 100
    height: 100

    Behavior on x {
        NumberAnimation { duration: 200; easing.type: Easing.InOutQuad }
    }

    Behavior on opacity {
        NumberAnimation { duration: 300 }
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
            "Should extract Rectangle with Behaviors"
        );
    }

    #[test]
    fn test_extract_spring_animation() {
        let qml_code = r#"
import QtQuick 2.15

Item {
    Rectangle {
        id: box
        width: 50
        height: 50

        Behavior on y {
            SpringAnimation {
                spring: 2
                damping: 0.2
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

        assert!(
            components.len() >= 2,
            "Should extract Item and Rectangle with SpringAnimation"
        );
    }

    #[test]
    fn test_extract_path_animation() {
        let qml_code = r#"
import QtQuick 2.15

Item {
    PathAnimation {
        target: box
        duration: 2000

        path: Path {
            startX: 0
            startY: 0

            PathCubic {
                x: 200
                y: 200
                control1X: 100
                control1Y: 0
                control2X: 200
                control2Y: 100
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

        assert!(
            components.len() >= 1,
            "Should extract Item with PathAnimation"
        );
    }
}
