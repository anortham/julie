// Phase 4a fixture: cross-module call. `func` is defined in `other.zig`
// imported via `@import("other.zig")`. The zig extractor must emit a
// StructuredPendingRelationship with target.terminal_name="func".
// The intra-file local_helper() call resolves concretely.

const m = @import("other.zig");

fn local_helper() i32 {
    return 42;
}

pub fn entry() i32 {
    _ = m.func();
    return local_helper();
}
