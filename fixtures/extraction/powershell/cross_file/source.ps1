# Phase 4a fixture: cross-module call. `Invoke-Other` is defined in module
# Other (imported via Import-Module). The powershell extractor must emit a
# StructuredPendingRelationship with target.terminal_name="Invoke-Other".
# The intra-script call to Local-Helper resolves concretely.

Import-Module Other

function Local-Helper {
    return 42
}

function Entry {
    Invoke-Other -arg "x"
    Local-Helper
}
