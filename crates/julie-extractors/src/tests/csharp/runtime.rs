use super::*;
use std::path::PathBuf;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exception_handling_and_resource_management() {
        let code = r#"
namespace ExceptionHandling
{
    public class CustomException : Exception
    {
        public string ErrorCode { get; }

        public CustomException(string message, string errorCode) : base(message)
        {
            ErrorCode = errorCode;
        }

        public CustomException(string message, string errorCode, Exception innerException)
            : base(message, innerException)
        {
            ErrorCode = errorCode;
        }
    }

    public class ResourceManager : IDisposable, IAsyncDisposable
    {
        private bool _disposed = false;
        private readonly FileStream? _fileStream;

        public ResourceManager(string filePath)
        {
            try
            {
                _fileStream = new FileStream(filePath, FileMode.Open);
            }
            catch (FileNotFoundException ex)
            {
                throw new CustomException($"File not found: {filePath}", "FILE_NOT_FOUND", ex);
            }
            catch (UnauthorizedAccessException)
            {
                throw new CustomException("Access denied", "ACCESS_DENIED");
            }
        }

        public void Dispose()
        {
            Dispose(true);
            GC.SuppressFinalize(this);
        }

        protected virtual void Dispose(bool disposing)
        {
            if (!_disposed)
            {
                if (disposing)
                {
                    _fileStream?.Dispose();
                }
                _disposed = true;
            }
        }

        public async ValueTask DisposeAsync()
        {
            await DisposeAsyncCore();
            Dispose(false);
            GC.SuppressFinalize(this);
        }

        protected virtual async ValueTask DisposeAsyncCore()
        {
            if (_fileStream is not null)
            {
                await _fileStream.DisposeAsync();
            }
        }

        ~ResourceManager()
        {
            Dispose(false);
        }
    }

    public static class ExceptionUtilities
    {
        public static void HandleException(Exception ex)
        {
            switch (ex)
            {
                case CustomException customEx:
                    Console.WriteLine($"Custom error: {customEx.ErrorCode}");
                    break;
                case ArgumentNullException argEx:
                    Console.WriteLine($"Null argument: {argEx.ParamName}");
                    break;
                case Exception generalEx:
                    Console.WriteLine($"General error: {generalEx.Message}");
                    break;
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

        let custom_exception = symbols.iter().find(|s| s.name == "CustomException");
        assert!(custom_exception.is_some());
        assert_eq!(custom_exception.unwrap().kind, SymbolKind::Class);
        assert!(
            custom_exception
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains(": Exception")
        );

        let resource_manager = symbols.iter().find(|s| s.name == "ResourceManager");
        assert!(resource_manager.is_some());
        assert!(
            resource_manager
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("IDisposable")
        );
        assert!(
            resource_manager
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("IAsyncDisposable")
        );

        let dispose_async = symbols.iter().find(|s| s.name == "DisposeAsync");
        assert!(dispose_async.is_some());
        assert!(
            dispose_async
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("ValueTask")
        );

        let handle_exception = symbols.iter().find(|s| s.name == "HandleException");
        assert!(handle_exception.is_some());
        assert_eq!(handle_exception.unwrap().kind, SymbolKind::Method);
    }

    #[test]
    fn test_csharp_testing_patterns() {
        let code = r#"
namespace TestingPatterns
{
    public abstract class TestBase
    {
        protected virtual void Setup() {}
        protected virtual void Teardown() {}
    }

    public class UserTests : TestBase
    {
        public override void Setup()
        {
            base.Setup();
        }

        public void ShouldCreateUser()
        {
            Assert.True(true);
        }
    }

    public class MockRepository<T> where T : class
    {
        public virtual Task<T> GetByIdAsync(int id) => Task.FromResult(default(T)!);
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

        let test_base = symbols.iter().find(|s| s.name == "TestBase");
        assert!(test_base.is_some());
        assert_eq!(test_base.unwrap().kind, SymbolKind::Class);

        let setup = symbols.iter().find(|s| s.name == "Setup");
        assert!(setup.is_some());
        assert!(
            setup
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("protected virtual")
        );

        let should_create_user = symbols.iter().find(|s| s.name == "ShouldCreateUser");
        assert!(should_create_user.is_some());
        assert_eq!(should_create_user.unwrap().kind, SymbolKind::Method);

        let mock_repository = symbols.iter().find(|s| s.name == "MockRepository");
        assert!(mock_repository.is_some());
        assert_eq!(mock_repository.unwrap().kind, SymbolKind::Class);
    }

    #[test]
    fn test_performance_testing() {
        let code = r#"
namespace PerformanceTests
{
    public class Benchmark
    {
        private readonly Stopwatch _stopwatch = new Stopwatch();

        public void Measure(Action action, int iterations = 1000)
        {
            _stopwatch.Restart();
            for (int i = 0; i < iterations; i++)
            {
                action();
            }
            _stopwatch.Stop();
        }

        public async Task MeasureAsync(Func<Task> action, int iterations = 100)
        {
            _stopwatch.Restart();
            for (int i = 0; i < iterations; i++)
            {
                await action();
            }
            _stopwatch.Stop();
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

        let benchmark = symbols.iter().find(|s| s.name == "Benchmark");
        assert!(benchmark.is_some());
        assert_eq!(benchmark.unwrap().kind, SymbolKind::Class);

        let measure = symbols.iter().find(|s| s.name == "Measure");
        assert!(measure.is_some());
        assert!(
            measure
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("Action")
        );

        let measure_async = symbols.iter().find(|s| s.name == "MeasureAsync");
        assert!(measure_async.is_some());
        assert!(
            measure_async
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("Func<Task>")
        );
    }
    #[test]
    fn test_edge_cases_and_error_handling() {
        let code = r#"
namespace EdgeCases
{
    public class ComplexGeneric<T, U, V>
        where T : class, IDisposable
        where U : struct
        where V : T, new()
    {
        public async Task<TResult> ProcessAsync<TResult, TInput>(TInput input, Func<TInput, Task<TResult>> processor)
            where TResult : class
            where TInput : notnull
        {
            return await processor(input);
        }
    }

    public abstract class BaseClass<T>
    {
        public abstract T Process(T input);
    }

    public class DerivedClass<T> : BaseClass<T>
    {
        public override T Process(T input)
        {
            return input;
        }
    }

    public struct ComplexNumber
    {
        public double Real { get; }
        public double Imaginary { get; }

        public ComplexNumber(double real, double imaginary)
        {
            Real = real;
            Imaginary = imaginary;
        }

        public static ComplexNumber operator +(ComplexNumber a, ComplexNumber b)
            => new ComplexNumber(a.Real + b.Real, a.Imaginary + b.Imaginary);

        public static bool operator ==(ComplexNumber a, ComplexNumber b)
            => a.Real == b.Real && a.Imaginary == b.Imaginary;

        public static bool operator !=(ComplexNumber a, ComplexNumber b)
            => !(a == b);
    }

    public class IndexedCollection
    {
        private readonly Dictionary<int, string> _items = new();

        public string this[int index]
        {
            get => _items[index];
            set => _items[index] = value;
        }

        public string this[string key]
        {
            get => _items[int.Parse(key)];
            set => _items[int.Parse(key)] = value;
        }
    }

    public unsafe class UnsafeOperations
    {
        public void ProcessPointer(int* pointer)
        {
            (*pointer)++;
        }
    }

    /* Malformed code section to ensure parser resilience
        public void IncompleteMethod(
        // Missing closing parenthesis and brace
    */

#if TRACE_ENABLED
    public static class TraceUtilities
    {
        [Conditional("TRACE")]
        public static void TraceMessage(string message)
        {
            Console.WriteLine($"TRACE: {message}");
        }
    }
#endif

    public class ModernStringFeatures
    {
        public const string JsonTemplate = """
        {
            "name": "{{name}}",
            "value": {{value}}
        }
        """;

        public static string FormatJson(string name, int value) =>
            JsonTemplate.Replace("{{name}}", name).Replace("{{value}}", value.ToString());
    }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = CSharpExtractor::new(
            "c_sharp".to_string(),
            "complex-test.cs".to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);

        let complex_generic = symbols.iter().find(|s| s.name == "ComplexGeneric");
        assert!(complex_generic.is_some());
        assert!(
            complex_generic
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("<T, U, V>")
        );

        let process_async = symbols.iter().find(|s| s.name == "ProcessAsync");
        assert!(process_async.is_some());
        assert!(
            process_async
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("<TResult, TInput>")
        );

        let complex_number = symbols.iter().find(|s| s.name == "ComplexNumber");
        assert!(complex_number.is_some());
        assert_eq!(complex_number.unwrap().kind, SymbolKind::Struct);

        // Check for operator overloads (should be 3: +, ==, !=)
        let operators: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.signature.as_ref().unwrap().contains("operator"))
            .collect();
        assert!(operators.len() >= 3);

        let indexed_collection = symbols.iter().find(|s| s.name == "IndexedCollection");
        assert!(indexed_collection.is_some());

        let indexers: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.signature.as_ref().unwrap().contains("this["))
            .collect();
        assert!(indexers.len() >= 2);

        let unsafe_operations = symbols.iter().find(|s| s.name == "UnsafeOperations");
        assert!(unsafe_operations.is_some());
        assert!(
            unsafe_operations
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("unsafe class")
        );

        let modern_string_features = symbols.iter().find(|s| s.name == "ModernStringFeatures");
        assert!(modern_string_features.is_some());

        let json_template = symbols.iter().find(|s| s.name == "JsonTemplate");
        assert!(json_template.is_some());
        assert_eq!(json_template.unwrap().kind, SymbolKind::Constant);

        assert!(symbols.len() > 20);
    }
}
