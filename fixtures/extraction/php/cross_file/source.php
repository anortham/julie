<?php
// Phase 4a fixture: cross-namespace class reference. Other is defined in
// another file under namespace App. The php extractor must emit a
// StructuredPendingRelationship with target.terminal_name="Other" and
// target.namespace_path=["App"]. The intra-class call to localHelper()
// resolves concretely.

namespace Fixture;

use App\Other;

class Worker {
    public function entry(): int {
        $x = new Other();
        return $this->localHelper();
    }

    private function localHelper(): int {
        return 42;
    }
}
