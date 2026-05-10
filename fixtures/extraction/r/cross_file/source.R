# Phase 4a fixture: cross-package call. `do_thing` is defined in another
# package loaded via `library(other)`. The r extractor must emit a
# StructuredPendingRelationship with target.terminal_name="do_thing"
# (ideally target.namespace_path=["other"]). The intra-file local_helper
# closure resolves concretely.

library(other)

local_helper <- function() {
  42
}

entry <- function() {
  other::do_thing()
  local_helper()
}
