use super::*;
use std::path::PathBuf;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_namespace_extraction() {
        let code = r#"
Namespace MyCompany.MyApp
    Public Class C
    End Class
End Namespace
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

        let ns = symbols.iter().find(|s| s.name == "MyCompany.MyApp");
        assert!(ns.is_some(), "Should find namespace MyCompany.MyApp");
        let ns = ns.unwrap();
        assert_eq!(ns.kind, SymbolKind::Namespace);
        assert_eq!(ns.visibility, Some(Visibility::Public));
        assert!(ns.signature.as_ref().unwrap().contains("Namespace MyCompany.MyApp"));
    }

    #[test]
    fn test_nested_namespace_extraction() {
        let code = r#"
Namespace Outer
    Namespace Inner
        Public Class C
        End Class
    End Namespace
End Namespace
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

        let outer = symbols.iter().find(|s| s.name == "Outer");
        assert!(outer.is_some(), "Should find namespace Outer");
        let inner = symbols.iter().find(|s| s.name == "Inner");
        assert!(inner.is_some(), "Should find namespace Inner");
        assert!(inner.unwrap().parent_id.is_some());
    }

    #[test]
    fn test_imports_extraction() {
        let code = r#"Imports System
Imports System.Collections.Generic

Module M
End Module
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

        let system_import = symbols.iter().find(|s| s.kind == SymbolKind::Import && s.name == "System");
        assert!(system_import.is_some(), "Should find import System");
        assert!(system_import.unwrap().signature.as_ref().unwrap().contains("Imports System"));

        let generic_import = symbols.iter().find(|s| s.kind == SymbolKind::Import && s.name == "Generic");
        assert!(generic_import.is_some(), "Should find import Generic (last segment)");
        assert!(
            generic_import.unwrap().signature.as_ref().unwrap()
                .contains("Imports System.Collections.Generic")
        );
    }

    #[test]
    fn test_aliased_imports_extraction() {
        let code = r#"Imports DAL = Company.Data.Access

Module M
End Module
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

        let alias_import = symbols.iter().find(|s| s.kind == SymbolKind::Import && s.name == "DAL");
        assert!(alias_import.is_some(), "Should find aliased import DAL");
        let sig = alias_import.unwrap().signature.as_ref().unwrap();
        assert!(sig.contains("Imports DAL = Company.Data.Access"), "Signature should be: {}", sig);
    }

    #[test]
    fn test_class_extraction() {
        let code = r#"
Public Class MyClass
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

        let class = symbols.iter().find(|s| s.name == "MyClass");
        assert!(class.is_some(), "Should find class MyClass");
        let class = class.unwrap();
        assert_eq!(class.kind, SymbolKind::Class);
        assert_eq!(class.visibility, Some(Visibility::Public));
        assert!(class.signature.as_ref().unwrap().contains("public Class MyClass"));
    }

    #[test]
    fn test_class_with_inherits_and_implements() {
        let code = r#"
Public Class Derived
    Inherits Base
    Implements IDisposable, ICloneable

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

        let class = symbols.iter().find(|s| s.name == "Derived");
        assert!(class.is_some(), "Should find class Derived");
        let sig = class.unwrap().signature.as_ref().unwrap();
        assert!(sig.contains("Inherits"), "Signature should contain Inherits: {}", sig);
        assert!(sig.contains("Implements"), "Signature should contain Implements: {}", sig);
    }

    #[test]
    fn test_module_extraction() {
        let code = r#"
Public Module Utilities
    Public Sub DoWork()
    End Sub
End Module
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

        let module = symbols.iter().find(|s| s.name == "Utilities");
        assert!(module.is_some(), "Should find module Utilities");
        let module = module.unwrap();
        assert_eq!(module.kind, SymbolKind::Class);
        assert!(module.signature.as_ref().unwrap().contains("Module Utilities"));

        let metadata = module.metadata.as_ref().unwrap();
        assert_eq!(metadata.get("vb_module").unwrap(), &serde_json::Value::Bool(true));
    }

    #[test]
    fn test_structure_extraction() {
        let code = r#"
Public Structure Point
    Public X As Integer
    Public Y As Integer
End Structure
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

        let structure = symbols.iter().find(|s| s.name == "Point");
        assert!(structure.is_some(), "Should find structure Point");
        let structure = structure.unwrap();
        assert_eq!(structure.kind, SymbolKind::Struct);
        assert_eq!(structure.visibility, Some(Visibility::Public));
        assert!(structure.signature.as_ref().unwrap().contains("Structure Point"));
    }

    #[test]
    fn test_structure_with_implements() {
        let code = r#"
Structure Point
    Implements IEquatable(Of Point)

End Structure
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

        let structure = symbols.iter().find(|s| s.name == "Point");
        assert!(structure.is_some(), "Should find structure Point");
        let sig = structure.unwrap().signature.as_ref().unwrap();
        assert!(sig.contains("Implements"), "Signature should contain Implements: {}", sig);
    }

    #[test]
    fn test_interface_extraction() {
        let code = r#"
Public Interface IShape
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

        let iface = symbols.iter().find(|s| s.name == "IShape");
        assert!(iface.is_some(), "Should find interface IShape");
        let iface = iface.unwrap();
        assert_eq!(iface.kind, SymbolKind::Interface);
        assert_eq!(iface.visibility, Some(Visibility::Public));
        assert!(iface.signature.as_ref().unwrap().contains("Interface IShape"));
    }

    #[test]
    fn test_generic_interface_with_inherits() {
        let code = r#"
Public Interface IRepository(Of T)
    Inherits IEnumerable

    Function GetById(id As Integer) As T
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

        let iface = symbols.iter().find(|s| s.name == "IRepository");
        assert!(iface.is_some(), "Should find interface IRepository");
        let sig = iface.unwrap().signature.as_ref().unwrap();
        assert!(sig.contains("(Of T)"), "Signature should contain generic params: {}", sig);
        assert!(sig.contains("Inherits"), "Signature should contain Inherits: {}", sig);
    }

    #[test]
    fn test_enum_extraction() {
        let code = r#"
Public Enum Color
    Red
    Green
    Blue
End Enum
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

        let enm = symbols.iter().find(|s| s.name == "Color");
        assert!(enm.is_some(), "Should find enum Color");
        let enm = enm.unwrap();
        assert_eq!(enm.kind, SymbolKind::Enum);
        assert_eq!(enm.visibility, Some(Visibility::Public));
        assert!(enm.signature.as_ref().unwrap().contains("Enum Color"));

        let members: Vec<&Symbol> = symbols.iter()
            .filter(|s| s.kind == SymbolKind::EnumMember)
            .collect();
        assert_eq!(members.len(), 3, "Should have 3 enum members");
        assert!(members.iter().any(|s| s.name == "Red"));
        assert!(members.iter().any(|s| s.name == "Green"));
        assert!(members.iter().any(|s| s.name == "Blue"));
    }

    #[test]
    fn test_enum_with_values() {
        let code = r#"
Enum Level
    Low = 1
    Medium = 5
    High = 10
End Enum
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

        let low = symbols.iter().find(|s| s.name == "Low" && s.kind == SymbolKind::EnumMember);
        assert!(low.is_some(), "Should find enum member Low");
        let sig = low.unwrap().signature.as_ref().unwrap();
        assert!(sig.contains("= 1"), "Low signature should contain '= 1': {}", sig);

        let high = symbols.iter().find(|s| s.name == "High" && s.kind == SymbolKind::EnumMember);
        assert!(high.is_some(), "Should find enum member High");
        let sig = high.unwrap().signature.as_ref().unwrap();
        assert!(sig.contains("= 10"), "High signature should contain '= 10': {}", sig);
    }

    #[test]
    fn test_enum_with_underlying_type() {
        let code = r#"
Enum Color As Byte
    Red = 1
    Green = 2
    Blue = 3
End Enum
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

        let enm = symbols.iter().find(|s| s.name == "Color" && s.kind == SymbolKind::Enum);
        assert!(enm.is_some(), "Should find enum Color");
        let sig = enm.unwrap().signature.as_ref().unwrap();
        assert!(sig.contains("As Byte"), "Enum signature should contain underlying type: {}", sig);
    }

    #[test]
    fn test_delegate_sub_extraction() {
        let code = r#"
Delegate Sub EventHandler(sender As Object, e As EventArgs)
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

        let delegate = symbols.iter().find(|s| s.name == "EventHandler");
        assert!(delegate.is_some(), "Should find delegate EventHandler");
        let delegate = delegate.unwrap();
        assert_eq!(delegate.kind, SymbolKind::Delegate);
        let sig = delegate.signature.as_ref().unwrap();
        assert!(sig.contains("Delegate Sub EventHandler"), "Signature: {}", sig);
        assert!(sig.contains("sender As Object"), "Signature should have params: {}", sig);
    }

    #[test]
    fn test_delegate_function_extraction() {
        let code = r#"
Delegate Function Comparer(Of T)(a As T, b As T) As Integer
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

        let delegate = symbols.iter().find(|s| s.name == "Comparer");
        assert!(delegate.is_some(), "Should find delegate Comparer");
        let delegate = delegate.unwrap();
        assert_eq!(delegate.kind, SymbolKind::Delegate);
        let sig = delegate.signature.as_ref().unwrap();
        assert!(sig.contains("Delegate Function Comparer"), "Signature: {}", sig);
        assert!(sig.contains("(Of T)"), "Signature should have type params: {}", sig);
        assert!(sig.contains("As Integer"), "Signature should have return type: {}", sig);
    }

    #[test]
    fn test_class_parent_child_relationship() {
        let code = r#"
Namespace MyApp
    Public Class Outer
        Public Class Inner
        End Class
    End Class
End Namespace
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

        let ns = symbols.iter().find(|s| s.name == "MyApp" && s.kind == SymbolKind::Namespace);
        assert!(ns.is_some(), "Should find namespace MyApp");

        let outer = symbols.iter().find(|s| s.name == "Outer" && s.kind == SymbolKind::Class);
        assert!(outer.is_some(), "Should find class Outer");
        assert_eq!(outer.unwrap().parent_id.as_ref(), Some(&ns.unwrap().id));

        let inner = symbols.iter().find(|s| s.name == "Inner" && s.kind == SymbolKind::Class);
        assert!(inner.is_some(), "Should find class Inner");
        assert_eq!(inner.unwrap().parent_id.as_ref(), Some(&outer.unwrap().id));
    }
}
