use anyhow::Result;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

use crate::editing::{EditingTransaction, MultiFileTransaction};

/// Test helper for creating temporary files and directories
pub struct TransactionalTestFixture {
    temp_dir: TempDir,
}

impl TransactionalTestFixture {
    pub fn new() -> Result<Self> {
        Ok(Self {
            temp_dir: tempfile::tempdir()?,
        })
    }

    pub fn create_test_file(&self, name: &str, content: &str) -> Result<PathBuf> {
        let file_path = self.temp_dir.path().join(name);
        fs::write(&file_path, content)?;
        Ok(file_path)
    }

    pub fn get_temp_dir(&self) -> &std::path::Path {
        self.temp_dir.path()
    }

    /// Verify no .backup or .tmp files exist in directory
    pub fn verify_no_backup_files(&self) -> Result<()> {
        for entry in fs::read_dir(self.temp_dir.path())? {
            let entry = entry?;
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy();

            assert!(
                !name.ends_with(".backup") && !name.contains(".tmp."),
                "Found backup/temp file that should have been cleaned up: {}",
                name
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod transactional_editing_tests {
    use super::*;

    // ==========================================
    // RED Phase - These tests MUST fail first!
    // ==========================================

    #[tokio::test]
    async fn test_single_file_transaction_commit() -> Result<()> {
        println!("🧪 Testing single file transaction commit...");

        let fixture = TransactionalTestFixture::new()?;
        let file_path = fixture.create_test_file("test.txt", "original content")?;

        // Start transaction
        let transaction = EditingTransaction::begin(file_path.to_str().unwrap())?;

        // Modify and commit
        transaction.commit("modified content")?;

        // Verify file was updated
        let final_content = fs::read_to_string(&file_path)?;
        assert_eq!(final_content, "modified content");

        // Verify no backup files created
        fixture.verify_no_backup_files()?;

        println!("✅ Single file transaction commit test passed");
        Ok(())
    }

    #[tokio::test]
    async fn test_single_file_transaction_rejects_readonly_target() -> Result<()> {
        let fixture = TransactionalTestFixture::new()?;
        let file_path = fixture.create_test_file("readonly.txt", "original content")?;

        let mut perms = fs::metadata(&file_path)?.permissions();
        perms.set_readonly(true);
        fs::set_permissions(&file_path, perms)?;

        let transaction = EditingTransaction::begin(file_path.to_str().unwrap())?;
        let result = transaction.commit("modified content");

        let mut restore_perms = fs::metadata(&file_path)?.permissions();
        restore_perms.set_readonly(false);
        fs::set_permissions(&file_path, restore_perms)?;

        assert!(result.is_err(), "readonly commit should fail");
        assert_eq!(fs::read_to_string(&file_path)?, "original content");
        fixture.verify_no_backup_files()?;

        Ok(())
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_single_file_transaction_preserves_existing_permissions() -> Result<()> {
        use std::os::unix::fs::PermissionsExt;

        let fixture = TransactionalTestFixture::new()?;
        let file_path = fixture.create_test_file("tool.sh", "#!/bin/sh\n")?;
        fs::set_permissions(&file_path, fs::Permissions::from_mode(0o755))?;

        let transaction = EditingTransaction::begin(file_path.to_str().unwrap())?;
        transaction.commit("#!/bin/sh\necho ok\n")?;

        let mode = fs::metadata(&file_path)?.permissions().mode() & 0o777;
        assert_eq!(mode, 0o755, "commit should preserve executable mode");
        assert_eq!(fs::read_to_string(&file_path)?, "#!/bin/sh\necho ok\n");
        fixture.verify_no_backup_files()?;

        Ok(())
    }

    #[tokio::test]
    async fn test_single_file_transaction_rejects_changed_target_before_commit() -> Result<()> {
        let fixture = TransactionalTestFixture::new()?;
        let file_path = fixture.create_test_file("test.txt", "original content")?;

        let transaction = EditingTransaction::begin(file_path.to_str().unwrap())?;
        fs::write(&file_path, "external content")?;

        let result = transaction.commit_if_unchanged("agent content", "original content");

        assert!(result.is_err(), "changed target commit should fail");
        assert_eq!(fs::read_to_string(&file_path)?, "external content");
        fixture.verify_no_backup_files()?;

        Ok(())
    }

    #[test]
    fn test_single_file_commit_if_unchanged_serializes_julie_writers() -> Result<()> {
        use crate::editing::set_commit_after_expected_check_hook;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::mpsc;
        use std::sync::{Arc, Mutex};
        use std::thread;
        use std::time::Duration;

        let fixture = TransactionalTestFixture::new()?;
        let file_path = fixture.create_test_file("test.txt", "original content")?;
        let transaction1 = EditingTransaction::begin(file_path.to_str().unwrap())?;
        let transaction2 = EditingTransaction::begin(file_path.to_str().unwrap())?;
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let release_rx = Arc::new(Mutex::new(release_rx));
        let first_hook = Arc::new(AtomicBool::new(true));
        let hook_path = file_path.clone();

        set_commit_after_expected_check_hook(Some(Arc::new(move |path| {
            if path != hook_path || !first_hook.swap(false, Ordering::SeqCst) {
                return;
            }
            entered_tx.send(()).expect("notify commit hook entry");
            release_rx
                .lock()
                .expect("release receiver lock")
                .recv_timeout(Duration::from_secs(5))
                .expect("release commit hook");
        })));

        let writer1 = thread::spawn(move || {
            transaction1.commit_if_unchanged("from transaction 1", "original content")
        });
        entered_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("first writer should reach the checked commit window");

        let writer2 = thread::spawn(move || {
            transaction2.commit_if_unchanged("from transaction 2", "original content")
        });
        thread::sleep(Duration::from_millis(100));
        release_tx.send(()).expect("release first writer");

        let result1 = writer1.join().expect("writer 1 panicked");
        let result2 = writer2.join().expect("writer 2 panicked");
        set_commit_after_expected_check_hook(None);

        let successes = usize::from(result1.is_ok()) + usize::from(result2.is_ok());
        assert_eq!(
            successes, 1,
            "only one same-process checked commit may succeed; got result1={result1:?}, result2={result2:?}"
        );

        let final_content = fs::read_to_string(&file_path)?;
        if result1.is_ok() {
            assert_eq!(final_content, "from transaction 1");
        } else {
            assert_eq!(final_content, "from transaction 2");
        }
        fixture.verify_no_backup_files()?;

        Ok(())
    }

    #[test]
    fn test_multi_file_transaction_serializes_with_single_file_checked_writer() -> Result<()> {
        use crate::editing::{
            set_commit_after_expected_check_hook, set_multi_file_before_rename_hook,
        };
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::mpsc;
        use std::sync::{Arc, Mutex};
        use std::thread;
        use std::time::Duration;

        let fixture = TransactionalTestFixture::new()?;
        let file_path = fixture.create_test_file("test.txt", "original content")?;

        let mut multi = MultiFileTransaction::new("multi-race")?;
        multi.add_file(file_path.to_str().unwrap())?;
        multi.set_content(file_path.to_str().unwrap(), "from multi-file transaction")?;

        let single = EditingTransaction::begin(file_path.to_str().unwrap())?;

        let (multi_entered_tx, multi_entered_rx) = mpsc::channel();
        let (multi_release_tx, multi_release_rx) = mpsc::channel();
        let multi_release_rx = Arc::new(Mutex::new(multi_release_rx));
        let multi_hook_path = file_path.clone();
        let first_multi_hook = Arc::new(AtomicBool::new(true));
        set_multi_file_before_rename_hook(Some(Arc::new(move |path| {
            if path != multi_hook_path || !first_multi_hook.swap(false, Ordering::SeqCst) {
                return;
            }
            multi_entered_tx.send(()).expect("notify multi hook entry");
            multi_release_rx
                .lock()
                .expect("multi release receiver lock")
                .recv_timeout(Duration::from_secs(5))
                .expect("release multi-file transaction");
        })));

        let (single_entered_tx, single_entered_rx) = mpsc::channel();
        let (single_release_tx, single_release_rx) = mpsc::channel();
        let single_release_rx = Arc::new(Mutex::new(single_release_rx));
        let single_hook_path = file_path.clone();
        let first_single_hook = Arc::new(AtomicBool::new(true));
        set_commit_after_expected_check_hook(Some(Arc::new(move |path| {
            if path != single_hook_path || !first_single_hook.swap(false, Ordering::SeqCst) {
                return;
            }
            single_entered_tx
                .send(())
                .expect("notify single hook entry");
            single_release_rx
                .lock()
                .expect("single release receiver lock")
                .recv_timeout(Duration::from_secs(5))
                .expect("release single-file transaction");
        })));

        let multi_writer = thread::spawn(move || multi.commit_all());
        multi_entered_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("multi writer should reach commit point");

        let single_writer = thread::spawn(move || {
            single.commit_if_unchanged("from single-file transaction", "original content")
        });

        if single_entered_rx
            .recv_timeout(Duration::from_millis(200))
            .is_ok()
        {
            single_release_tx
                .send(())
                .expect("release single-file transaction");
            thread::sleep(Duration::from_millis(50));
        }
        multi_release_tx
            .send(())
            .expect("release multi-file transaction");

        let multi_result = multi_writer.join().expect("multi writer panicked");
        let single_result = single_writer.join().expect("single writer panicked");
        set_multi_file_before_rename_hook(None);
        set_commit_after_expected_check_hook(None);

        assert!(multi_result.is_ok(), "multi-file commit should succeed");
        assert!(
            single_result.is_err(),
            "checked single-file writer should reject stale content after multi-file commit; got {single_result:?}"
        );
        assert_eq!(
            fs::read_to_string(&file_path)?,
            "from multi-file transaction"
        );
        fixture.verify_no_backup_files()?;

        Ok(())
    }

    #[tokio::test]
    async fn test_single_file_transaction_rollback() -> Result<()> {
        println!("🧪 Testing single file transaction rollback...");

        let fixture = TransactionalTestFixture::new()?;
        let file_path = fixture.create_test_file("test.txt", "original content")?;

        // Start transaction
        let transaction = EditingTransaction::begin(file_path.to_str().unwrap())?;

        // Simulate some work that needs to be rolled back
        fs::write(&file_path, "corrupted content")?;

        // Rollback should restore original
        transaction.rollback()?;

        // Verify file was restored
        let final_content = fs::read_to_string(&file_path)?;
        assert_eq!(final_content, "original content");

        // Verify no backup files created
        fixture.verify_no_backup_files()?;

        println!("✅ Single file transaction rollback test passed");
        Ok(())
    }

    #[tokio::test]
    async fn test_multi_file_transaction_all_succeed() -> Result<()> {
        println!("🧪 Testing multi-file transaction all succeed...");

        let fixture = TransactionalTestFixture::new()?;
        let file1 = fixture.create_test_file("file1.txt", "content1")?;
        let file2 = fixture.create_test_file("file2.txt", "content2")?;
        let file3 = fixture.create_test_file("file3.txt", "content3")?;

        // Start multi-file transaction
        let mut transaction = MultiFileTransaction::new("test-session")?;
        transaction.add_file(file1.to_str().unwrap())?;
        transaction.add_file(file2.to_str().unwrap())?;
        transaction.add_file(file3.to_str().unwrap())?;

        // Apply changes to all files
        transaction.set_content(file1.to_str().unwrap(), "new content1")?;
        transaction.set_content(file2.to_str().unwrap(), "new content2")?;
        transaction.set_content(file3.to_str().unwrap(), "new content3")?;

        // Commit all changes atomically
        transaction.commit_all()?;

        // Verify all files updated
        assert_eq!(fs::read_to_string(&file1)?, "new content1");
        assert_eq!(fs::read_to_string(&file2)?, "new content2");
        assert_eq!(fs::read_to_string(&file3)?, "new content3");

        // Verify no backup files created
        fixture.verify_no_backup_files()?;

        println!("✅ Multi-file transaction all succeed test passed");
        Ok(())
    }

    #[tokio::test]
    async fn test_multi_file_transaction_failure_rollback() -> Result<()> {
        println!("🧪 Testing multi-file transaction failure rollback...");

        let fixture = TransactionalTestFixture::new()?;
        let file1 = fixture.create_test_file("file1.txt", "original1")?;
        let file2 = fixture.create_test_file("file2.txt", "original2")?;
        let readonly_file = fixture.create_test_file("readonly.txt", "readonly")?;

        // Make one file read-only to cause failure
        let mut perms = fs::metadata(&readonly_file)?.permissions();
        perms.set_readonly(true);
        fs::set_permissions(&readonly_file, perms)?;

        // Start multi-file transaction
        let mut transaction = MultiFileTransaction::new("test-session")?;
        transaction.add_file(file1.to_str().unwrap())?;
        transaction.add_file(file2.to_str().unwrap())?;
        transaction.add_file(readonly_file.to_str().unwrap())?;

        // Set new content for all files
        transaction.set_content(file1.to_str().unwrap(), "modified1")?;
        transaction.set_content(file2.to_str().unwrap(), "modified2")?;
        transaction.set_content(readonly_file.to_str().unwrap(), "modified_readonly")?;

        // Attempt to commit - should fail and rollback all
        let result = transaction.commit_all();
        assert!(
            result.is_err(),
            "Transaction should fail due to readonly file"
        );

        // Verify ALL files were rolled back (all-or-nothing)
        assert_eq!(fs::read_to_string(&file1)?, "original1");
        assert_eq!(fs::read_to_string(&file2)?, "original2");
        assert_eq!(fs::read_to_string(&readonly_file)?, "readonly");

        // Verify no backup files created
        fixture.verify_no_backup_files()?;

        println!("✅ Multi-file transaction failure rollback test passed");
        Ok(())
    }

    #[tokio::test]
    async fn test_atomic_write_no_partial_corruption() -> Result<()> {
        println!("🧪 Testing atomic write prevents partial corruption...");

        let fixture = TransactionalTestFixture::new()?;
        let file_path = fixture.create_test_file("test.txt", "original")?;

        // Start transaction
        let transaction = EditingTransaction::begin(file_path.to_str().unwrap())?;

        // Large content that would cause partial write if interrupted
        let large_content = "A".repeat(1_000_000); // 1MB

        // This should either fully succeed or fully fail (no partial writes)
        let result = transaction.commit(&large_content);

        if result.is_ok() {
            // If successful, content should be complete
            let final_content = fs::read_to_string(&file_path)?;
            assert_eq!(final_content.len(), 1_000_000);
            assert!(final_content.chars().all(|c| c == 'A'));
        } else {
            // If failed, original content should be preserved
            let final_content = fs::read_to_string(&file_path)?;
            assert_eq!(final_content, "original");
        }

        // Either way, no backup files should exist
        fixture.verify_no_backup_files()?;

        println!("✅ Atomic write test passed");
        Ok(())
    }

    #[tokio::test]
    async fn test_concurrent_transactions_safe() -> Result<()> {
        println!("🧪 Testing concurrent transactions are safe...");

        let fixture = TransactionalTestFixture::new()?;
        let file_path = fixture.create_test_file("test.txt", "original")?;

        // Start two transactions on the same file
        let transaction1 = EditingTransaction::begin(file_path.to_str().unwrap())?;
        let transaction2 = EditingTransaction::begin(file_path.to_str().unwrap())?;

        // First transaction commits
        transaction1.commit("from transaction 1")?;

        // Second transaction should either:
        // 1. Fail safely, or
        // 2. Succeed with proper conflict resolution
        let _result2 = transaction2.commit("from transaction 2");

        // Verify file is in a consistent state (not corrupted)
        let final_content = fs::read_to_string(&file_path)?;
        assert!(
            final_content == "from transaction 1" || final_content == "from transaction 2",
            "File content is corrupted: {}",
            final_content
        );

        // Verify no backup files created
        fixture.verify_no_backup_files()?;

        println!("✅ Concurrent transactions test passed");
        Ok(())
    }

    #[tokio::test]
    async fn test_emergency_cleanup_orphaned_temp_files() -> Result<()> {
        println!("🧪 Testing emergency cleanup of orphaned temp files...");

        let fixture = TransactionalTestFixture::new()?;

        // Simulate orphaned temp files from crashed transactions
        let orphan1 = fixture.get_temp_dir().join("test.txt.tmp.session1");
        let orphan2 = fixture.get_temp_dir().join("other.js.tmp.session2");
        fs::write(&orphan1, "orphaned content 1")?;
        fs::write(&orphan2, "orphaned content 2")?;

        // Run emergency cleanup
        EditingTransaction::emergency_cleanup(fixture.get_temp_dir())?;

        // Verify orphaned temp files were cleaned up
        assert!(
            !orphan1.exists(),
            "Orphaned temp file 1 should be cleaned up"
        );
        assert!(
            !orphan2.exists(),
            "Orphaned temp file 2 should be cleaned up"
        );

        println!("✅ Emergency cleanup test passed");
        Ok(())
    }

    #[tokio::test]
    async fn test_large_file_memory_efficiency() -> Result<()> {
        println!("🧪 Testing memory efficiency with large files...");

        let fixture = TransactionalTestFixture::new()?;

        // Create a large file (10MB)
        let large_content = "B".repeat(10_000_000);
        let file_path = fixture.create_test_file("large.txt", &large_content)?;

        // Transaction should handle large files efficiently
        let transaction = EditingTransaction::begin(file_path.to_str().unwrap())?;

        // Modify with different large content
        let new_large_content = "C".repeat(10_000_000);
        transaction.commit(&new_large_content)?;

        // Verify content was updated
        let final_content = fs::read_to_string(&file_path)?;
        assert_eq!(final_content.len(), 10_000_000);
        assert!(final_content.chars().all(|c| c == 'C'));

        // Verify no backup files created (important for large files)
        fixture.verify_no_backup_files()?;

        println!("✅ Large file memory efficiency test passed");
        Ok(())
    }

    /// Rollback must DELETE files that didn't exist before the transaction
    /// instead of writing an empty placeholder (the old behavior).
    ///
    /// This exercises rollback_partial_commit with a path whose pre-transaction
    /// content is None (not "").
    #[tokio::test]
    async fn test_rollback_deletes_new_file_not_truncates() -> Result<()> {
        let fixture = TransactionalTestFixture::new()?;

        // file_existing: was on disk before the transaction
        let file_existing = fixture.create_test_file("existing.txt", "original")?;

        // file_new: did NOT exist before the transaction
        let file_new = fixture.get_temp_dir().join("brand_new.txt");
        assert!(
            !file_new.exists(),
            "Precondition: brand_new.txt must not exist"
        );

        // Build the transaction so add_file records the pre-transaction state.
        let mut txn = MultiFileTransaction::new("test-rollback-delete")?;
        txn.add_file(file_existing.to_str().unwrap())?;
        txn.add_file(file_new.to_str().unwrap())?;
        txn.set_content(file_existing.to_str().unwrap(), "modified")?;
        txn.set_content(file_new.to_str().unwrap(), "brand new content")?;

        // Simulate Phase 2 having committed file_new (rename succeeded) but not yet
        // processed file_existing when the failure occurred.  We do this by physically
        // creating file_new on disk (as the rename would have) and then calling the
        // internal rollback directly via the test helper.
        fs::write(&file_new, "brand new content")?;
        assert!(
            file_new.exists(),
            "Manually created to simulate committed rename"
        );

        // Call rollback for the paths that were "committed" (just file_new here).
        txn.test_rollback_partial_commit(&[file_new.clone()])?;

        // After rollback:
        // - file_new MUST NOT EXIST (old code wrote "" here, creating an empty file)
        // - file_existing is untouched (it wasn't in committed_paths)
        assert!(
            !file_new.exists(),
            "Rollback must delete a file that didn't exist before the transaction (not truncate it)"
        );
        assert_eq!(
            fs::read_to_string(&file_existing)?,
            "original",
            "Existing file should be unchanged (it wasn't in committed_paths)"
        );

        Ok(())
    }

    /// Atomic rollback must use temp+rename, not plain write, when restoring existing files.
    /// The existing file content must be restored exactly, not corrupted on crash.
    #[tokio::test]
    async fn test_rollback_restores_existing_file_atomically() -> Result<()> {
        let fixture = TransactionalTestFixture::new()?;
        let file = fixture.create_test_file("restore_me.txt", "original content")?;

        let mut txn = MultiFileTransaction::new("test-rollback-atomic")?;
        txn.add_file(file.to_str().unwrap())?;
        txn.set_content(file.to_str().unwrap(), "modified content")?;

        // Simulate Phase 2 committing the file (write modified content as rename would)
        fs::write(&file, "modified content")?;

        // Rollback should restore to original
        txn.test_rollback_partial_commit(&[file.clone()])?;

        assert_eq!(
            fs::read_to_string(&file)?,
            "original content",
            "Rollback must restore existing file to its pre-transaction content"
        );
        // No leftover temp files
        fixture.verify_no_backup_files()?;

        Ok(())
    }
}
