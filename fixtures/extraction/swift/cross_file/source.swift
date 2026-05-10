// Phase 4a fixture: cross-module call. `Other.thing` is defined in another
// module imported via `import Other`. The swift extractor must emit a
// StructuredPendingRelationship with target.terminal_name="thing".
// The intra-class call to self.localHelper() resolves concretely.

import Other

class Worker {
    func entry() -> Int {
        Other.thing()
        return localHelper()
    }

    func localHelper() -> Int {
        return 42
    }
}
