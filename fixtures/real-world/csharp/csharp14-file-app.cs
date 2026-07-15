#!/usr/bin/env -S dotnet --
#:include helpers.cs
#:package Spectre.Console@0.50.0
#:project ../Shared/Shared.csproj
#:property TargetFramework=net10.0
#:sdk Microsoft.NET.Sdk

using System;
using System.Collections.Generic;
using System.Linq;

Console.WriteLine(nameof(List<>));

var customer = new FileAppCustomer();
customer?.Order = GetCurrentOrder();

TryParse<int> parse = (text, out result) => int.TryParse(text, out result);
var counter = new MutableCounter();
counter += 5;

static Order GetCurrentOrder() => new();

delegate bool TryParse<T>(string text, out T result);

public static class EnumerableExtensions
{
    extension<TSource>(IEnumerable<TSource> source)
    {
        public bool IsEmpty => !source.Any();

        public IEnumerable<TSource> Where(Func<TSource, bool> predicate) =>
            Enumerable.Where(source, predicate);
    }
}

public partial class FileAppCustomer
{
    public partial FileAppCustomer();

    public Order? Order { get; set; }

    public string Message
    {
        get;
        set => field = value ?? throw new ArgumentNullException(nameof(value));
    }

    public partial event EventHandler? Changed;
}

public partial class FileAppCustomer
{
    public partial FileAppCustomer()
    {
        Message = "ready";
    }

    public partial event EventHandler? Changed
    {
        add { }
        remove { }
    }
}

public sealed class Order { }

public sealed class MutableCounter
{
    public int Value { get; private set; }

    public void operator +=(int amount)
    {
        Value += amount;
    }
}
