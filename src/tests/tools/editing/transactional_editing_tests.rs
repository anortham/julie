use anyhow::Result;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

use crate::tools::editing::{EditingTransaction, MultiFileTransaction};

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
        assert!(result.is_err(), "Transaction should fail due to readonly file");

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
        assert!(!orphan1.exists(), "Orphaned temp file 1 should be cleaned up");
        assert!(!orphan2.exists(), "Orphaned temp file 2 should be cleaned up");

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
}