use std::time::Duration;

use tempfile::TempDir;

use crate::database::SymbolDatabase;
use crate::external_extract::{
    ExternalExtractOperationLock, external_extract_operation_lock_path,
    open_external_extract_database_for_operation, read_external_extract_info,
};

#[test]
fn extract_write_operations_serialize_per_db_lock() {
    let temp_dir = TempDir::new().expect("temp dir");
    let db_path = temp_dir.path().join("external.sqlite");
    let lock_path = external_extract_operation_lock_path(&db_path);

    let first = ExternalExtractOperationLock::acquire_with_timeout(&db_path, Duration::ZERO)
        .expect("first write operation lock acquire succeeds");

    let blocked = ExternalExtractOperationLock::acquire_with_timeout(&db_path, Duration::ZERO)
        .expect_err("second write operation cannot enter while first lock is held");

    assert_eq!(blocked.lock_path(), lock_path);
    assert_eq!(blocked.timeout(), Duration::ZERO);
    assert!(
        blocked
            .to_string()
            .contains(&lock_path.display().to_string()),
        "timeout error must include contended lock path: {blocked}"
    );
    assert!(
        blocked.to_string().contains("0ns"),
        "timeout error must include timeout: {blocked}"
    );

    drop(first);

    let reacquired = ExternalExtractOperationLock::acquire_with_timeout(&db_path, Duration::ZERO)
        .expect("write operation lock reacquires after previous holder drops");
    assert_eq!(reacquired.lock_path(), lock_path);
}

#[test]
fn external_extract_operation_lock_file_persists_after_release() {
    let temp_dir = TempDir::new().expect("temp dir");
    let db_path = temp_dir.path().join("external.sqlite");
    let lock_path = external_extract_operation_lock_path(&db_path);

    {
        let lock = ExternalExtractOperationLock::acquire_with_timeout(&db_path, Duration::ZERO)
            .expect("lock acquire succeeds");
        assert_eq!(lock.lock_path(), lock_path);
        assert!(lock_path.exists(), "lock file exists while held");
    }

    assert!(
        lock_path.exists(),
        "operation lock file must persist after release so future lockers contend on a stable inode"
    );
}

#[test]
fn extract_info_does_not_take_exclusive_write_lock() {
    let temp_dir = TempDir::new().expect("temp dir");
    let db_path = temp_dir.path().join("external.sqlite");

    {
        let db = SymbolDatabase::new(&db_path).expect("create external db");
        assert!(db.get_schema_version().unwrap() > 0);
    }

    let _write_lock = ExternalExtractOperationLock::acquire_with_timeout(&db_path, Duration::ZERO)
        .expect("hold write operation lock");

    let info = read_external_extract_info(&db_path)
        .expect("read-only info must not block behind external operation lock");

    assert_eq!(info.db, db_path);
    assert!(info.schema_version.is_some());
}

#[test]
fn external_extract_operation_opens_database_under_write_lock() {
    let temp_dir = TempDir::new().expect("temp dir");
    let db_path = temp_dir.path().join("external.sqlite");

    let operation = open_external_extract_database_for_operation(&db_path, false)
        .expect("operation opens database");

    assert!(operation.db().get_schema_version().unwrap() > 0);

    let blocked = ExternalExtractOperationLock::acquire_with_timeout(&db_path, Duration::ZERO)
        .expect_err("operation must hold write lock while db handle is open");
    assert_eq!(
        blocked.lock_path(),
        external_extract_operation_lock_path(&db_path)
    );

    drop(operation);

    ExternalExtractOperationLock::acquire_with_timeout(&db_path, Duration::ZERO)
        .expect("operation lock releases after operation drops");
}
