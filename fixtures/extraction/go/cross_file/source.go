// Phase 4a fixture: cross-package call. DoIt is defined in another file under
// package "example/other". The go extractor must emit a
// StructuredPendingRelationship with target.terminal_name="DoIt".
// The intra-file local_helper() call resolves concretely.

package main

import "example/other"

func local_helper() int {
	return 42
}

func entry() int {
	other.DoIt()
	return local_helper()
}
