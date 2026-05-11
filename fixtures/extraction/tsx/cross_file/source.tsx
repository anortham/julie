// Phase 4a fixture: cross-file class import with JSX. Foo is defined in
// another file at './other'. The tsx extractor (TypeScriptExtractor) must
// emit a StructuredPendingRelationship with target.terminal_name="Foo" and
// target.import_context="./other". The intra-file local_helper() resolves.

import { Foo } from './other';

function local_helper(): number {
    return 42;
}

export function entry() {
    const x = new Foo();
    return <Foo>{local_helper()}</Foo>;
}
