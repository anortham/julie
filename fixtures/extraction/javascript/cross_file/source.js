// Phase 4a fixture: cross-file ESM import. `foo` is defined in './other'.
// The javascript extractor must emit a StructuredPendingRelationship with
// target.terminal_name="foo" (carrying import_context="./other" when the
// extractor wires through the import binding). The intra-file local_helper()
// resolves concretely.

import { foo } from './other';

function local_helper() {
    return 42;
}

export function entry() {
    foo();
    return local_helper();
}
