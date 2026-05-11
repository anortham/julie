// Phase 4b fixture: QML cross-file call. `external_helper` lives in
// another QML module loaded via `import "OtherModule"`. The qml
// extractor must emit a StructuredPendingRelationship with
// target.terminal_name="external_helper". The intra-file
// `local_helper` resolves concretely.

import QtQuick 2.15
import "OtherModule"

Item {
    id: root

    function local_helper() {
        return 42
    }

    function entry() {
        external_helper()
        return local_helper()
    }
}
