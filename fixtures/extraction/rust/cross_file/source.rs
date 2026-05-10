// Phase 4a fixture: cross-module call to `other_module::Function` whose
// definition lives in a different file. The rust extractor must emit a
// StructuredPendingRelationship with target.terminal_name="Function" and
// target.namespace_path=["crate","other_module"]. The local helper call
// (`local_helper()`) resolves concretely.

use crate::other_module::Function;

fn local_helper() -> i32 {
    42
}

pub fn entry() -> i32 {
    let _ = Function();
    local_helper()
}
