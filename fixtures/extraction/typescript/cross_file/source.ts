// Phase 4a fixture: cross-file class import. Foo is defined in another file
// at './other'. The typescript extractor must emit a
// StructuredPendingRelationship with target.terminal_name="Foo" and
// target.import_context="./other". The intra-file local_helper() resolves.

import { Foo } from './other';

function local_helper(): number {
    return 42;
}

export function entry(): number {
    const x = new Foo();
    return local_helper();
}
