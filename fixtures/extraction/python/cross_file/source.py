# Phase 4a fixture: cross-module call. `bar` is defined in another file under
# module "other". The python extractor must emit a StructuredPendingRelationship
# with target.terminal_name="bar" and target.import_context="other".
# The intra-file local_helper() call resolves concretely.

from other import bar


def local_helper():
    return 42


def entry():
    bar()
    return local_helper()
