use super::*;
use std::path::PathBuf;

#[cfg(test)]
mod tests {
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

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = CSharpExtractor::new(
            "c_sharp".to_string(),
            "test.cs".to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);

        let system_import = symbols.iter().find(|s| s.name == "System");
        assert!(system_import.is_some());
        assert_eq!(system_import.unwrap().kind, SymbolKind::Import);

        let static_import = symbols.iter().find(|s| s.name == "Math");
        assert!(static_import.is_some());
        assert!(static_import
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("using static"));

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

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = CSharpExtractor::new(
            "c_sharp".to_string(),
            "test.cs".to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);

        let base_entity = symbols.iter().find(|s| s.name == "BaseEntity");
        assert!(base_entity.is_some());
        assert_eq!(base_entity.unwrap().kind, SymbolKind::Class);
        assert!(base_entity
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("public abstract class BaseEntity<T>"));
        assert_eq!(
            base_entity
                .unwrap()
                .visibility
                .as_ref()
                .map(|v| VisibilityExt::to_string(v).to_lowercase())
                .unwrap_or_else(|| "private".to_string()),
            "public"
        );

        let user = symbols.iter().find(|s| s.name == "User");
        assert!(user.is_some());
        assert!(user.unwrap().signature.as_ref().unwrap().contains("sealed"));
        assert!(user
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("BaseEntity<User>"));
        assert!(user
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("IEquatable<User>"));

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

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = CSharpExtractor::new(
            "c_sharp".to_string(),
            "test.cs".to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);

        let repository = symbols.iter().find(|s| s.name == "IRepository");
        assert!(repository.is_some());
        assert_eq!(repository.unwrap().kind, SymbolKind::Interface);
        assert!(repository
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("public interface IRepository<T>"));

        let point = symbols.iter().find(|s| s.name == "Point");
        assert!(point.is_some());
        assert_eq!(point.unwrap().kind, SymbolKind::Struct);
        assert!(point
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("public struct Point"));

        let readonly_point = symbols.iter().find(|s| s.name == "ReadOnlyPoint");
        assert!(readonly_point.is_some());
        assert!(readonly_point
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("readonly"));
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

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = CSharpExtractor::new(
            "c_sharp".to_string(),
            "test.cs".to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);

        let add = symbols.iter().find(|s| s.name == "Add");
        assert!(add.is_some());
        assert_eq!(add.unwrap().kind, SymbolKind::Method);
        assert!(add
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("public static int Add(int a, int b)"));
        assert_eq!(
            add.unwrap()
                .visibility
                .as_ref()
                .map(|v| VisibilityExt::to_string(v).to_lowercase())
                .unwrap_or_else(|| "private".to_string()),
            "public"
        );

        let get_data_async = symbols.iter().find(|s| s.name == "GetDataAsync");
        assert!(get_data_async.is_some());
        assert!(get_data_async
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("async"));
        assert!(get_data_async
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("Task<string>"));

        let process_data = symbols.iter().find(|s| s.name == "ProcessData");
        assert!(process_data.is_some());
        assert!(process_data
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("protected virtual"));
        assert!(process_data
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("<T>"));

        let to_string = symbols.iter().find(|s| s.name == "ToString");
        assert!(to_string.is_some());
        assert!(to_string
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("override"));
    }

    #[test]
    fn test_property_extraction() {
        let code = r#"
namespace MyProject
{
    public class Person
    {
        public string Name { get; set; }
        public int Age { get; }
        public string Email { get; private set; }

        private string _address;
        public string Address
        {
            get { return _address; }
            set { _address = value?.Trim(); }
        }

        public string FullName => $"{FirstName} {LastName}";
        public static int Count { get; set; }
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

        let name = symbols.iter().find(|s| s.name == "Name");
        assert!(name.is_some());
        assert_eq!(name.unwrap().kind, SymbolKind::Property);
        assert!(name
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("public string Name { get; set; }"));

        let age = symbols.iter().find(|s| s.name == "Age");
        assert!(age.is_some());
        assert!(age
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("{ get; }"));

        let email = symbols.iter().find(|s| s.name == "Email");
        assert!(email.is_some());
        assert!(email
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("private set"));

        let full_name = symbols.iter().find(|s| s.name == "FullName");
        assert!(full_name.is_some());
        assert!(full_name
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("=>"));

        let count = symbols.iter().find(|s| s.name == "Count");
        assert!(count.is_some());
        assert!(count
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("static"));
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
        }

        public Configuration()
        {
        }

        public Configuration(string path) : this()
        {
        }

        private Configuration(string path, bool validate) : base(path)
        {
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

        let constructors: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Constructor)
            .collect();
        assert_eq!(constructors.len(), 4);

        let static_constructor = constructors
            .iter()
            .find(|s| s.signature.as_ref().unwrap().contains("static"));
        assert!(static_constructor.is_some());

        let default_constructor = constructors
            .iter()
            .find(|s| s.signature.as_ref().unwrap().contains("Configuration()"));
        assert!(default_constructor.is_some());
        assert_eq!(
            default_constructor
                .unwrap()
                .visibility
                .as_ref()
                .map(|v| VisibilityExt::to_string(v).to_lowercase())
                .unwrap_or_else(|| "private".to_string()),
            "public"
        );

        let private_constructor = constructors.iter().find(|s| {
            let signature = s.signature.as_ref().unwrap();
            signature.contains("private") && signature.contains("bool validate")
        });
        assert!(private_constructor.is_some());
        assert_eq!(
            private_constructor
                .unwrap()
                .visibility
                .as_ref()
                .map(|v| VisibilityExt::to_string(v).to_lowercase())
                .unwrap_or_else(|| "private".to_string()),
            "private"
        );
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

        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = CSharpExtractor::new(
            "c_sharp".to_string(),
            "test.cs".to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);

        let message_received = symbols.iter().find(|s| s.name == "MessageReceived");
        assert!(message_received.is_some());
        assert_eq!(message_received.unwrap().kind, SymbolKind::Event);
        assert!(message_received
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("event Action<string>"));

        let global_event = symbols.iter().find(|s| s.name == "GlobalEvent");
        assert!(global_event.is_some());
        assert!(global_event
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("static event"));

        let version = symbols.iter().find(|s| s.name == "Version");
        assert!(version.is_some());
        assert_eq!(version.unwrap().kind, SymbolKind::Constant);
        assert!(version
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("const"));

        let start_time = symbols.iter().find(|s| s.name == "StartTime");
        assert!(start_time.is_some());
        assert!(start_time
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("static readonly"));

        let count = symbols.iter().find(|s| s.name == "_count");
        assert!(count.is_some());
        assert_eq!(
            count
                .unwrap()
                .visibility
                .as_ref()
                .map(|v| VisibilityExt::to_string(v).to_lowercase())
                .unwrap_or_else(|| "private".to_string()),
            "protected"
        );
    }
}
