# Phase 4a fixture: cross-script reference. The base class is in another file
# referenced via `extends "res://other.gd"`. The gdscript extractor must emit
# a StructuredPendingRelationship with target.terminal_name="other_method".
# The intra-class call to local_helper() resolves concretely.

extends "res://other.gd"

func local_helper() -> int:
    return 42

func entry() -> int:
    other_method()
    return local_helper()
