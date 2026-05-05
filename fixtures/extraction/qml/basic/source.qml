import QtQuick 2.15

Item {
    id: root
    property string title: "Worker"
    signal activated(string value)

    function format(value) {
        return value.trim()
    }

    Text {
        text: root.format(root.title)
    }
}
