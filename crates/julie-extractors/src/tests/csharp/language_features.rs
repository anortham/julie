use super::*;
use std::path::PathBuf;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relationship_extraction() {
        let code = r#"
namespace MyProject
{
    public interface IEntity
    {
        int Id { get; }
    }

    public abstract class BaseEntity : IEntity
    {
        public int Id { get; set; }
    }

    public class User : BaseEntity, IEquatable<User>
    {
        public string Name { get; set; }
    }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = CSharpExtractor::new(
            "c_sharp".to_string(),
            "test.cs".to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);
        let relationships = extractor.extract_relationships(&tree, &symbols);

        assert!(relationships.len() >= 1);

        let inheritance = relationships.iter().find(|r| {
            r.kind.to_string() == "extends"
                && symbols
                    .iter()
                    .find(|s| s.id == r.from_symbol_id)
                    .unwrap()
                    .name
                    == "User"
                && symbols
                    .iter()
                    .find(|s| s.id == r.to_symbol_id)
                    .unwrap()
                    .name
                    == "BaseEntity"
        });
        assert!(inheritance.is_some());
    }

    #[test]
    fn test_modern_csharp_async_await_patterns() {
        let code = r#"
#nullable enable
namespace ModernFeatures
{
    public class AsyncService
    {
        public async Task<string?> GetDataAsync(CancellationToken cancellationToken = default)
        {
            await Task.Delay(1000, cancellationToken);
            return await ProcessDataAsync();
        }

        public async ValueTask<int> CountItemsAsync()
        {
            await foreach (var item in GetItemsAsync())
            {
            }
            return 42;
        }

        public async IAsyncEnumerable<string> GetItemsAsync([EnumeratorCancellation] CancellationToken cancellationToken = default)
        {
            for (int i = 0; i < 10; i++)
            {
                await Task.Delay(100, cancellationToken);
                yield return $"Item {i}";
            }
        }

        private async Task<string?> ProcessDataAsync() => await Task.FromResult("data");
    }

    public class NullableExample
    {
        public string? NullableString { get; init; }
        public required string RequiredString { get; init; }
        public string NonNullableString { get; init; } = string.Empty;
    }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = CSharpExtractor::new(
            "c_sharp".to_string(),
            "test.cs".to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);

        let async_service = symbols.iter().find(|s| s.name == "AsyncService");
        assert!(async_service.is_some());

        let get_data_async = symbols.iter().find(|s| s.name == "GetDataAsync");
        assert!(get_data_async.is_some());
        assert!(
            get_data_async
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("async Task<string?>")
        );

        let count_items_async = symbols.iter().find(|s| s.name == "CountItemsAsync");
        assert!(count_items_async.is_some());
        assert!(
            count_items_async
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("ValueTask<int>")
        );

        let get_items_async = symbols.iter().find(|s| s.name == "GetItemsAsync");
        assert!(get_items_async.is_some());
        assert!(
            get_items_async
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("IAsyncEnumerable<string>")
        );

        let nullable_example = symbols.iter().find(|s| s.name == "NullableExample");
        assert!(nullable_example.is_some());
    }

    #[test]
    fn test_modern_csharp_pattern_matching() {
        let code = r#"
namespace PatternMatching
{
    public abstract record Shape;
    public record Circle(double Radius) : Shape;
    public record Rectangle(double Width, double Height) : Shape;
    public record Triangle(double Base, double Height) : Shape;

    public class ShapeProcessor
    {
        public double CalculateArea(Shape shape) => shape switch
        {
            Circle { Radius: var r } => Math.PI * r * r,
            Rectangle { Width: var w, Height: var h } => w * h,
            Triangle { Base: var b, Height: var h } => 0.5 * b * h,
            _ => throw new ArgumentException("Unknown shape")
        };

        public string DescribeShape(Shape shape)
        {
            return shape switch
            {
                Circle c when c.Radius > 10 => "Large circle",
                Circle => "Small circle",
                Rectangle r when r.Width == r.Height => "Square",
                Rectangle => "Rectangle",
                Triangle => "Triangle",
                null => "No shape",
                _ => "Unknown"
            };
        }

        public bool IsLargeShape(Shape shape) => shape is Circle { Radius: > 5 } or Rectangle { Width: > 10, Height: > 10 };
    }

    public class PatternExamples
    {
        public void ProcessValue(object value)
        {
            if (value is string { Length: > 0 } str)
            {
                Console.WriteLine($"Non-empty string: {str}");
            }
            else if (value is int i and > 0)
            {
                Console.WriteLine($"Positive integer: {i}");
            }
        }
    }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = CSharpExtractor::new(
            "c_sharp".to_string(),
            "test.cs".to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);

        let shape = symbols.iter().find(|s| s.name == "Shape");
        assert!(shape.is_some());
        assert!(
            shape
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("abstract record")
        );

        let circle = symbols.iter().find(|s| s.name == "Circle");
        assert!(circle.is_some());
        assert!(
            circle
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("record Circle(double Radius)")
        );

        let calculate_area = symbols.iter().find(|s| s.name == "CalculateArea");
        assert!(calculate_area.is_some());
        assert!(
            calculate_area
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("=> shape switch")
        );

        let describe_shape = symbols.iter().find(|s| s.name == "DescribeShape");
        assert!(describe_shape.is_some());
        assert_eq!(describe_shape.unwrap().kind, SymbolKind::Method);

        let is_large_shape = symbols.iter().find(|s| s.name == "IsLargeShape");
        assert!(is_large_shape.is_some());
        assert!(
            is_large_shape
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("is Circle")
        );

        let process_value = symbols.iter().find(|s| s.name == "ProcessValue");
        assert!(process_value.is_some());
        assert!(
            process_value
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("object value")
        );
    }

    #[test]
    fn test_advanced_generic_and_type_features() {
        let code = r#"
namespace AdvancedGenerics
{
    public interface ICovariant<out T>
    {
        T GetValue();
    }

    public interface IContravariant<in T>
    {
        void SetValue(T value);
    }

    public interface IRepository<T> where T : class, IEntity, new()
    {
        Task<T> GetByIdAsync<TKey>(TKey id) where TKey : struct, IComparable<TKey>;
    }

    public class GenericService<T, U, V> where T : class, IDisposable where U : struct where V : T, new()
    {
        public async Task<TResult> ProcessAsync<TResult, TInput>(TInput input, Func<TInput, Task<TResult>> processor)
            where TResult : class
            where TInput : notnull
        {
            return await processor(input);
        }

        public void HandleNullableTypes<TNullable>(TNullable? nullable) where TNullable : struct
        {
            if (nullable.HasValue)
            {
                Console.WriteLine(nullable.Value);
            }
        }
    }

    public readonly struct ValueTuple<T1, T2, T3>
    {
        public readonly T1 Item1;
        public readonly T2 Item2;
        public readonly T3 Item3;
    }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = CSharpExtractor::new(
            "c_sharp".to_string(),
            "test.cs".to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);

        let icovariant = symbols.iter().find(|s| s.name == "ICovariant");
        assert!(icovariant.is_some());
        assert_eq!(icovariant.unwrap().kind, SymbolKind::Interface);

        let icontravariant = symbols.iter().find(|s| s.name == "IContravariant");
        assert!(icontravariant.is_some());
        assert_eq!(icontravariant.unwrap().kind, SymbolKind::Interface);

        let repository = symbols.iter().find(|s| s.name == "IRepository");
        assert!(repository.is_some());
        assert!(
            repository
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("where T : class, IEntity, new()")
        );

        let process_async = symbols.iter().find(|s| s.name == "ProcessAsync");
        assert!(process_async.is_some());
        assert!(
            process_async
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("where TResult : class")
        );

        let handle_nullable_types = symbols.iter().find(|s| s.name == "HandleNullableTypes");
        assert!(handle_nullable_types.is_some());
        assert!(
            handle_nullable_types
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("where TNullable : struct")
        );
    }

    #[test]
    fn test_linq_and_lambda_expressions() {
        let code = r#"
namespace LinqExamples
{
    public class QueryService
    {
        public IEnumerable<TResult> QueryData<T, TResult>(
            IEnumerable<T> source,
            Expression<Func<T, bool>> predicate,
            Expression<Func<T, TResult>> selector)
        {
            return source.Where(predicate.Compile()).Select(selector.Compile());
        }

        public async Task<List<User>> GetFilteredUsersAsync(List<User> users)
        {
            var result = from user in users
                        where user.Age > 18 && user.IsActive
                        let fullName = $"{user.FirstName} {user.LastName}"
                        orderby user.LastName, user.FirstName
                        select new User
                        {
                            Id = user.Id,
                            FullName = fullName,
                            Email = user.Email?.ToLower()
                        };

            return await Task.FromResult(result.ToList());
        }

        public void ProcessItems<T>(IEnumerable<T> items, Action<T> processor)
        {
            items.AsParallel()
                 .Where(item => item != null)
                 .ForAll(processor);
        }

        public Func<int, int> CreateMultiplier(int factor) => x => x * factor;

        public Expression<Func<T, bool>> CreatePredicate<T>(string propertyName, object value)
        {
            var parameter = Expression.Parameter(typeof(T), "x");
            var property = Expression.Property(parameter, propertyName);
            var constant = Expression.Constant(value);
            var equality = Expression.Equal(property, constant);
            return Expression.Lambda<Func<T, bool>>(equality, parameter);
        }
    }

    public class LocalFunctionExamples
    {
        public int CalculateFactorial(int n)
        {
            return n <= 1 ? 1 : CalculateFactorialLocal(n);

            static int CalculateFactorialLocal(int num)
            {
                if (num <= 1) return 1;
                return num * CalculateFactorialLocal(num - 1);
            }
        }

        public async Task<string> ProcessDataAsync(string input)
        {
            return await ProcessLocalAsync();

            async Task<string> ProcessLocalAsync()
            {
                await Task.Delay(100);
                return input.ToUpper();
            }
        }
    }

    public class User
    {
        public int Id { get; set; }
        public string FirstName { get; set; } = string.Empty;
        public string LastName { get; set; } = string.Empty;
        public string? Email { get; set; }
        public int Age { get; set; }
        public bool IsActive { get; set; }
        public string FullName { get; set; } = string.Empty;
    }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = CSharpExtractor::new(
            "c_sharp".to_string(),
            "test.cs".to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);

        let query_service = symbols.iter().find(|s| s.name == "QueryService");
        assert!(query_service.is_some());

        let query_data = symbols.iter().find(|s| s.name == "QueryData");
        assert!(query_data.is_some());
        assert!(
            query_data
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("Expression<Func<T, bool>>")
        );

        let get_filtered_users_async = symbols.iter().find(|s| s.name == "GetFilteredUsersAsync");
        assert!(get_filtered_users_async.is_some());
        assert!(
            get_filtered_users_async
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("async Task<List<User>>")
        );

        let process_items = symbols.iter().find(|s| s.name == "ProcessItems");
        assert!(process_items.is_some());
        assert!(
            process_items
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("Action<T>")
        );

        let create_multiplier = symbols.iter().find(|s| s.name == "CreateMultiplier");
        assert!(create_multiplier.is_some());
        assert!(
            create_multiplier
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("Func<int, int>")
        );

        let create_predicate = symbols.iter().find(|s| s.name == "CreatePredicate");
        assert!(create_predicate.is_some());
        assert!(
            create_predicate
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("Expression<Func<T, bool>>")
        );

        let local_function_examples = symbols.iter().find(|s| s.name == "LocalFunctionExamples");
        assert!(local_function_examples.is_some());

        let calculate_factorial = symbols.iter().find(|s| s.name == "CalculateFactorial");
        assert!(calculate_factorial.is_some());
        assert_eq!(calculate_factorial.unwrap().kind, SymbolKind::Method);

        let process_data_async = symbols.iter().find(|s| s.name == "ProcessDataAsync");
        assert!(process_data_async.is_some());
    }
}
