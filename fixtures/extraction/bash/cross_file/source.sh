#!/usr/bin/env bash
# Phase 4a fixture: cross-script call. `other_fn` is defined in another file
# sourced via `source ./other.sh`. The bash extractor must emit a
# StructuredPendingRelationship with target.terminal_name="other_fn".
# The intra-script call to local_helper resolves concretely.

source ./other.sh

local_helper() {
    echo 42
}

entry() {
    other_fn "$@"
    local_helper
}
