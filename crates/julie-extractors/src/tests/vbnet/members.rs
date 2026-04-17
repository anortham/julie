use super::*;
use std::path::PathBuf;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sub_method_extraction() {
        let code = r#"
Class C
    Public Sub DoWork()
    End Sub
End Class
"#;
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VbNetExtractor::new(
            "vbnet".to_string(),
            "test.vb".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);

        let method = symbols.iter().find(|s| s.name == "DoWork");
        assert!(method.is_some(), "Should find method DoWork");
        let method = method.unwrap();
        assert_eq!(method.kind, SymbolKind::Method);
        assert_eq!(method.visibility, Some(Visibility::Public));
        let sig = method.signature.as_ref().unwrap();
        assert!(
            sig.contains("Sub DoWork"),
            "Signature should contain 'Sub DoWork': {}",
            sig
        );
    }

    #[test]
    fn test_function_method_extraction() {
        let code = r#"
Class Calculator
    Public Function Add(a As Integer, b As Integer) As Integer
        Return a + b
    End Function
End Class
"#;
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VbNetExtractor::new(
            "vbnet".to_string(),
            "test.vb".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);

        let method = symbols.iter().find(|s| s.name == "Add");
        assert!(method.is_some(), "Should find method Add");
        let method = method.unwrap();
        assert_eq!(method.kind, SymbolKind::Method);
        let sig = method.signature.as_ref().unwrap();
        assert!(
            sig.contains("Function Add"),
            "Signature should contain 'Function Add': {}",
            sig
        );
        assert!(
            sig.contains("As Integer"),
            "Signature should contain return type: {}",
            sig
        );
        assert!(
            sig.contains("a As Integer"),
            "Signature should contain params: {}",
            sig
        );
    }

    #[test]
    fn test_default_member_visibility_follows_vb_rules() {
        let code = r#"
Class Service
    Sub Run()
    End Sub

    Property Name As String

    Dim _state As Integer
End Class
"#;
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VbNetExtractor::new(
            "vbnet".to_string(),
            "test.vb".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);

        let run = symbols.iter().find(|s| s.name == "Run").unwrap();
        assert_eq!(get_vb_visibility(run), "public");

        let name = symbols
            .iter()
            .find(|s| s.name == "Name" && s.kind == SymbolKind::Property)
            .unwrap();
        assert_eq!(get_vb_visibility(name), "public");

        let state = symbols
            .iter()
            .find(|s| s.name == "_state" && s.kind == SymbolKind::Field)
            .unwrap();
        assert_eq!(get_vb_visibility(state), "private");
    }

    #[test]
    fn test_shared_method_extraction() {
        let code = r#"
Class MathHelper
    Public Shared Function Square(x As Integer) As Integer
        Return x * x
    End Function
End Class
"#;
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VbNetExtractor::new(
            "vbnet".to_string(),
            "test.vb".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);

        let method = symbols.iter().find(|s| s.name == "Square");
        assert!(method.is_some(), "Should find method Square");
        let sig = method.unwrap().signature.as_ref().unwrap();
        assert!(
            sig.contains("shared"),
            "Signature should contain 'shared': {}",
            sig
        );
    }

    #[test]
    fn test_constructor_extraction() {
        let code = r#"
Class Person
    Public Sub New(name As String)
    End Sub
End Class
"#;
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VbNetExtractor::new(
            "vbnet".to_string(),
            "test.vb".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);

        let ctor = symbols.iter().find(|s| s.kind == SymbolKind::Constructor);
        assert!(ctor.is_some(), "Should find constructor");
        let ctor = ctor.unwrap();
        assert_eq!(ctor.name, "New");
        assert_eq!(ctor.visibility, Some(Visibility::Public));
        let sig = ctor.signature.as_ref().unwrap();
        assert!(
            sig.contains("Sub New"),
            "Signature should contain 'Sub New': {}",
            sig
        );
        assert!(
            sig.contains("name As String"),
            "Signature should contain params: {}",
            sig
        );
    }

    #[test]
    fn test_multiple_constructors() {
        let code = r#"
Class Config
    Public Sub New()
    End Sub

    Public Sub New(path As String)
    End Sub

    Private Sub New(path As String, validate As Boolean)
    End Sub
End Class
"#;
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VbNetExtractor::new(
            "vbnet".to_string(),
            "test.vb".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);

        let ctors: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Constructor)
            .collect();
        assert_eq!(ctors.len(), 3, "Should have 3 constructors");

        let private_ctor = ctors
            .iter()
            .find(|s| s.visibility == Some(Visibility::Private));
        assert!(private_ctor.is_some(), "Should find private constructor");
    }

    #[test]
    fn test_auto_property_extraction() {
        let code = r#"
Class Person
    Public Property Name As String
    Public Property Age As Integer
End Class
"#;
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VbNetExtractor::new(
            "vbnet".to_string(),
            "test.vb".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);

        let name_prop = symbols
            .iter()
            .find(|s| s.name == "Name" && s.kind == SymbolKind::Property);
        assert!(name_prop.is_some(), "Should find property Name");
        let sig = name_prop.unwrap().signature.as_ref().unwrap();
        assert!(sig.contains("Property Name"), "Signature: {}", sig);
        assert!(
            sig.contains("As String"),
            "Signature should contain type: {}",
            sig
        );

        let age_prop = symbols
            .iter()
            .find(|s| s.name == "Age" && s.kind == SymbolKind::Property);
        assert!(age_prop.is_some(), "Should find property Age");
    }

    #[test]
    fn test_readonly_property_extraction() {
        let code = r#"
Class C
    Public ReadOnly Property IsEmpty As Boolean
        Get
            Return Count = 0
        End Get
    End Property
End Class
"#;
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VbNetExtractor::new(
            "vbnet".to_string(),
            "test.vb".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);

        let prop = symbols
            .iter()
            .find(|s| s.name == "IsEmpty" && s.kind == SymbolKind::Property);
        assert!(prop.is_some(), "Should find property IsEmpty");
        let sig = prop.unwrap().signature.as_ref().unwrap();
        assert!(
            sig.contains("readonly"),
            "Signature should contain 'readonly': {}",
            sig
        );
        assert!(
            sig.contains("As Boolean"),
            "Signature should contain type: {}",
            sig
        );
    }

    #[test]
    fn test_indexed_property_extraction() {
        let code = r#"
Class C
    Public Property Item(index As Integer) As String
        Get
            Return _items(index)
        End Get
        Set(value As String)
            _items(index) = value
        End Set
    End Property
End Class
"#;
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VbNetExtractor::new(
            "vbnet".to_string(),
            "test.vb".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);

        let prop = symbols
            .iter()
            .find(|s| s.name == "Item" && s.kind == SymbolKind::Property);
        assert!(prop.is_some(), "Should find indexed property Item");
        let sig = prop.unwrap().signature.as_ref().unwrap();
        assert!(
            sig.contains("index As Integer"),
            "Signature should contain index param: {}",
            sig
        );
    }

    #[test]
    fn test_field_extraction() {
        let code = r#"
Class Person
    Private _name As String
    Public Age As Integer
End Class
"#;
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VbNetExtractor::new(
            "vbnet".to_string(),
            "test.vb".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);

        let name_field = symbols
            .iter()
            .find(|s| s.name == "_name" && s.kind == SymbolKind::Field);
        assert!(name_field.is_some(), "Should find field _name");
        let name_field = name_field.unwrap();
        assert_eq!(name_field.visibility, Some(Visibility::Private));
        let sig = name_field.signature.as_ref().unwrap();
        assert!(
            sig.contains("As String"),
            "Field signature should contain type: {}",
            sig
        );

        let age_field = symbols
            .iter()
            .find(|s| s.name == "Age" && s.kind == SymbolKind::Field);
        assert!(age_field.is_some(), "Should find field Age");
        assert_eq!(age_field.unwrap().visibility, Some(Visibility::Public));
    }

    #[test]
    fn test_shared_field_extraction() {
        let code = r#"
Class Counter
    Public Shared Count As Integer
End Class
"#;
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VbNetExtractor::new(
            "vbnet".to_string(),
            "test.vb".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);

        let field = symbols
            .iter()
            .find(|s| s.name == "Count" && s.kind == SymbolKind::Field);
        assert!(field.is_some(), "Should find shared field Count");
        let sig = field.unwrap().signature.as_ref().unwrap();
        assert!(
            sig.contains("shared"),
            "Signature should contain 'shared': {}",
            sig
        );
    }

    #[test]
    fn test_dim_field_extraction() {
        let code = r#"
Class C
    Dim _value As Integer
End Class
"#;
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VbNetExtractor::new(
            "vbnet".to_string(),
            "test.vb".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);

        let field = symbols
            .iter()
            .find(|s| s.name == "_value" && s.kind == SymbolKind::Field);
        assert!(
            field.is_some(),
            "Should find field _value declared with Dim"
        );
        let sig = field.unwrap().signature.as_ref().unwrap();
        assert!(
            sig.contains("Dim _value"),
            "Signature should contain 'Dim _value': {}",
            sig
        );
        assert!(
            sig.contains("As Integer"),
            "Signature should contain type: {}",
            sig
        );
    }

    #[test]
    fn test_typed_event_extraction() {
        let code = r#"
Class C
    Public Event Click As EventHandler
End Class
"#;
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VbNetExtractor::new(
            "vbnet".to_string(),
            "test.vb".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);

        let event = symbols
            .iter()
            .find(|s| s.name == "Click" && s.kind == SymbolKind::Event);
        assert!(event.is_some(), "Should find event Click");
        let event = event.unwrap();
        assert_eq!(event.visibility, Some(Visibility::Public));
        let sig = event.signature.as_ref().unwrap();
        assert!(sig.contains("Event Click"), "Signature: {}", sig);
        assert!(
            sig.contains("As EventHandler"),
            "Signature should contain type: {}",
            sig
        );
    }

    #[test]
    fn test_parameterized_event_extraction() {
        let code = r#"
Class C
    Public Event ValueChanged(sender As Object, newValue As Integer)
End Class
"#;
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VbNetExtractor::new(
            "vbnet".to_string(),
            "test.vb".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);

        let event = symbols
            .iter()
            .find(|s| s.name == "ValueChanged" && s.kind == SymbolKind::Event);
        assert!(event.is_some(), "Should find event ValueChanged");
        let sig = event.unwrap().signature.as_ref().unwrap();
        assert!(sig.contains("Event ValueChanged"), "Signature: {}", sig);
    }

    #[test]
    fn test_operator_extraction() {
        let code = r#"
Class Vector
    Public Shared Operator +(a As Vector, b As Vector) As Vector
        Return New Vector()
    End Operator
End Class
"#;
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VbNetExtractor::new(
            "vbnet".to_string(),
            "test.vb".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);

        let op = symbols.iter().find(|s| s.kind == SymbolKind::Operator);
        assert!(op.is_some(), "Should find operator +");
        let op = op.unwrap();
        assert!(
            op.name.contains("+"),
            "Operator name should contain '+': {}",
            op.name
        );
        let sig = op.signature.as_ref().unwrap();
        assert!(sig.contains("Operator +"), "Signature: {}", sig);
        assert!(
            sig.contains("As Vector"),
            "Signature should contain return type: {}",
            sig
        );
    }

    #[test]
    fn test_const_extraction() {
        let code = r#"
Class C
    Public Const MaxSize As Integer = 100
    Private Const DefaultName As String = "Unknown"
End Class
"#;
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VbNetExtractor::new(
            "vbnet".to_string(),
            "test.vb".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);

        let max_size = symbols
            .iter()
            .find(|s| s.name == "MaxSize" && s.kind == SymbolKind::Constant);
        assert!(max_size.is_some(), "Should find constant MaxSize");
        let max_size = max_size.unwrap();
        assert_eq!(max_size.visibility, Some(Visibility::Public));
        let sig = max_size.signature.as_ref().unwrap();
        assert!(sig.contains("Const MaxSize"), "Signature: {}", sig);
        assert!(
            sig.contains("As Integer"),
            "Signature should contain type: {}",
            sig
        );

        let default_name = symbols
            .iter()
            .find(|s| s.name == "DefaultName" && s.kind == SymbolKind::Constant);
        assert!(default_name.is_some(), "Should find constant DefaultName");
        assert_eq!(default_name.unwrap().visibility, Some(Visibility::Private));
    }

    #[test]
    fn test_method_parent_relationship() {
        let code = r#"
Class MyService
    Public Sub Initialize()
    End Sub

    Public Function GetData() As String
        Return "data"
    End Function
End Class
"#;
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VbNetExtractor::new(
            "vbnet".to_string(),
            "test.vb".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);

        let class = symbols
            .iter()
            .find(|s| s.name == "MyService" && s.kind == SymbolKind::Class);
        assert!(class.is_some(), "Should find class MyService");
        let class_id = &class.unwrap().id;

        let init = symbols.iter().find(|s| s.name == "Initialize");
        assert!(init.is_some(), "Should find method Initialize");
        assert_eq!(init.unwrap().parent_id.as_ref(), Some(class_id));

        let get_data = symbols.iter().find(|s| s.name == "GetData");
        assert!(get_data.is_some(), "Should find method GetData");
        assert_eq!(get_data.unwrap().parent_id.as_ref(), Some(class_id));
    }

    #[test]
    fn test_abstract_method_in_interface() {
        let code = r#"
Interface IShape
    Function Area() As Double
    Sub Draw()
End Interface
"#;
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();
        let workspace_root = PathBuf::from("/tmp/test");
        let mut extractor = VbNetExtractor::new(
            "vbnet".to_string(),
            "test.vb".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let symbols = extractor.extract_symbols(&tree);

        let area = symbols
            .iter()
            .find(|s| s.name == "Area" && s.kind == SymbolKind::Method);
        assert!(area.is_some(), "Should find abstract method Area");
        let sig = area.unwrap().signature.as_ref().unwrap();
        assert!(sig.contains("Function Area"), "Signature: {}", sig);
        assert!(
            sig.contains("As Double"),
            "Signature should contain return type: {}",
            sig
        );

        let draw = symbols
            .iter()
            .find(|s| s.name == "Draw" && s.kind == SymbolKind::Method);
        assert!(draw.is_some(), "Should find abstract method Draw");
        let sig = draw.unwrap().signature.as_ref().unwrap();
        assert!(sig.contains("Sub Draw"), "Signature: {}", sig);
    }
}
