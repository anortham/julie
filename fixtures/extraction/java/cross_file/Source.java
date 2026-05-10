// Phase 4a fixture: cross-package class reference. Other is defined in
// another file under package "com.example". The java extractor must emit
// a StructuredPendingRelationship with target.terminal_name="Other" and
// target.namespace_path=["com","example"].
// The intra-class call to localHelper() resolves concretely.

package fixture;

import com.example.Other;

public class Source {
    public int entry() {
        Other o = new Other();
        return localHelper();
    }

    private int localHelper() {
        return 42;
    }
}
