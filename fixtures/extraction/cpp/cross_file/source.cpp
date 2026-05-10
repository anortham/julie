// Phase 4a fixture: cross-namespace call. `other_ns::do_thing` is declared in
// a sibling header. The cpp extractor must emit a StructuredPendingRelationship
// with target.terminal_name="do_thing" and target.namespace_path=["other_ns"].
// The intra-file local_helper() call resolves concretely.

#include "other.h"

static int local_helper() {
    return 42;
}

int entry() {
    other_ns::do_thing();
    return local_helper();
}
