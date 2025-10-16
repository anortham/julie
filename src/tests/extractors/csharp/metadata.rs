use super::*;

#[cfg(test)]
mod tests {
    use super::*;

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

        let status = symbols.iter().find(|s| s.name == "Status");
        assert!(status.is_some());
        assert_eq!(status.unwrap().kind, SymbolKind::Enum);

        let pending = symbols.iter().find(|s| s.name == "Pending");
        assert!(pending.is_some());
        assert_eq!(pending.unwrap().kind, SymbolKind::EnumMember);

        let file_access = symbols.iter().find(|s| s.name == "FileAccess");
        assert!(file_access.is_some());
        assert!(file_access
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains(": byte"));

        let all = symbols.iter().find(|s| s.name == "All");
        assert!(all.is_some());
        assert!(all
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("Read | Write | Execute"));
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

        let user = symbols.iter().find(|s| s.name == "User");
        assert!(user.is_some());
        let signature = user.unwrap().signature.as_ref().unwrap();
        assert!(signature.contains("[Serializable]"));
        assert!(signature.contains("[DataContract]"));

        let get_user = symbols.iter().find(|s| s.name == "GetUserAsync");
        assert!(get_user.is_some());
        let signature = get_user.unwrap().signature.as_ref().unwrap();
        assert!(signature.contains("[HttpGet"));
        assert!(signature.contains("[Authorize"));

        let full_name = symbols.iter().find(|s| s.name == "FullName");
        assert!(full_name.is_some());
        assert!(full_name
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("[JsonProperty"));

        let person = symbols.iter().find(|s| s.name == "Person");
        assert!(person.is_some());
        assert_eq!(person.unwrap().kind, SymbolKind::Class);
        assert!(person
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("record Person"));

        let point = symbols.iter().find(|s| s.name == "Point");
        assert!(point.is_some());
        assert_eq!(point.unwrap().kind, SymbolKind::Struct);
        assert!(point
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("record struct"));
    }

    #[test]
    fn test_delegate_and_nested_classes() {
        let code = r#"
namespace MyProject
{
    public delegate void EventHandler<T>(object sender, T args);

    public class OuterClass
    {
        public class NestedClass
        {
            public void DoSomething() {}
        }

        protected struct NestedStruct
        {
            public int Value;
        }

        private enum NestedEnum
        {
            One,
            Two
        }

        public event EventHandler<string> MessageReceived;
        public static Func<T, TResult> Create<T, TResult>() => null;
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

        let event_handler = symbols.iter().find(|s| s.name == "EventHandler");
        assert!(event_handler.is_some());
        assert_eq!(event_handler.unwrap().kind, SymbolKind::Delegate);
        assert!(event_handler
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("delegate void EventHandler<T>"));

        // The Create method should be extracted (not Func which is just a type reference)
        let create_method = symbols.iter().find(|s| s.name == "Create");
        assert!(create_method.is_some());
        assert_eq!(create_method.unwrap().kind, SymbolKind::Method);
        assert!(create_method
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("Func<T, TResult>"));

        let nested_class = symbols.iter().find(|s| s.name == "NestedClass");
        assert!(nested_class.is_some());
        assert_eq!(nested_class.unwrap().kind, SymbolKind::Class);

        let nested_struct = symbols.iter().find(|s| s.name == "NestedStruct");
        assert!(nested_struct.is_some());
        assert_eq!(nested_struct.unwrap().kind, SymbolKind::Struct);
        assert_eq!(
            nested_struct
                .unwrap()
                .visibility
                .as_ref()
                .map(|v| VisibilityExt::to_string(v).to_lowercase())
                .unwrap_or_else(|| "private".to_string()),
            "protected"
        );

        let nested_enum = symbols.iter().find(|s| s.name == "NestedEnum");
        assert!(nested_enum.is_some());
        assert_eq!(
            nested_enum
                .unwrap()
                .visibility
                .as_ref()
                .map(|v| VisibilityExt::to_string(v).to_lowercase())
                .unwrap_or_else(|| "private".to_string()),
            "private"
        );
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

        let get_name = symbols.iter().find(|s| s.name == "GetName");
        assert!(get_name.is_some());
        assert_eq!(types.get(&get_name.unwrap().id).unwrap(), "string");

        let get_users = symbols.iter().find(|s| s.name == "GetUsersAsync");
        assert!(get_users.is_some());
        assert_eq!(
            types.get(&get_users.unwrap().id).unwrap(),
            "Task<List<User>>"
        );
    }
}
