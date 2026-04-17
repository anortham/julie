use super::*;
use std::path::PathBuf;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_function_return_type_inference() {
        let code = r#"
Public Class Calculator
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
        let types = extractor.infer_types(&symbols);

        let add_symbol = symbols.iter().find(|s| s.name == "Add").unwrap();
        let inferred = types.get(&add_symbol.id);
        assert!(
            inferred.is_some(),
            "Should infer return type for Function Add"
        );
        assert_eq!(inferred.unwrap(), "Integer");
    }

    #[test]
    fn test_function_string_return_type() {
        let code = r#"
Public Class Formatter
    Public Function Format(value As Object) As String
        Return value.ToString()
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
        let types = extractor.infer_types(&symbols);

        let format_sym = symbols.iter().find(|s| s.name == "Format").unwrap();
        let inferred = types.get(&format_sym.id);
        assert!(inferred.is_some(), "Should infer return type for Format");
        assert_eq!(inferred.unwrap(), "String");
    }

    #[test]
    fn test_sub_has_no_return_type() {
        let code = r#"
Public Class Service
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
        let types = extractor.infer_types(&symbols);

        let do_work = symbols.iter().find(|s| s.name == "DoWork").unwrap();
        let inferred = types.get(&do_work.id);
        assert!(
            inferred.is_none(),
            "Sub should not have a return type, but got: {:?}",
            inferred
        );
    }

    #[test]
    fn test_property_type_inference() {
        let code = r#"
Public Class Person
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
        let types = extractor.infer_types(&symbols);

        let name_prop = symbols
            .iter()
            .find(|s| s.name == "Name" && s.kind == SymbolKind::Property)
            .unwrap();
        let name_type = types.get(&name_prop.id);
        assert!(name_type.is_some(), "Should infer type for Property Name");
        assert_eq!(name_type.unwrap(), "String");

        let age_prop = symbols
            .iter()
            .find(|s| s.name == "Age" && s.kind == SymbolKind::Property)
            .unwrap();
        let age_type = types.get(&age_prop.id);
        assert!(age_type.is_some(), "Should infer type for Property Age");
        assert_eq!(age_type.unwrap(), "Integer");
    }

    #[test]
    fn test_field_type_inference() {
        let code = r#"
Public Class Config
    Private _name As String
    Public MaxRetries As Integer
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
        let types = extractor.infer_types(&symbols);

        let name_field = symbols
            .iter()
            .find(|s| s.name == "_name" && s.kind == SymbolKind::Field)
            .unwrap();
        let name_type = types.get(&name_field.id);
        assert!(name_type.is_some(), "Should infer type for field _name");
        assert_eq!(name_type.unwrap(), "String");

        let retries_field = symbols
            .iter()
            .find(|s| s.name == "MaxRetries" && s.kind == SymbolKind::Field)
            .unwrap();
        let retries_type = types.get(&retries_field.id);
        assert!(
            retries_type.is_some(),
            "Should infer type for field MaxRetries"
        );
        assert_eq!(retries_type.unwrap(), "Integer");
    }

    #[test]
    fn test_function_double_return_type() {
        let code = r#"
Public Class MathHelper
    Public Function CalculateArea(radius As Double) As Double
        Return 3.14159 * radius * radius
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
        let types = extractor.infer_types(&symbols);

        let calc = symbols.iter().find(|s| s.name == "CalculateArea").unwrap();
        let inferred = types.get(&calc.id);
        assert!(
            inferred.is_some(),
            "Should infer Double return type for CalculateArea"
        );
        assert_eq!(inferred.unwrap(), "Double");
    }

    #[test]
    fn test_const_type_inference() {
        let code = r#"
Public Class Settings
    Public Const MaxSize As Integer = 100
    Public Const AppName As String = "MyApp"
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
        let types = extractor.infer_types(&symbols);

        let max_size = symbols
            .iter()
            .find(|s| s.name == "MaxSize" && s.kind == SymbolKind::Constant)
            .unwrap();
        let max_type = types.get(&max_size.id);
        assert!(max_type.is_some(), "Should infer type for Const MaxSize");
        assert_eq!(max_type.unwrap(), "Integer");

        let app_name = symbols
            .iter()
            .find(|s| s.name == "AppName" && s.kind == SymbolKind::Constant)
            .unwrap();
        let app_type = types.get(&app_name.id);
        assert!(app_type.is_some(), "Should infer type for Const AppName");
        assert_eq!(app_type.unwrap(), "String");
    }
}
