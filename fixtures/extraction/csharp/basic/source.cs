namespace Fixture;

public interface IJob
{
    int Run();
}

public sealed class Worker : IJob
{
    public Worker(int id)
    {
        Id = id;
    }

    public int Id { get; }

    public int Run()
    {
        return Helper(Id);
    }

    private static int Helper(int value)
    {
        return value + 1;
    }
}
