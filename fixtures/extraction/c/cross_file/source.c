// Phase 4a fixture: cross-translation-unit call. `other_func` is declared in
// a sibling header (`other.h`); only the include line is visible here. The c
// extractor must emit a StructuredPendingRelationship with
// target.terminal_name="other_func". The intra-file static helper
// local_helper resolves concretely.

#include "other.h"

static int local_helper(void) {
    return 42;
}

int entry(void) {
    other_func();
    return local_helper();
}
