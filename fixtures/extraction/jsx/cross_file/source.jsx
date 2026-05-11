// Phase 4a fixture: cross-file ESM import + JSX. Foo is defined in './other'.
// The jsx extractor (JavaScriptExtractor) must emit a
// StructuredPendingRelationship with target.terminal_name="Foo".
// The intra-file local_helper() resolves concretely.

import { Foo } from './other';

function local_helper() {
    return 42;
}

export function entry() {
    const x = new Foo();
    return <Foo>{local_helper()}</Foo>;
}
