// Phase 4a fixture: cross-package object call. `Thing` is defined in another
// file under package `other`. The scala extractor must emit a
// StructuredPendingRelationship with target.terminal_name="Thing".
// The intra-class call to localHelper() resolves concretely.

package fixture

import other.Thing

class Worker {
  def entry(): Int = {
    Thing.apply()
    localHelper()
  }

  def localHelper(): Int = 42
}
