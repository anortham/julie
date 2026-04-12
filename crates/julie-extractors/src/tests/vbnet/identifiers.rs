use super::*;
use crate::base::IdentifierKind;
use std::path::PathBuf;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_call_identifier_from_invocation() {
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
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        let call_id = identifiers.iter().find(|id| id.name == "DoWork");
        assert!(
            call_id.is_some(),
            "Should extract 'DoWork' call identifier"
        );
        assert_eq!(call_id.unwrap().kind, IdentifierKind::Call);
    }

    #[test]
    fn test_call_identifier_from_member_access_invocation() {
        let code = r#"
Public Class Service
    Public Sub Process()
        Helper.Format("test")
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
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        let format_call = identifiers
            .iter()
            .find(|id| id.name == "Format" && id.kind == IdentifierKind::Call);
        assert!(
            format_call.is_some(),
            "Should extract 'Format' call identifier from member access invocation"
        );
    }

    #[test]
    fn test_member_access_identifier_non_invocation() {
        let code = r#"
Public Class Service
    Public Sub Process()
        Dim x As Integer = Config.MaxRetries
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
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        let member_access = identifiers
            .iter()
            .find(|id| id.name == "MaxRetries" && id.kind == IdentifierKind::MemberAccess);
        assert!(
            member_access.is_some(),
            "Should extract 'MaxRetries' as MemberAccess identifier"
        );
    }

    #[test]
    fn test_call_identifier_has_containing_symbol() {
        let code = r#"
Public Class Calculator
    Public Function Compute(x As Integer) As Integer
        Return Transform(x)
    End Function

    Public Function Transform(value As Integer) As Integer
        Return value * 2
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
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        let transform_call = identifiers
            .iter()
            .find(|id| id.name == "Transform" && id.kind == IdentifierKind::Call);
        assert!(
            transform_call.is_some(),
            "Should extract 'Transform' call identifier"
        );
        assert!(
            transform_call.unwrap().containing_symbol_id.is_some(),
            "Call identifier should have a containing symbol"
        );
    }

    #[test]
    fn test_multiple_calls_in_method() {
        let code = r#"
Public Class Workflow
    Public Sub Run()
        Initialize()
        Process()
        Cleanup()
    End Sub

    Public Sub Initialize()
    End Sub

    Public Sub Process()
    End Sub

    Public Sub Cleanup()
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
        let identifiers = extractor.extract_identifiers(&tree, &symbols);

        let call_names: Vec<&str> = identifiers
            .iter()
            .filter(|id| id.kind == IdentifierKind::Call)
            .map(|id| id.name.as_str())
            .collect();

        assert!(
            call_names.contains(&"Initialize"),
            "Should find Initialize call"
        );
        assert!(
            call_names.contains(&"Process"),
            "Should find Process call"
        );
        assert!(
            call_names.contains(&"Cleanup"),
            "Should find Cleanup call"
        );
    }
}
