use tempfile::tempdir;

use crate::database::{LATEST_SCHEMA_VERSION, SymbolDatabase};
use crate::test_support::{file_info_builder, identifier_builder};

#[test]
fn receiver_type_column_is_present_after_open_and_reopen() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("symbols.db");
    for _ in 0..2 {
        let db = SymbolDatabase::new(&path).unwrap();
        assert!(db.has_column("identifiers", "receiver_type").unwrap());
    }
}

#[test]
fn fresh_database_creation_initializes_schema_with_receiver_type() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("fresh.db");
    let db = SymbolDatabase::new(&path).unwrap();
    assert_eq!(db.get_schema_version().unwrap(), LATEST_SCHEMA_VERSION);
    assert_eq!(LATEST_SCHEMA_VERSION, 32);
    assert!(db.has_column("identifiers", "receiver_type").unwrap());
}

#[test]
fn bulk_identifier_insertion_preserves_some_and_none_receiver_types() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("bulk_identifiers.db");
    let mut db = SymbolDatabase::new(&path).unwrap();

    let file = file_info_builder("src/service.rs").build();
    db.store_file_info(&file).unwrap();

    let id_with_type = identifier_builder("id-typed", "execute", "src/service.rs")
        .receiver_type("OrderService")
        .build();
    let id_without_type = identifier_builder("id-untyped", "execute", "src/service.rs").build();

    db.bulk_store_identifiers(&[id_with_type, id_without_type], "primary")
        .unwrap();

    let refs = db
        .get_identifiers_by_names(&["execute".to_string()])
        .unwrap();
    assert_eq!(refs.len(), 2);

    let typed_ref = refs.iter().find(|r| r.receiver_type.is_some()).unwrap();
    assert_eq!(typed_ref.receiver_type.as_deref(), Some("OrderService"));

    let untyped_ref = refs.iter().find(|r| r.receiver_type.is_none()).unwrap();
    assert_eq!(untyped_ref.receiver_type, None);
}
