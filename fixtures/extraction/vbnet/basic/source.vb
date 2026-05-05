Namespace Fixture
    Public Interface IJob
        Function Run() As Integer
    End Interface

    Public Class Worker
        Implements IJob

        Public Property Id As Integer

        Public Function Run() As Integer Implements IJob.Run
            Return Helper(Id)
        End Function

        Private Function Helper(value As Integer) As Integer
            Return value + 1
        End Function
    End Class
End Namespace
