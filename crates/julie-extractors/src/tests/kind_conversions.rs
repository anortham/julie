use crate::base::{IdentifierKind, RelationshipKind, SymbolKind, Visibility};

#[test]
fn test_core_kind_conversion_rejects_or_reports_unknown_values() {
    assert_eq!(
        SymbolKind::try_from_string("function"),
        Some(SymbolKind::Function)
    );
    assert_eq!(SymbolKind::try_from_string("totally_not_a_kind"), None);

    assert_eq!(
        RelationshipKind::try_from_string("calls"),
        Some(RelationshipKind::Calls)
    );
    assert_eq!(RelationshipKind::try_from_string("maybe_calls"), None);

    assert_eq!(
        IdentifierKind::try_from_string("member_access"),
        Some(IdentifierKind::MemberAccess)
    );
    assert_eq!(IdentifierKind::try_from_string("import"), None);

    assert_eq!(
        Visibility::from_storage_str("protected"),
        Some(Visibility::Protected)
    );
    assert_eq!(Visibility::from_storage_str("package_private"), None);
}

#[test]
fn test_relationship_kind_from_string_round_trips_composition() {
    assert_eq!(RelationshipKind::Composition.to_string(), "composition");
    assert_eq!(
        RelationshipKind::try_from_string("composition"),
        Some(RelationshipKind::Composition)
    );
    assert_eq!(
        RelationshipKind::from_string("composition"),
        RelationshipKind::Composition
    );
}
