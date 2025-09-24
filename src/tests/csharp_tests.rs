// C# Extractor Tests
//
// Direct port of Miller's comprehensive C# extractor tests (TDD RED phase)
// Original: /Users/murphy/Source/miller/src/__tests__/parser/csharp-extractor.test.ts
//
// This file contains 20 comprehensive test categories covering:
// - Namespaces, using statements, classes, interfaces, structs
// - Methods, properties, constructors, fields, events, enums
// - Attributes, records, delegates, nested types
// - Modern C# features, generics, LINQ, exception handling
// - Testing patterns, performance testing, edge cases

use crate::extractors::base::{Symbol, SymbolKind, RelationshipKind};
use crate::extractors::csharp::CSharpExtractor;
use tree_sitter::Parser;
use std::collections::HashMap;

/// Initialize C# parser for testing
fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_c_sharp::LANGUAGE.into()).expect("Error loading C# grammar");
    parser
}

#[cfg(test)]
mod csharp_extractor_tests {
    use super::*;

    #[test]
    fn test_namespace_and_using_extraction() {
        let code = r#"
using System;
using System.Collections.Generic;
using static System.Math;

namespace MyCompany.MyProject
{
    // Content here
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = CSharpExtractor::new(
            "c_sharp".to_string(),
            "test.cs".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        // Find System import
        let system_import = symbols.iter().find(|s| s.name == "System");
        assert!(system_import.is_some());
        assert_eq!(system_import.unwrap().kind, SymbolKind::Import);

        // Find static import
        let static_import = symbols.iter().find(|s| s.name == "Math");
        assert!(static_import.is_some());
        assert!(static_import.unwrap().signature.as_ref().unwrap().contains("using static"));

        // Find namespace
        let namespace = symbols.iter().find(|s| s.name == "MyCompany.MyProject");
        assert!(namespace.is_some());
        assert_eq!(namespace.unwrap().kind, SymbolKind::Namespace);
    }

    #[test]
    fn test_class_extraction() {
        let code = r#"
namespace MyProject
{
    public abstract class BaseEntity<T> where T : class
    {
        public int Id { get; set; }
    }

    public sealed class User : BaseEntity<User>, IEquatable<User>
    {
        private readonly string _name;

        public User(string name)
        {
            _name = name;
        }
    }

    internal class InternalClass
    {
        // Internal class content
    }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = CSharpExtractor::new(
            "c_sharp".to_string(),
            "test.cs".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        // Find BaseEntity
        let base_entity = symbols.iter().find(|s| s.name == "BaseEntity");
        assert!(base_entity.is_some());
        assert_eq!(base_entity.unwrap().kind, SymbolKind::Class);
        assert!(base_entity.unwrap().signature.as_ref().unwrap().contains("public abstract class BaseEntity<T>"));
        assert_eq!(base_entity.unwrap().visibility.as_ref().unwrap().to_string().to_lowercase(), "public");

        // Find User class
        let user = symbols.iter().find(|s| s.name == "User");
        assert!(user.is_some());
        assert!(user.unwrap().signature.as_ref().unwrap().contains("sealed"));
        assert!(user.unwrap().signature.as_ref().unwrap().contains("BaseEntity<User>"));
        assert!(user.unwrap().signature.as_ref().unwrap().contains("IEquatable<User>"));

        // Find InternalClass
        let internal_class = symbols.iter().find(|s| s.name == "InternalClass");
        assert!(internal_class.is_some());
        assert_eq!(get_csharp_visibility(internal_class.unwrap()), "internal");
    }

    #[test]
    fn test_interface_and_struct_extraction() {
        let code = r#"
namespace MyProject
{
    public interface IRepository<T> where T : class
    {
        Task<T> GetByIdAsync(int id);
        void Delete(T entity);
    }

    public struct Point
    {
        public int X { get; }
        public int Y { get; }

        public Point(int x, int y)
        {
            X = x;
            Y = y;
        }
    }

    public readonly struct ReadOnlyPoint
    {
        public readonly int X;
        public readonly int Y;
    }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = CSharpExtractor::new(
            "c_sharp".to_string(),
            "test.cs".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        // Find IRepository interface
        let repository = symbols.iter().find(|s| s.name == "IRepository");
        assert!(repository.is_some());
        assert_eq!(repository.unwrap().kind, SymbolKind::Interface);
        assert!(repository.unwrap().signature.as_ref().unwrap().contains("public interface IRepository<T>"));

        // Find Point struct
        let point = symbols.iter().find(|s| s.name == "Point");
        assert!(point.is_some());
        assert_eq!(point.unwrap().kind, SymbolKind::Struct);
        assert!(point.unwrap().signature.as_ref().unwrap().contains("public struct Point"));

        // Find ReadOnlyPoint struct
        let readonly_point = symbols.iter().find(|s| s.name == "ReadOnlyPoint");
        assert!(readonly_point.is_some());
        assert!(readonly_point.unwrap().signature.as_ref().unwrap().contains("readonly"));
    }

    #[test]
    fn test_method_extraction() {
        let code = r#"
namespace MyProject
{
    public class Calculator
    {
        public static int Add(int a, int b)
        {
            return a + b;
        }

        public async Task<string> GetDataAsync()
        {
            return await SomeAsyncOperation();
        }

        protected virtual void ProcessData<T>(T data) where T : class
        {
            // Process data
        }

        private static readonly Func<int, int> Square = x => x * x;

        public override string ToString()
        {
            return "Calculator";
        }
    }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = CSharpExtractor::new(
            "c_sharp".to_string(),
            "test.cs".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        // Find Add method
        let add = symbols.iter().find(|s| s.name == "Add");
        assert!(add.is_some());
        assert_eq!(add.unwrap().kind, SymbolKind::Method);
        assert!(add.unwrap().signature.as_ref().unwrap().contains("public static int Add(int a, int b)"));
        assert_eq!(add.unwrap().visibility.as_ref().unwrap().to_string().to_lowercase(), "public");

        // Find GetDataAsync method
        let get_data_async = symbols.iter().find(|s| s.name == "GetDataAsync");
        assert!(get_data_async.is_some());
        assert!(get_data_async.unwrap().signature.as_ref().unwrap().contains("async"));
        assert!(get_data_async.unwrap().signature.as_ref().unwrap().contains("Task<string>"));

        // Find ProcessData method
        let process_data = symbols.iter().find(|s| s.name == "ProcessData");
        assert!(process_data.is_some());
        assert!(process_data.unwrap().signature.as_ref().unwrap().contains("protected virtual"));
        assert!(process_data.unwrap().signature.as_ref().unwrap().contains("<T>"));

        // Find ToString method
        let to_string = symbols.iter().find(|s| s.name == "ToString");
        assert!(to_string.is_some());
        assert!(to_string.unwrap().signature.as_ref().unwrap().contains("override"));
    }

    #[test]
    fn test_property_extraction() {
        let code = r#"
namespace MyProject
{
    public class Person
    {
        // Auto property
        public string Name { get; set; }

        // Read-only auto property
        public int Age { get; }

        // Property with private setter
        public string Email { get; private set; }

        // Full property with backing field
        private string _address;
        public string Address
        {
            get { return _address; }
            set { _address = value?.Trim(); }
        }

        // Expression-bodied property
        public string FullName => $"{FirstName} {LastName}";

        // Static property
        public static int Count { get; set; }
    }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = CSharpExtractor::new(
            "c_sharp".to_string(),
            "test.cs".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        // Find Name property
        let name = symbols.iter().find(|s| s.name == "Name");
        assert!(name.is_some());
        assert_eq!(name.unwrap().kind, SymbolKind::Property);
        assert!(name.unwrap().signature.as_ref().unwrap().contains("public string Name { get; set; }"));

        // Find Age property
        let age = symbols.iter().find(|s| s.name == "Age");
        assert!(age.is_some());
        assert!(age.unwrap().signature.as_ref().unwrap().contains("{ get; }"));

        // Find Email property
        let email = symbols.iter().find(|s| s.name == "Email");
        assert!(email.is_some());
        assert!(email.unwrap().signature.as_ref().unwrap().contains("private set"));

        // Find FullName property
        let full_name = symbols.iter().find(|s| s.name == "FullName");
        assert!(full_name.is_some());
        assert!(full_name.unwrap().signature.as_ref().unwrap().contains("=>"));

        // Find Count property
        let count = symbols.iter().find(|s| s.name == "Count");
        assert!(count.is_some());
        assert!(count.unwrap().signature.as_ref().unwrap().contains("static"));
    }

    #[test]
    fn test_constructor_extraction() {
        let code = r#"
namespace MyProject
{
    public class Configuration
    {
        static Configuration()
        {
            // Static constructor
        }

        public Configuration()
        {
            // Default constructor
        }

        public Configuration(string path) : this()
        {
            // Constructor with base call
        }

        private Configuration(string path, bool validate) : base(path)
        {
            // Private constructor with base call
        }
    }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = CSharpExtractor::new(
            "c_sharp".to_string(),
            "test.cs".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        let constructors: Vec<&Symbol> = symbols.iter().filter(|s| s.kind == SymbolKind::Constructor).collect();
        assert_eq!(constructors.len(), 4);

        let static_constructor = constructors.iter().find(|s| s.signature.as_ref().unwrap().contains("static"));
        assert!(static_constructor.is_some());

        let default_constructor = constructors.iter().find(|s| s.signature.as_ref().unwrap().contains("Configuration()"));
        assert!(default_constructor.is_some());
        assert_eq!(default_constructor.unwrap().visibility.as_ref().unwrap().to_string().to_lowercase(), "public");

        let private_constructor = constructors.iter().find(|s|
            s.signature.as_ref().unwrap().contains("private") &&
            s.signature.as_ref().unwrap().contains("bool validate")
        );
        assert!(private_constructor.is_some());
        assert_eq!(private_constructor.unwrap().visibility.as_ref().unwrap().to_string().to_lowercase(), "private");
    }

    #[test]
    fn test_field_and_event_extraction() {
        let code = r#"
namespace MyProject
{
    public class EventPublisher
    {
        public event Action<string> MessageReceived;
        public static event EventHandler GlobalEvent;

        private readonly ILogger _logger;
        public const string Version = "1.0.0";
        public static readonly DateTime StartTime = DateTime.Now;

        private string _name;
        protected internal int _count;
    }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = CSharpExtractor::new(
            "c_sharp".to_string(),
            "test.cs".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        // Find MessageReceived event
        let message_received = symbols.iter().find(|s| s.name == "MessageReceived");
        assert!(message_received.is_some());
        assert_eq!(message_received.unwrap().kind, SymbolKind::Event);
        assert!(message_received.unwrap().signature.as_ref().unwrap().contains("event Action<string>"));

        // Find GlobalEvent
        let global_event = symbols.iter().find(|s| s.name == "GlobalEvent");
        assert!(global_event.is_some());
        assert!(global_event.unwrap().signature.as_ref().unwrap().contains("static event"));

        // Find Version constant
        let version = symbols.iter().find(|s| s.name == "Version");
        assert!(version.is_some());
        assert_eq!(version.unwrap().kind, SymbolKind::Constant);
        assert!(version.unwrap().signature.as_ref().unwrap().contains("const"));

        // Find StartTime field
        let start_time = symbols.iter().find(|s| s.name == "StartTime");
        assert!(start_time.is_some());
        assert!(start_time.unwrap().signature.as_ref().unwrap().contains("static readonly"));

        // Find _count field
        let count = symbols.iter().find(|s| s.name == "_count");
        assert!(count.is_some());
        assert_eq!(count.unwrap().visibility.as_ref().unwrap().to_string().to_lowercase(), "protected");
    }

    #[test]
    fn test_enum_extraction() {
        let code = r#"
namespace MyProject
{
    public enum Status
    {
        Pending,
        Active,
        Inactive
    }

    [Flags]
    public enum FileAccess : byte
    {
        None = 0,
        Read = 1,
        Write = 2,
        Execute = 4,
        All = Read | Write | Execute
    }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = CSharpExtractor::new(
            "c_sharp".to_string(),
            "test.cs".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        // Find Status enum
        let status = symbols.iter().find(|s| s.name == "Status");
        assert!(status.is_some());
        assert_eq!(status.unwrap().kind, SymbolKind::Enum);

        // Find Pending enum member
        let pending = symbols.iter().find(|s| s.name == "Pending");
        assert!(pending.is_some());
        assert_eq!(pending.unwrap().kind, SymbolKind::EnumMember);

        // Find FileAccess enum
        let file_access = symbols.iter().find(|s| s.name == "FileAccess");
        assert!(file_access.is_some());
        assert!(file_access.unwrap().signature.as_ref().unwrap().contains(": byte"));

        // Find All enum member
        let all = symbols.iter().find(|s| s.name == "All");
        assert!(all.is_some());
        assert!(all.unwrap().signature.as_ref().unwrap().contains("Read | Write | Execute"));
    }

    #[test]
    fn test_attribute_and_record_extraction() {
        let code = r#"
namespace MyProject
{
    [Serializable]
    [DataContract]
    public class User
    {
        [HttpGet("/users/{id}")]
        [Authorize(Roles = "Admin")]
        public async Task<User> GetUserAsync(int id)
        {
            return null;
        }

        [JsonProperty("full_name")]
        public string FullName { get; set; }
    }

    public record Person(string FirstName, string LastName);

    public record struct Point(int X, int Y);
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = CSharpExtractor::new(
            "c_sharp".to_string(),
            "test.cs".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        // Find User class
        let user = symbols.iter().find(|s| s.name == "User");
        assert!(user.is_some());
        assert!(user.unwrap().signature.as_ref().unwrap().contains("[Serializable]"));
        assert!(user.unwrap().signature.as_ref().unwrap().contains("[DataContract]"));

        // Find GetUserAsync method
        let get_user = symbols.iter().find(|s| s.name == "GetUserAsync");
        assert!(get_user.is_some());
        assert!(get_user.unwrap().signature.as_ref().unwrap().contains("[HttpGet"));
        assert!(get_user.unwrap().signature.as_ref().unwrap().contains("[Authorize"));

        // Find FullName property
        let full_name = symbols.iter().find(|s| s.name == "FullName");
        assert!(full_name.is_some());
        assert!(full_name.unwrap().signature.as_ref().unwrap().contains("[JsonProperty"));

        // Find Person record
        let person = symbols.iter().find(|s| s.name == "Person");
        assert!(person.is_some());
        assert_eq!(person.unwrap().kind, SymbolKind::Class); // Records are classes
        assert!(person.unwrap().signature.as_ref().unwrap().contains("record Person"));

        // Find Point record struct
        let point = symbols.iter().find(|s| s.name == "Point");
        assert!(point.is_some());
        assert_eq!(point.unwrap().kind, SymbolKind::Struct);
        assert!(point.unwrap().signature.as_ref().unwrap().contains("record struct"));
    }

    #[test]
    fn test_delegate_and_nested_classes() {
        let code = r#"
namespace MyProject
{
    public delegate void EventHandler<T>(T data);
    public delegate TResult Func<in T, out TResult>(T input);

    public class OuterClass
    {
        public class NestedClass
        {
            private static void NestedMethod() { }
        }

        protected internal struct NestedStruct
        {
            public int Value;
        }

        private enum NestedEnum
        {
            Option1, Option2
        }
    }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = CSharpExtractor::new(
            "c_sharp".to_string(),
            "test.cs".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        // Find EventHandler delegate
        let event_handler = symbols.iter().find(|s| s.name == "EventHandler");
        assert!(event_handler.is_some());
        assert_eq!(event_handler.unwrap().kind, SymbolKind::Delegate);
        assert!(event_handler.unwrap().signature.as_ref().unwrap().contains("delegate void EventHandler<T>"));

        // Find Func delegate
        let func = symbols.iter().find(|s| s.name == "Func");
        assert!(func.is_some());
        assert!(func.unwrap().signature.as_ref().unwrap().contains("in T, out TResult"));

        // Find NestedClass
        let nested_class = symbols.iter().find(|s| s.name == "NestedClass");
        assert!(nested_class.is_some());
        assert_eq!(nested_class.unwrap().kind, SymbolKind::Class);

        // Find NestedStruct
        let nested_struct = symbols.iter().find(|s| s.name == "NestedStruct");
        assert!(nested_struct.is_some());
        assert_eq!(nested_struct.unwrap().kind, SymbolKind::Struct);
        assert_eq!(nested_struct.unwrap().visibility.as_ref().unwrap().to_string().to_lowercase(), "protected");

        // Find NestedEnum
        let nested_enum = symbols.iter().find(|s| s.name == "NestedEnum");
        assert!(nested_enum.is_some());
        assert_eq!(nested_enum.unwrap().visibility.as_ref().unwrap().to_string().to_lowercase(), "private");
    }

    #[test]
    fn test_type_inference() {
        let code = r#"
namespace MyProject
{
    public class TypeExample
    {
        public string GetName() => "test";
        public Task<List<User>> GetUsersAsync() => null;
        public void ProcessData<T>(T data) where T : class { }
    }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = CSharpExtractor::new(
            "c_sharp".to_string(),
            "test.cs".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);
        let types = extractor.infer_types(&symbols);

        // Find GetName method
        let get_name = symbols.iter().find(|s| s.name == "GetName");
        assert!(get_name.is_some());
        assert_eq!(types.get(&get_name.unwrap().id).unwrap(), "string");

        // Find GetUsersAsync method
        let get_users = symbols.iter().find(|s| s.name == "GetUsersAsync");
        assert!(get_users.is_some());
        assert_eq!(types.get(&get_users.unwrap().id).unwrap(), "Task<List<User>>");
    }

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

        let mut extractor = CSharpExtractor::new(
            "c_sharp".to_string(),
            "test.cs".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);
        let relationships = extractor.extract_relationships(&tree, &symbols);

        // Should find inheritance and implementation relationships
        assert!(relationships.len() >= 1);

        let inheritance = relationships.iter().find(|r| {
            r.kind.to_string() == "extends" &&
            symbols.iter().find(|s| s.id == r.from_symbol_id).unwrap().name == "User" &&
            symbols.iter().find(|s| s.id == r.to_symbol_id).unwrap().name == "BaseEntity"
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
                // Process item
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

        let mut extractor = CSharpExtractor::new(
            "c_sharp".to_string(),
            "test.cs".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        // Find AsyncService class
        let async_service = symbols.iter().find(|s| s.name == "AsyncService");
        assert!(async_service.is_some());
        assert_eq!(async_service.unwrap().kind, SymbolKind::Class);

        // Find GetDataAsync method
        let get_data_async = symbols.iter().find(|s| s.name == "GetDataAsync");
        assert!(get_data_async.is_some());
        assert!(get_data_async.unwrap().signature.as_ref().unwrap().contains("async Task<string?>"));
        assert!(get_data_async.unwrap().signature.as_ref().unwrap().contains("CancellationToken"));

        // Find CountItemsAsync method
        let count_items_async = symbols.iter().find(|s| s.name == "CountItemsAsync");
        assert!(count_items_async.is_some());
        assert!(count_items_async.unwrap().signature.as_ref().unwrap().contains("ValueTask<int>"));

        // Find GetItemsAsync method
        let get_items_async = symbols.iter().find(|s| s.name == "GetItemsAsync");
        assert!(get_items_async.is_some());
        assert!(get_items_async.unwrap().signature.as_ref().unwrap().contains("IAsyncEnumerable<string>"));
        assert!(get_items_async.unwrap().signature.as_ref().unwrap().contains("[EnumeratorCancellation]"));

        // Find NullableExample class
        let nullable_example = symbols.iter().find(|s| s.name == "NullableExample");
        assert!(nullable_example.is_some());

        // Find NullableString property
        let nullable_string = symbols.iter().find(|s| s.name == "NullableString");
        assert!(nullable_string.is_some());
        assert!(nullable_string.unwrap().signature.as_ref().unwrap().contains("string?"));
        assert!(nullable_string.unwrap().signature.as_ref().unwrap().contains("init;"));

        // Find RequiredString property
        let required_string = symbols.iter().find(|s| s.name == "RequiredString");
        assert!(required_string.is_some());
        assert!(required_string.unwrap().signature.as_ref().unwrap().contains("required string"));
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

    public class ShapeCalculator
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

        let mut extractor = CSharpExtractor::new(
            "c_sharp".to_string(),
            "test.cs".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        // Find Shape record
        let shape = symbols.iter().find(|s| s.name == "Shape");
        assert!(shape.is_some());
        assert!(shape.unwrap().signature.as_ref().unwrap().contains("abstract record"));

        // Find Circle record
        let circle = symbols.iter().find(|s| s.name == "Circle");
        assert!(circle.is_some());
        assert!(circle.unwrap().signature.as_ref().unwrap().contains("record Circle(double Radius)"));
        assert!(circle.unwrap().signature.as_ref().unwrap().contains(": Shape"));

        // Find CalculateArea method
        let calculate_area = symbols.iter().find(|s| s.name == "CalculateArea");
        assert!(calculate_area.is_some());
        assert!(calculate_area.unwrap().signature.as_ref().unwrap().contains("=> shape switch"));

        // Find DescribeShape method
        let describe_shape = symbols.iter().find(|s| s.name == "DescribeShape");
        assert!(describe_shape.is_some());
        assert_eq!(describe_shape.unwrap().kind, SymbolKind::Method);

        // Find IsLargeShape method
        let is_large_shape = symbols.iter().find(|s| s.name == "IsLargeShape");
        assert!(is_large_shape.is_some());
        assert!(is_large_shape.unwrap().signature.as_ref().unwrap().contains("is Circle"));

        // Find ProcessValue method
        let process_value = symbols.iter().find(|s| s.name == "ProcessValue");
        assert!(process_value.is_some());
        assert!(process_value.unwrap().signature.as_ref().unwrap().contains("object value"));
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

        public ValueTuple(T1 item1, T2 item2, T3 item3)
        {
            Item1 = item1;
            Item2 = item2;
            Item3 = item3;
        }

        public void Deconstruct(out T1 item1, out T2 item2, out T3 item3)
        {
            item1 = Item1;
            item2 = Item2;
            item3 = Item3;
        }
    }

    public class TupleExamples
    {
        public (string Name, int Age, DateTime Birth) GetPersonInfo() => ("John", 30, DateTime.Now);
        public (int Sum, int Product) Calculate(int a, int b) => (a + b, a * b);
    }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = CSharpExtractor::new(
            "c_sharp".to_string(),
            "test.cs".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        // Find ICovariant interface
        let covariant = symbols.iter().find(|s| s.name == "ICovariant");
        assert!(covariant.is_some());
        assert!(covariant.unwrap().signature.as_ref().unwrap().contains("out T"));

        // Find IContravariant interface
        let contravariant = symbols.iter().find(|s| s.name == "IContravariant");
        assert!(contravariant.is_some());
        assert!(contravariant.unwrap().signature.as_ref().unwrap().contains("in T"));

        // Find IRepository interface
        let repository = symbols.iter().find(|s| s.name == "IRepository");
        assert!(repository.is_some());
        assert!(repository.unwrap().signature.as_ref().unwrap().contains("where T : class, IEntity, new()"));

        // Find GetByIdAsync method
        let get_by_id_async = symbols.iter().find(|s| s.name == "GetByIdAsync");
        assert!(get_by_id_async.is_some());
        assert!(get_by_id_async.unwrap().signature.as_ref().unwrap().contains("<TKey>"));
        assert!(get_by_id_async.unwrap().signature.as_ref().unwrap().contains("where TKey : struct"));

        // Find GenericService class
        let generic_service = symbols.iter().find(|s| s.name == "GenericService");
        assert!(generic_service.is_some());
        assert!(generic_service.unwrap().signature.as_ref().unwrap().contains("<T, U, V>"));
        assert!(generic_service.unwrap().signature.as_ref().unwrap().contains("where T : class, IDisposable"));

        // Find ProcessAsync method
        let process_async = symbols.iter().find(|s| s.name == "ProcessAsync");
        assert!(process_async.is_some());
        assert!(process_async.unwrap().signature.as_ref().unwrap().contains("<TResult, TInput>"));
        assert!(process_async.unwrap().signature.as_ref().unwrap().contains("where TResult : class"));
        assert!(process_async.unwrap().signature.as_ref().unwrap().contains("where TInput : notnull"));

        // Find HandleNullableTypes method
        let handle_nullable_types = symbols.iter().find(|s| s.name == "HandleNullableTypes");
        assert!(handle_nullable_types.is_some());
        assert!(handle_nullable_types.unwrap().signature.as_ref().unwrap().contains("TNullable?"));

        // Find ValueTuple struct
        let value_tuple = symbols.iter().find(|s| s.name == "ValueTuple");
        assert!(value_tuple.is_some());
        assert_eq!(value_tuple.unwrap().kind, SymbolKind::Struct);
        assert!(value_tuple.unwrap().signature.as_ref().unwrap().contains("readonly struct"));

        // Find Deconstruct method
        let deconstruct = symbols.iter().find(|s| s.name == "Deconstruct");
        assert!(deconstruct.is_some());
        assert!(deconstruct.unwrap().signature.as_ref().unwrap().contains("out T1"));

        // Find GetPersonInfo method
        let get_person_info = symbols.iter().find(|s| s.name == "GetPersonInfo");
        assert!(get_person_info.is_some());
        assert!(get_person_info.unwrap().signature.as_ref().unwrap().contains("(string Name, int Age, DateTime Birth)"));
    }

    #[test]
    fn test_linq_and_lambda_expressions() {
        let code = r#"
using System.Linq.Expressions;

namespace LinqExamples
{
    public class QueryService
    {
        public IQueryable<TResult> QueryData<T, TResult>(IQueryable<T> source, Expression<Func<T, bool>> predicate, Expression<Func<T, TResult>> selector)
        {
            return source.Where(predicate).Select(selector);
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

        let mut extractor = CSharpExtractor::new(
            "c_sharp".to_string(),
            "test.cs".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        // Find QueryService class
        let query_service = symbols.iter().find(|s| s.name == "QueryService");
        assert!(query_service.is_some());
        assert_eq!(query_service.unwrap().kind, SymbolKind::Class);

        // Find QueryData method
        let query_data = symbols.iter().find(|s| s.name == "QueryData");
        assert!(query_data.is_some());
        assert!(query_data.unwrap().signature.as_ref().unwrap().contains("Expression<Func<T, bool>>"));
        assert!(query_data.unwrap().signature.as_ref().unwrap().contains("Expression<Func<T, TResult>>"));

        // Find GetFilteredUsersAsync method
        let get_filtered_users_async = symbols.iter().find(|s| s.name == "GetFilteredUsersAsync");
        assert!(get_filtered_users_async.is_some());
        assert!(get_filtered_users_async.unwrap().signature.as_ref().unwrap().contains("async Task<List<User>>"));

        // Find ProcessItems method
        let process_items = symbols.iter().find(|s| s.name == "ProcessItems");
        assert!(process_items.is_some());
        assert!(process_items.unwrap().signature.as_ref().unwrap().contains("Action<T>"));

        // Find CreateMultiplier method
        let create_multiplier = symbols.iter().find(|s| s.name == "CreateMultiplier");
        assert!(create_multiplier.is_some());
        assert!(create_multiplier.unwrap().signature.as_ref().unwrap().contains("Func<int, int>"));
        assert!(create_multiplier.unwrap().signature.as_ref().unwrap().contains("=> x => x * factor"));

        // Find CreatePredicate method
        let create_predicate = symbols.iter().find(|s| s.name == "CreatePredicate");
        assert!(create_predicate.is_some());
        assert!(create_predicate.unwrap().signature.as_ref().unwrap().contains("Expression<Func<T, bool>>"));

        // Find LocalFunctionExamples class
        let local_function_examples = symbols.iter().find(|s| s.name == "LocalFunctionExamples");
        assert!(local_function_examples.is_some());

        // Find CalculateFactorial method
        let calculate_factorial = symbols.iter().find(|s| s.name == "CalculateFactorial");
        assert!(calculate_factorial.is_some());
        assert_eq!(calculate_factorial.unwrap().kind, SymbolKind::Method);

        // Find ProcessDataAsync method
        let process_data_async = symbols.iter().find(|s| s.name == "ProcessDataAsync");
        assert!(process_data_async.is_some());
        assert!(process_data_async.unwrap().signature.as_ref().unwrap().contains("async Task<string>"));

        // Find User class
        let user = symbols.iter().find(|s| s.name == "User");
        assert!(user.is_some());
        assert_eq!(user.unwrap().kind, SymbolKind::Class);
    }

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

        let mut extractor = CSharpExtractor::new(
            "c_sharp".to_string(),
            "test.cs".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        // Find CustomException class
        let custom_exception = symbols.iter().find(|s| s.name == "CustomException");
        assert!(custom_exception.is_some());
        assert_eq!(custom_exception.unwrap().kind, SymbolKind::Class);
        assert!(custom_exception.unwrap().signature.as_ref().unwrap().contains(": Exception"));

        // Find ErrorCode property
        let error_code = symbols.iter().find(|s| s.name == "ErrorCode");
        assert!(error_code.is_some());
        assert_eq!(error_code.unwrap().kind, SymbolKind::Property);

        // Find ResourceManager class
        let resource_manager = symbols.iter().find(|s| s.name == "ResourceManager");
        assert!(resource_manager.is_some());
        assert!(resource_manager.unwrap().signature.as_ref().unwrap().contains(": IDisposable, IAsyncDisposable"));

        // Find constructors
        let constructors: Vec<&Symbol> = symbols.iter().filter(|s|
            s.kind == SymbolKind::Constructor &&
            s.signature.as_ref().unwrap().contains("ResourceManager")
        ).collect();
        assert!(constructors.len() >= 1);

        // Find Dispose method (public)
        let dispose = symbols.iter().find(|s|
            s.name == "Dispose" &&
            !s.signature.as_ref().unwrap().contains("bool")
        );
        assert!(dispose.is_some());
        assert_eq!(dispose.unwrap().visibility.as_ref().unwrap().to_string().to_lowercase(), "public");

        // Find Dispose method (protected)
        let dispose_protected = symbols.iter().find(|s|
            s.name == "Dispose" &&
            s.signature.as_ref().unwrap().contains("bool")
        );
        assert!(dispose_protected.is_some());
        assert_eq!(dispose_protected.unwrap().visibility.as_ref().unwrap().to_string().to_lowercase(), "protected");

        // Find DisposeAsync method
        let dispose_async = symbols.iter().find(|s| s.name == "DisposeAsync");
        assert!(dispose_async.is_some());
        assert!(dispose_async.unwrap().signature.as_ref().unwrap().contains("ValueTask"));

        // Find DisposeAsyncCore method
        let dispose_async_core = symbols.iter().find(|s| s.name == "DisposeAsyncCore");
        assert!(dispose_async_core.is_some());
        assert_eq!(dispose_async_core.unwrap().visibility.as_ref().unwrap().to_string().to_lowercase(), "protected");

        // Find finalizer
        let finalizer = symbols.iter().find(|s| s.signature.as_ref().unwrap().contains("~ResourceManager"));
        assert!(finalizer.is_some());

        // Find ExceptionUtilities class
        let exception_utilities = symbols.iter().find(|s| s.name == "ExceptionUtilities");
        assert!(exception_utilities.is_some());
        assert!(exception_utilities.unwrap().signature.as_ref().unwrap().contains("static class"));

        // Find HandleException method
        let handle_exception = symbols.iter().find(|s| s.name == "HandleException");
        assert!(handle_exception.is_some());
        assert!(handle_exception.unwrap().signature.as_ref().unwrap().contains("static void"));
    }

    #[test]
    fn test_csharp_testing_patterns() {
        let code = r#"
using Xunit;
using NUnit.Framework;
using Microsoft.VisualStudio.TestTools.UnitTesting;

namespace TestExamples
{
    // xUnit Test Class
    public class XUnitTests : IClassFixture<DatabaseFixture>, IDisposable
    {
        private readonly DatabaseFixture _fixture;

        public XUnitTests(DatabaseFixture fixture)
        {
            _fixture = fixture;
        }

        [Fact]
        public void ShouldCalculateCorrectly()
        {
            // Arrange
            var calculator = new Calculator();

            // Act
            var result = calculator.Add(2, 3);

            // Assert
            Assert.Equal(5, result);
        }

        [Theory]
        [InlineData(2, 3, 5)]
        [InlineData(-1, 1, 0)]
        public void ShouldAddNumbers(int a, int b, int expected)
        {
            var calculator = new Calculator();
            var result = calculator.Add(a, b);
            Assert.Equal(expected, result);
        }

        public void Dispose()
        {
            // Cleanup
        }
    }

    // NUnit Test Class
    [TestFixture]
    [Category("Integration")]
    public class NUnitTests
    {
        private Calculator _calculator = null!;

        [OneTimeSetUp]
        public void OneTimeSetup()
        {
            // One-time setup
        }

        [SetUp]
        public void Setup()
        {
            _calculator = new Calculator();
        }

        [Test]
        [TestCase(1, 2, 3)]
        [TestCase(10, 20, 30)]
        public void Add_ShouldReturnCorrectSum(int a, int b, int expected)
        {
            var result = _calculator.Add(a, b);
            Assert.That(result, Is.EqualTo(expected));
        }

        [Test]
        [Timeout(5000)]
        public async Task ProcessData_ShouldCompleteWithinTimeout()
        {
            await Task.Delay(1000);
            Assert.Pass();
        }

        [Test]
        [Ignore("Temporarily disabled")]
        public void IgnoredTest()
        {
            Assert.Fail("This should not run");
        }
    }

    // MSTest Test Class
    [TestClass]
    [TestCategory("Unit")]
    public class MSTestTests
    {
        private Calculator? _calculator;

        [ClassInitialize]
        public static void ClassInitialize(TestContext context)
        {
            // Class-level initialization
        }

        [TestInitialize]
        public void TestInitialize()
        {
            _calculator = new Calculator();
        }

        [TestMethod]
        [DataRow(1, 2, 3)]
        [DataRow(10, 15, 25)]
        public void Add_WithDataRows_ShouldReturnExpectedResult(int a, int b, int expected)
        {
            var result = _calculator!.Add(a, b);
            Assert.AreEqual(expected, result);
        }

        [TestMethod]
        [ExpectedException(typeof(ArgumentException))]
        public void Divide_ByZero_ShouldThrowException()
        {
            _calculator!.Divide(10, 0);
        }

        [TestMethod]
        [Owner("TeamA")]
        [Priority(1)]
        public void HighPriorityTest()
        {
            Assert.IsTrue(true);
        }
    }

    public class DatabaseFixture : IDisposable
    {
        public string ConnectionString { get; } = "test_connection";
        public void Dispose() { }
    }

    public class Calculator
    {
        public int Add(int a, int b) => a + b;
        public int Divide(int a, int b) => a / b;
    }
}
"#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = CSharpExtractor::new(
            "c_sharp".to_string(),
            "test.cs".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        // xUnit Tests
        let xunit_tests = symbols.iter().find(|s| s.name == "XUnitTests");
        assert!(xunit_tests.is_some());
        assert!(xunit_tests.unwrap().signature.as_ref().unwrap().contains(": IClassFixture<DatabaseFixture>, IDisposable"));

        let should_calculate_correctly = symbols.iter().find(|s| s.name == "ShouldCalculateCorrectly");
        assert!(should_calculate_correctly.is_some());
        assert!(should_calculate_correctly.unwrap().signature.as_ref().unwrap().contains("[Fact]"));

        let should_add_numbers = symbols.iter().find(|s| s.name == "ShouldAddNumbers");
        assert!(should_add_numbers.is_some());
        assert!(should_add_numbers.unwrap().signature.as_ref().unwrap().contains("[Theory]"));
        assert!(should_add_numbers.unwrap().signature.as_ref().unwrap().contains("[InlineData"));

        // NUnit Tests
        let nunit_tests = symbols.iter().find(|s| s.name == "NUnitTests");
        assert!(nunit_tests.is_some());
        assert!(nunit_tests.unwrap().signature.as_ref().unwrap().contains("[TestFixture]"));
        assert!(nunit_tests.unwrap().signature.as_ref().unwrap().contains("[Category(\"Integration\")]"));

        let one_time_setup = symbols.iter().find(|s| s.name == "OneTimeSetup");
        assert!(one_time_setup.is_some());
        assert!(one_time_setup.unwrap().signature.as_ref().unwrap().contains("[OneTimeSetUp]"));

        let setup = symbols.iter().find(|s| s.name == "Setup");
        assert!(setup.is_some());
        assert!(setup.unwrap().signature.as_ref().unwrap().contains("[SetUp]"));

        let add_should_return_correct_sum = symbols.iter().find(|s| s.name == "Add_ShouldReturnCorrectSum");
        assert!(add_should_return_correct_sum.is_some());
        assert!(add_should_return_correct_sum.unwrap().signature.as_ref().unwrap().contains("[TestCase"));

        let process_data_should_complete_within_timeout = symbols.iter().find(|s| s.name == "ProcessData_ShouldCompleteWithinTimeout");
        assert!(process_data_should_complete_within_timeout.is_some());
        assert!(process_data_should_complete_within_timeout.unwrap().signature.as_ref().unwrap().contains("[Timeout(5000)]"));

        let ignored_test = symbols.iter().find(|s| s.name == "IgnoredTest");
        assert!(ignored_test.is_some());
        assert!(ignored_test.unwrap().signature.as_ref().unwrap().contains("[Ignore"));

        // MSTest Tests
        let ms_test_tests = symbols.iter().find(|s| s.name == "MSTestTests");
        assert!(ms_test_tests.is_some());
        assert!(ms_test_tests.unwrap().signature.as_ref().unwrap().contains("[TestClass]"));
        assert!(ms_test_tests.unwrap().signature.as_ref().unwrap().contains("[TestCategory(\"Unit\")]"));

        let class_initialize = symbols.iter().find(|s| s.name == "ClassInitialize");
        assert!(class_initialize.is_some());
        assert!(class_initialize.unwrap().signature.as_ref().unwrap().contains("[ClassInitialize]"));
        assert!(class_initialize.unwrap().signature.as_ref().unwrap().contains("static"));

        let test_initialize = symbols.iter().find(|s| s.name == "TestInitialize");
        assert!(test_initialize.is_some());
        assert!(test_initialize.unwrap().signature.as_ref().unwrap().contains("[TestInitialize]"));

        let add_with_data_rows = symbols.iter().find(|s| s.name == "Add_WithDataRows_ShouldReturnExpectedResult");
        assert!(add_with_data_rows.is_some());
        assert!(add_with_data_rows.unwrap().signature.as_ref().unwrap().contains("[DataRow"));

        let divide_by_zero = symbols.iter().find(|s| s.name == "Divide_ByZero_ShouldThrowException");
        assert!(divide_by_zero.is_some());
        assert!(divide_by_zero.unwrap().signature.as_ref().unwrap().contains("[ExpectedException"));

        let high_priority_test = symbols.iter().find(|s| s.name == "HighPriorityTest");
        assert!(high_priority_test.is_some());
        assert!(high_priority_test.unwrap().signature.as_ref().unwrap().contains("[Owner(\"TeamA\")]"));
        assert!(high_priority_test.unwrap().signature.as_ref().unwrap().contains("[Priority(1)]"));

        // Supporting classes
        let database_fixture = symbols.iter().find(|s| s.name == "DatabaseFixture");
        assert!(database_fixture.is_some());
        assert!(database_fixture.unwrap().signature.as_ref().unwrap().contains(": IDisposable"));

        let calculator = symbols.iter().find(|s| s.name == "Calculator");
        assert!(calculator.is_some());
        assert_eq!(calculator.unwrap().kind, SymbolKind::Class);
    }

    #[test]
    fn test_performance_testing() {
        // Generate large codebase with many symbols
        let mut large_codebase = r#"
namespace PerformanceTest
{
    public interface IService<T> where T : BaseEntity
    {
        Task<T> GetByIdAsync(int id, CancellationToken cancellationToken = default);
        Task<IEnumerable<T>> GetAllAsync(CancellationToken cancellationToken = default);
    }

    public interface IRepository<T> where T : BaseEntity
    {
        Task<T> GetByIdAsync(int id, CancellationToken cancellationToken = default);
        Task<IEnumerable<T>> GetAllAsync(CancellationToken cancellationToken = default);
        Task<T> CreateAsync(T entity, CancellationToken cancellationToken = default);
        Task<T> UpdateAsync(T entity, CancellationToken cancellationToken = default);
        Task DeleteAsync(T entity, CancellationToken cancellationToken = default);
    }

    public abstract class BaseEntity
    {
        public int Id { get; set; }
        public DateTime CreatedAt { get; set; }
        public DateTime? UpdatedAt { get; set; }
    }

    public interface IMapper
    {
        TDestination Map<TDestination>(object source);
        void Map<TSource, TDestination>(TSource source, TDestination destination);
    }

    public class NotFoundException : Exception
    {
        public NotFoundException(string message) : base(message) { }
    }
"#.to_string();

        // Generate 20 service classes with complex structures
        for i in 1..=20 {
            large_codebase.push_str(&format!(r#"
    public class Service{i} : IService<Entity{i}>
    {{
        private readonly IRepository<Entity{i}> _repository;
        private readonly ILogger<Service{i}> _logger;
        private readonly IMapper _mapper;

        public Service{i}(IRepository<Entity{i}> repository, ILogger<Service{i}> logger, IMapper mapper)
        {{
            _repository = repository ?? throw new ArgumentNullException(nameof(repository));
            _logger = logger ?? throw new ArgumentNullException(nameof(logger));
            _mapper = mapper ?? throw new ArgumentNullException(nameof(mapper));
        }}

        public async Task<Entity{i}> GetByIdAsync(int id, CancellationToken cancellationToken = default)
        {{
            try
            {{
                _logger.LogInformation("Retrieving entity with ID {{Id}}", id);
                var entity = await _repository.GetByIdAsync(id, cancellationToken);
                return entity ?? throw new NotFoundException($"Entity{i} with ID {{id}} not found");
            }}
            catch (Exception ex)
            {{
                _logger.LogError(ex, "Error retrieving entity with ID {{Id}}", id);
                throw;
            }}
        }}

        public async Task<IEnumerable<Entity{i}>> GetAllAsync(CancellationToken cancellationToken = default)
        {{
            return await _repository.GetAllAsync(cancellationToken);
        }}

        public async Task<Entity{i}> CreateAsync(CreateEntity{i}Request request, CancellationToken cancellationToken = default)
        {{
            var entity = _mapper.Map<Entity{i}>(request);
            return await _repository.CreateAsync(entity, cancellationToken);
        }}

        public async Task<Entity{i}> UpdateAsync(int id, UpdateEntity{i}Request request, CancellationToken cancellationToken = default)
        {{
            var existingEntity = await GetByIdAsync(id, cancellationToken);
            _mapper.Map(request, existingEntity);
            return await _repository.UpdateAsync(existingEntity, cancellationToken);
        }}

        public async Task DeleteAsync(int id, CancellationToken cancellationToken = default)
        {{
            var entity = await GetByIdAsync(id, cancellationToken);
            await _repository.DeleteAsync(entity, cancellationToken);
        }}
    }}

    public class Entity{i} : BaseEntity
    {{
        public string Name {{ get; set; }} = string.Empty;
        public string Description {{ get; set; }} = string.Empty;
        public DateTime CreatedAt {{ get; set; }}
        public DateTime? UpdatedAt {{ get; set; }}
        public bool IsActive {{ get; set; }} = true;
        public decimal Value{i} {{ get; set; }}
        public EntityType{i} Type {{ get; set; }}

        public virtual ICollection<RelatedEntity{i}> RelatedEntities {{ get; set; }} = new List<RelatedEntity{i}>();
    }}

    public enum EntityType{i}
    {{
        Type1,
        Type2,
        Type3
    }}

    public record CreateEntity{i}Request(string Name, string Description, decimal Value{i}, EntityType{i} Type);
    public record UpdateEntity{i}Request(string Name, string Description, decimal Value{i}, bool IsActive);

    public class RelatedEntity{i}
    {{
        public int Id {{ get; set; }}
        public int Entity{i}Id {{ get; set; }}
        public virtual Entity{i} Entity{i} {{ get; set; }} = null!;
        public string RelatedData {{ get; set; }} = string.Empty;
    }}
"#, i = i));
        }

        large_codebase.push_str("\n}");

        let mut parser = init_parser();
        let tree = parser.parse(&large_codebase, None).unwrap();

        let mut extractor = CSharpExtractor::new(
            "c_sharp".to_string(),
            "large-test.cs".to_string(),
            large_codebase,
        );

        let start_time = std::time::Instant::now();
        let symbols = extractor.extract_symbols(&tree);
        let duration = start_time.elapsed();

        // Should extract all symbols efficiently
        assert!(symbols.len() > 200); // Many symbols expected
        assert!(duration.as_millis() < 5000); // Should complete within 5 seconds

        // Verify some key symbols are extracted
        let services: Vec<&Symbol> = symbols.iter().filter(|s|
            s.name.starts_with("Service") &&
            s.kind == SymbolKind::Class
        ).collect();
        assert_eq!(services.len(), 20);

        let entities: Vec<&Symbol> = symbols.iter().filter(|s|
            s.name.starts_with("Entity") &&
            s.kind == SymbolKind::Class &&
            !s.name.contains("Related")
        ).collect();
        assert_eq!(entities.len(), 20);

        let get_by_id_methods: Vec<&Symbol> = symbols.iter().filter(|s| s.name == "GetByIdAsync").collect();
        assert!(get_by_id_methods.len() >= 20);

        let enums: Vec<&Symbol> = symbols.iter().filter(|s|
            s.name.starts_with("EntityType") &&
            s.kind == SymbolKind::Enum
        ).collect();
        assert_eq!(enums.len(), 20);

        let records: Vec<&Symbol> = symbols.iter().filter(|s|
            s.name.contains("Request") &&
            s.signature.as_ref().unwrap().contains("record")
        ).collect();
        assert_eq!(records.len(), 40); // 2 records per entity (Create and Update)
    }

    #[test]
    fn test_edge_cases_and_error_handling() {
        let code = r#"
#if DEBUG
#define TRACE_ENABLED
#endif

namespace EdgeCases
{
    // Nested generic constraints
    public class ComplexGeneric<T, U, V>
        where T : class, IComparable<T>, new()
        where U : struct, IEquatable<U>
        where V : T, IDisposable
    {
        public async Task<TResult> ProcessAsync<TResult, TInput>(
            TInput input,
            Func<TInput, Task<TResult>> processor,
            CancellationToken cancellationToken = default)
        where TResult : class
        where TInput : notnull
        {
            return await processor(input);
        }
    }

    // Complex inheritance chain
    public abstract class BaseClass<T> : IDisposable, IAsyncDisposable where T : class
    {
        protected abstract Task<T> ProcessInternalAsync();
        public abstract void Dispose();
        public abstract ValueTask DisposeAsync();
    }

    public class DerivedClass<T, U> : BaseClass<T> where T : class where U : struct
    {
        protected override async Task<T> ProcessInternalAsync() => await Task.FromResult(default(T)!);
        public override void Dispose() { }
        public override ValueTask DisposeAsync() => ValueTask.CompletedTask;
    }

    // Operator overloading
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
            => new(a.Real + b.Real, a.Imaginary + b.Imaginary);

        public static ComplexNumber operator -(ComplexNumber a, ComplexNumber b)
            => new(a.Real - b.Real, a.Imaginary - b.Imaginary);

        public static implicit operator ComplexNumber(double real) => new(real, 0);
        public static explicit operator double(ComplexNumber complex) => complex.Real;

        public static bool operator ==(ComplexNumber left, ComplexNumber right) => left.Equals(right);
        public static bool operator !=(ComplexNumber left, ComplexNumber right) => !left.Equals(right);

        public override bool Equals(object? obj) => obj is ComplexNumber other && Equals(other);
        public bool Equals(ComplexNumber other) => Real == other.Real && Imaginary == other.Imaginary;
        public override int GetHashCode() => HashCode.Combine(Real, Imaginary);
    }

    // Indexers and properties
    public class IndexedCollection<T>
    {
        private readonly List<T> _items = new();

        public T this[int index]
        {
            get => _items[index];
            set => _items[index] = value;
        }

        public T this[string key]
        {
            get => _items.FirstOrDefault()!;
            set { /* Implementation */ }
        }

        public int Count => _items.Count;
        public bool IsEmpty => _items.Count == 0;
    }

    // Unsafe code
    public unsafe class UnsafeOperations
    {
        public static unsafe int ProcessPointer(int* ptr)
        {
            return *ptr * 2;
        }

        public static unsafe void ProcessArray(int[] array)
        {
            fixed (int* ptr = array)
            {
                for (int i = 0; i < array.Length; i++)
                {
                    *(ptr + i) *= 2;
                }
            }
        }
    }

    // Malformed code that should be handled gracefully
    /* This is intentionally malformed to test error handling */
    /*
    public class IncompleteClass
    {
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

    // Raw string literals and other modern features
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

        let mut extractor = CSharpExtractor::new(
            "c_sharp".to_string(),
            "complex-test.cs".to_string(),
            code.to_string(),
        );

        let symbols = extractor.extract_symbols(&tree);

        // Should handle complex generics
        let complex_generic = symbols.iter().find(|s| s.name == "ComplexGeneric");
        assert!(complex_generic.is_some());
        assert!(complex_generic.unwrap().signature.as_ref().unwrap().contains("<T, U, V>"));
        assert!(complex_generic.unwrap().signature.as_ref().unwrap().contains("where T : class"));

        let process_async = symbols.iter().find(|s| s.name == "ProcessAsync");
        assert!(process_async.is_some());
        assert!(process_async.unwrap().signature.as_ref().unwrap().contains("<TResult, TInput>"));

        // Should handle inheritance
        let base_class = symbols.iter().find(|s| s.name == "BaseClass");
        assert!(base_class.is_some());
        assert!(base_class.unwrap().signature.as_ref().unwrap().contains("abstract class"));

        let derived_class = symbols.iter().find(|s| s.name == "DerivedClass");
        assert!(derived_class.is_some());
        assert!(derived_class.unwrap().signature.as_ref().unwrap().contains(": BaseClass<T>"));

        // Should handle operators
        let complex_number = symbols.iter().find(|s| s.name == "ComplexNumber");
        assert!(complex_number.is_some());
        assert_eq!(complex_number.unwrap().kind, SymbolKind::Struct);

        let operators: Vec<&Symbol> = symbols.iter().filter(|s|
            s.signature.as_ref().unwrap().contains("operator")
        ).collect();
        assert!(operators.len() >= 4); // +, -, ==, !=, implicit, explicit

        // Should handle indexers
        let indexed_collection = symbols.iter().find(|s| s.name == "IndexedCollection");
        assert!(indexed_collection.is_some());

        let indexers: Vec<&Symbol> = symbols.iter().filter(|s|
            s.signature.as_ref().unwrap().contains("this[")
        ).collect();
        assert!(indexers.len() >= 2); // int and string indexers

        // Should handle unsafe code
        let unsafe_operations = symbols.iter().find(|s| s.name == "UnsafeOperations");
        assert!(unsafe_operations.is_some());
        assert!(unsafe_operations.unwrap().signature.as_ref().unwrap().contains("unsafe class"));

        let process_pointer = symbols.iter().find(|s| s.name == "ProcessPointer");
        assert!(process_pointer.is_some());
        assert!(process_pointer.unwrap().signature.as_ref().unwrap().contains("unsafe"));

        // Should handle modern string features
        let modern_string_features = symbols.iter().find(|s| s.name == "ModernStringFeatures");
        assert!(modern_string_features.is_some());

        let json_template = symbols.iter().find(|s| s.name == "JsonTemplate");
        assert!(json_template.is_some());
        assert_eq!(json_template.unwrap().kind, SymbolKind::Constant);

        // Should not crash on malformed code
        assert!(symbols.len() > 20); // Should extract valid symbols despite malformed sections
    }
}

// Helper trait for visibility conversion
trait VisibilityExt {
    fn to_string(&self) -> String;
}

impl VisibilityExt for crate::extractors::base::Visibility {
    fn to_string(&self) -> String {
        match self {
            crate::extractors::base::Visibility::Public => "public".to_string(),
            crate::extractors::base::Visibility::Private => "private".to_string(),
            crate::extractors::base::Visibility::Protected => "protected".to_string(),
        }
    }
}

// Helper function to get C# visibility including internal from metadata
fn get_csharp_visibility(symbol: &crate::extractors::base::Symbol) -> String {
    // Check metadata for stored csharp_visibility
    if let Some(csharp_visibility) = symbol.metadata.get("csharp_visibility") {
        if let Some(vis) = csharp_visibility.as_str() {
            return vis.to_string();
        }
    }
    // Fallback to standard visibility
    symbol.visibility.as_ref().map_or("private".to_string(), |v| v.to_string())
}