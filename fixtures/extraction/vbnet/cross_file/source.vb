' Phase 4a fixture: cross-namespace reference. OtherClass is defined in
' another file under namespace OtherNs. The vbnet extractor must emit a
' StructuredPendingRelationship with target.terminal_name="OtherClass".
' The intra-class call (Helper) resolves concretely.

Imports OtherNs

Namespace Fixture
    Public Class Worker
        Public Function Run() As Integer
            Dim x As New OtherClass()
            Return Helper(x.Value)
        End Function

        Private Function Helper(value As Integer) As Integer
            Return value + 1
        End Function
    End Class
End Namespace
