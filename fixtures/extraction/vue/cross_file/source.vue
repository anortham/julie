<script setup>
// Phase 4a fixture: cross-file `script setup` import. `foo` is defined in
// './other'. The vue extractor must emit a StructuredPendingRelationship
// with target.terminal_name="foo" and target.import_context="./other".
// The intra-file local_helper() resolves concretely.

import { foo } from './other';

function local_helper() {
    return 42;
}

function entry() {
    foo();
    return local_helper();
}
</script>

<template>
    <div>{{ entry() }}</div>
</template>
