// Phase 4a fixture: cross-file import. Other is defined in `other.dart`.
// The dart extractor must emit a StructuredPendingRelationship with
// target.terminal_name="Other" and target.import_context="other.dart".
// The intra-file local_helper() call resolves concretely.

import 'other.dart';

int local_helper() {
  return 42;
}

int entry() {
  var x = Other();
  return local_helper();
}
