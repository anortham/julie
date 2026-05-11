// Phase 4a fixture: cross-namespace class reference. OtherClass is defined
// in another file under namespace OtherNs. The csharp extractor must emit a
// StructuredPendingRelationship with target.terminal_name="OtherClass" and
// target.import_context="OtherNs".
// The intra-class call to LocalHelper() resolves concretely.

using OtherNs;

namespace Fixture;

public class Source
{
    public int Entry()
    {
        var x = new OtherClass();
        return LocalHelper();
    }

    private int LocalHelper()
    {
        return 42;
    }
}
