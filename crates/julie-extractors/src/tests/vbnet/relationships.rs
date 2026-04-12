use super::*;
use crate::base::RelationshipKind;
use std::path::PathBuf;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_class_inherits_base_class() {
        let code = r#"
Public Class Animal
    Public Sub Speak()
    End Sub
End Class

Public Class Dog
    Inherits Animal
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
        let relationships = extractor.extract_relationships(&tree, &symbols);

        let extends = relationships
            .iter()
            .find(|r| r.kind == RelationshipKind::Extends);
        assert!(extends.is_some(), "Should find Extends relationship");
        let extends = extends.unwrap();

        let dog = symbols.iter().find(|s| s.name == "Dog").unwrap();
        let animal = symbols.iter().find(|s| s.name == "Animal").unwrap();
        assert_eq!(extends.from_symbol_id, dog.id);
        assert_eq!(extends.to_symbol_id, animal.id);
        assert_eq!(extends.confidence, 1.0);
    }

    #[test]
    fn test_class_implements_interface() {
        let code = r#"
Public Interface IAnimal
    Sub Speak()
End Interface

Public Class Dog
    Implements IAnimal

    Public Sub Speak() Implements IAnimal.Speak
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
        let relationships = extractor.extract_relationships(&tree, &symbols);

        let implements = relationships
            .iter()
            .find(|r| r.kind == RelationshipKind::Implements);
        assert!(
            implements.is_some(),
            "Should find Implements relationship"
        );
        let implements = implements.unwrap();

        let dog = symbols.iter().find(|s| s.name == "Dog").unwrap();
        let ianimal = symbols.iter().find(|s| s.name == "IAnimal").unwrap();
        assert_eq!(implements.from_symbol_id, dog.id);
        assert_eq!(implements.to_symbol_id, ianimal.id);
        assert_eq!(implements.confidence, 1.0);
    }

    #[test]
    fn test_class_implements_multiple_interfaces() {
        let code = r#"
Public Interface IDisposable
    Sub Dispose()
End Interface

Public Interface ICloneable
    Function Clone() As Object
End Interface

Public Class Resource
    Implements IDisposable, ICloneable

    Public Sub Dispose() Implements IDisposable.Dispose
    End Sub

    Public Function Clone() As Object Implements ICloneable.Clone
        Return Nothing
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
        let relationships = extractor.extract_relationships(&tree, &symbols);

        let impl_rels: Vec<_> = relationships
            .iter()
            .filter(|r| r.kind == RelationshipKind::Implements)
            .collect();
        assert_eq!(
            impl_rels.len(),
            2,
            "Should have 2 Implements relationships, got {}",
            impl_rels.len()
        );

        let resource = symbols.iter().find(|s| s.name == "Resource").unwrap();
        for r in &impl_rels {
            assert_eq!(r.from_symbol_id, resource.id);
        }
    }

    #[test]
    fn test_interface_inherits_interface() {
        let code = r#"
Public Interface IEnumerable
    Function GetEnumerator() As Object
End Interface

Public Interface IRepository
    Inherits IEnumerable

    Function GetById(id As Integer) As Object
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
        let relationships = extractor.extract_relationships(&tree, &symbols);

        let extends = relationships
            .iter()
            .find(|r| r.kind == RelationshipKind::Extends);
        assert!(
            extends.is_some(),
            "Should find Extends for interface inheritance"
        );
        let extends = extends.unwrap();

        let repo = symbols.iter().find(|s| s.name == "IRepository").unwrap();
        let enumerable = symbols
            .iter()
            .find(|s| s.name == "IEnumerable")
            .unwrap();
        assert_eq!(extends.from_symbol_id, repo.id);
        assert_eq!(extends.to_symbol_id, enumerable.id);
    }

    #[test]
    fn test_call_relationship_same_class() {
        let code = r#"
Public Class Service
    Public Sub Process()
        DoWork()
    End Sub

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
        let relationships = extractor.extract_relationships(&tree, &symbols);

        let calls = relationships
            .iter()
            .find(|r| r.kind == RelationshipKind::Calls);
        assert!(calls.is_some(), "Should find Calls relationship");
        let calls = calls.unwrap();

        let process = symbols.iter().find(|s| s.name == "Process").unwrap();
        let do_work = symbols.iter().find(|s| s.name == "DoWork").unwrap();
        assert_eq!(calls.from_symbol_id, process.id);
        assert_eq!(calls.to_symbol_id, do_work.id);
    }

    #[test]
    fn test_pending_relationship_cross_file_type() {
        let code = r#"
Public Class Dog
    Inherits ExternalBase
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
        let relationships = extractor.extract_relationships(&tree, &symbols);

        assert!(
            relationships
                .iter()
                .all(|r| r.kind != RelationshipKind::Extends),
            "Should not have a resolved Extends relationship"
        );

        let pending = extractor.get_pending_relationships();
        let extends_pending = pending
            .iter()
            .find(|p| p.kind == RelationshipKind::Extends && p.callee_name == "ExternalBase");
        assert!(
            extends_pending.is_some(),
            "Should have a pending Extends for ExternalBase"
        );
        assert_eq!(extends_pending.unwrap().confidence, 0.9);
    }

    #[test]
    fn test_pending_relationship_unresolved_call() {
        let code = r#"
Public Class Service
    Public Sub Process()
        ExternalHelper()
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
        let relationships = extractor.extract_relationships(&tree, &symbols);

        assert!(
            relationships
                .iter()
                .all(|r| r.kind != RelationshipKind::Calls),
            "Should not have a resolved Calls relationship for unresolved method"
        );

        let pending = extractor.get_pending_relationships();
        let call_pending = pending
            .iter()
            .find(|p| p.kind == RelationshipKind::Calls && p.callee_name == "ExternalHelper");
        assert!(
            call_pending.is_some(),
            "Should have a pending Calls for ExternalHelper"
        );
        assert_eq!(call_pending.unwrap().confidence, 0.7);
    }

    #[test]
    fn test_structure_implements_interface() {
        let code = r#"
Public Interface IEquatable
    Function Equals(other As Object) As Boolean
End Interface

Public Structure Point
    Implements IEquatable

    Public X As Integer
    Public Y As Integer

    Public Function Equals(other As Object) As Boolean Implements IEquatable.Equals
        Return False
    End Function
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
        let relationships = extractor.extract_relationships(&tree, &symbols);

        let implements = relationships
            .iter()
            .find(|r| r.kind == RelationshipKind::Implements);
        assert!(
            implements.is_some(),
            "Structure should have Implements relationship"
        );
        let implements = implements.unwrap();

        let point = symbols.iter().find(|s| s.name == "Point").unwrap();
        let iequatable = symbols.iter().find(|s| s.name == "IEquatable").unwrap();
        assert_eq!(implements.from_symbol_id, point.id);
        assert_eq!(implements.to_symbol_id, iequatable.id);
    }
}
