use crate::base::IdentifierKind;

#[test]
fn test_identifier_kind_round_trips_supported_values() {
    assert_eq!(IdentifierKind::from_string("call"), IdentifierKind::Call);
    assert_eq!(
        IdentifierKind::from_string("variable_ref"),
        IdentifierKind::VariableRef
    );
    assert_eq!(
        IdentifierKind::from_string("type_usage"),
        IdentifierKind::TypeUsage
    );
    assert_eq!(
        IdentifierKind::from_string("member_access"),
        IdentifierKind::MemberAccess
    );
}

#[test]
fn test_identifier_kind_import_falls_back_to_variable_ref() {
    assert_eq!(
        IdentifierKind::from_string("import"),
        IdentifierKind::VariableRef
    );
}
