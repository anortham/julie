// Phase 4a fixture: cross-package class reference. Thing is defined in
// another file under package "other". The kotlin extractor must emit a
// StructuredPendingRelationship with target.terminal_name="Thing".
// The intra-class call to localHelper() resolves concretely.

package fixture

import other.Thing

class Worker {
    fun entry(): Int {
        Thing()
        return localHelper()
    }

    fun localHelper(): Int {
        return 42
    }
}
