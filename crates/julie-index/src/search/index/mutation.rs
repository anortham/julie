use std::sync::atomic::Ordering;

#[cfg(any(test, feature = "test-support"))]
use std::sync::mpsc::{Receiver, SyncSender};

use tantivy::schema::TantivyDocument;
use tantivy::{IndexWriter, Term};

#[cfg(any(test, feature = "test-support"))]
use super::RebuildPauseForTest;
use super::{SearchDocument, SearchIndex};
use crate::search::error::{Result, SearchError};

// 256MB total budget. Tantivy 0.26's `Index::writer(budget)` auto-clamps thread
// count when per-thread budget falls below the 15MB floor. At 50MB we got 3
// threads at ~16.67MB each; 256MB gives 8 threads at 32MB each — closer to the
// indexing throughput ceiling on multi-core boxes.
const WRITER_HEAP_SIZE: usize = 256_000_000;

impl SearchIndex {
    pub fn num_docs(&self) -> u64 {
        self.reader.reload().ok();
        self.reader.searcher().num_docs()
    }

    /// Add a unified `SearchDocument` to the index.
    ///
    /// Does NOT call `commit`; callers are responsible for batching.
    pub fn add_search_doc(&self, doc: &SearchDocument) -> Result<()> {
        let f = &self.schema_fields;
        let mut tantivy_doc = TantivyDocument::new();

        // ---- discriminator ----
        tantivy_doc.add_text(f.doc_type, &doc.doc_type);

        // ---- shared fields ----
        tantivy_doc.add_text(f.id, &doc.id);
        tantivy_doc.add_text(f.file_path, &doc.file_path);
        tantivy_doc.add_text(f.basename, &doc.basename);
        tantivy_doc.add_text(f.language, &doc.language);
        tantivy_doc.add_text(f.kind, &doc.kind);
        tantivy_doc.add_text(f.role, &doc.role);
        tantivy_doc.add_text(f.test_role, &doc.test_role);

        // ---- symbol fields ----
        tantivy_doc.add_text(f.name, &doc.name);
        tantivy_doc.add_text(f.signature, &doc.signature);
        tantivy_doc.add_text(f.doc_comment, &doc.doc_comment);
        tantivy_doc.add_text(f.code_body, &doc.code_body);
        for key in &doc.annotation_keys {
            let key = key.trim().to_ascii_lowercase();
            if !key.is_empty() {
                tantivy_doc.add_text(f.annotations_exact, &key);
            }
        }
        tantivy_doc.add_text(f.annotations_text, &doc.annotations_text);
        tantivy_doc.add_text(f.owner_names_text, &doc.owner_names_text);
        tantivy_doc.add_u64(f.start_line, doc.start_line as u64);

        // ---- file fields ----
        tantivy_doc.add_text(f.path_text, &doc.path_text);
        tantivy_doc.add_text(f.content, &doc.content);

        // ---- Phase 2 fields (empty in T2; wired by T4 / T7) ----
        tantivy_doc.add_text(f.pretokenized_code, &doc.pretokenized_code);
        tantivy_doc.add_text(f.relationship_text, &doc.relationship_text);

        let guard = self.get_or_create_writer()?;
        let writer = guard.as_ref().unwrap();
        writer.add_document(tantivy_doc)?;
        Ok(())
    }

    /// Commit pending changes to make them searchable.
    pub fn commit(&self) -> Result<()> {
        let mut guard = self.writer.lock().unwrap();
        if let Some(ref mut writer) = *guard {
            writer.commit()?;
        }
        self.reader.reload()?;
        Ok(())
    }

    /// Commit and release the current writer without shutting down search.
    ///
    /// This keeps the reader usable and lets future writes recreate the writer,
    /// while releasing Tantivy's process-wide write lock for path-backed callers.
    pub fn release_writer(&self) -> Result<()> {
        let mut guard = self.writer.lock().unwrap_or_else(|e| {
            tracing::warn!("writer mutex was poisoned during writer release; recovering");
            e.into_inner()
        });
        if let Some(mut writer) = guard.take() {
            writer.commit()?;
        }
        self.reader.reload()?;
        Ok(())
    }

    pub(crate) fn rollback_and_release_writer(&self) -> Result<()> {
        let mut guard = self.writer.lock().unwrap_or_else(|e| {
            tracing::warn!("writer mutex was poisoned during writer rollback; recovering");
            e.into_inner()
        });
        if let Some(mut writer) = guard.take() {
            writer.rollback()?;
        }
        self.reader.reload()?;
        Ok(())
    }

    pub(crate) fn clear_all_uncommitted(&self) -> Result<()> {
        let guard = self.get_or_create_writer()?;
        let writer = guard.as_ref().unwrap();
        writer.delete_all_documents()?;
        Ok(())
    }

    /// Remove all documents from the index (for force re-index).
    pub fn clear_all(&self) -> Result<()> {
        self.clear_all_uncommitted()?;
        self.commit()?;
        Ok(())
    }

    /// Remove all documents (both symbols and file content) for a given file path.
    pub fn remove_by_file_path(&self, path: &str) -> Result<()> {
        let term = Term::from_field_text(self.schema_fields.file_path, path);
        let guard = self.get_or_create_writer()?;
        let writer = guard.as_ref().unwrap();
        writer.delete_term(term);
        Ok(())
    }
    fn get_or_create_writer(&self) -> Result<std::sync::MutexGuard<'_, Option<IndexWriter>>> {
        if self.shutdown.load(Ordering::Acquire) {
            return Err(SearchError::Shutdown);
        }
        let mut guard = self.writer.lock().unwrap_or_else(|e| {
            tracing::warn!("writer mutex was poisoned (a previous writer panicked); recovering");
            e.into_inner()
        });
        if guard.is_none() {
            // Double-check after acquiring mutex: shutdown() may have run between
            // the flag check above and the mutex acquisition, dropping the writer.
            // Without this, a watcher task can re-create the writer after shutdown
            // released it, causing LockBusy for the next IndexWriter.
            if self.shutdown.load(Ordering::Acquire) {
                return Err(SearchError::Shutdown);
            }
            *guard = Some(self.index.writer(WRITER_HEAP_SIZE)?);
        }
        Ok(guard)
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn set_rebuild_pause_for_test(&self, cleared: SyncSender<()>, resume: Receiver<()>) {
        *self.rebuild_pause_for_test.lock().unwrap() =
            Some(RebuildPauseForTest { cleared, resume });
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn wait_after_rebuild_delete_for_test(&self) {
        let pause = self.rebuild_pause_for_test.lock().unwrap().take();
        if let Some(pause) = pause {
            pause.cleared.send(()).unwrap();
            pause.resume.recv().unwrap();
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn set_rebuild_failure_for_test(&self) {
        self.rebuild_failure_for_test.store(true, Ordering::Release);
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn take_rebuild_failure_for_test(&self) -> bool {
        self.rebuild_failure_for_test.swap(false, Ordering::AcqRel)
    }

    /// Hold the interior writer mutex for contention tests.
    ///
    /// Production code should not call this — projection and write paths
    /// already take the writer briefly via `get_or_create_writer`. Tests use
    /// this to simulate a long-held write without an outer `Mutex<SearchIndex>`.
    #[cfg(any(test, feature = "test-support"))]
    pub fn acquire_writer_for_test(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, Option<IndexWriter>>> {
        self.get_or_create_writer()
    }

    /// Gracefully shut down this index: commit pending writes, release the
    /// Tantivy file lock, and prevent any future writes.
    ///
    /// After shutdown, `get_or_create_writer()` returns `Err(Shutdown)`.
    /// Reads (search) continue to work — the `IndexReader` is independent.
    pub fn shutdown(&self) -> Result<()> {
        self.shutdown.store(true, Ordering::Release);

        let mut guard = self.writer.lock().unwrap_or_else(|e| {
            tracing::warn!("writer mutex was poisoned during shutdown; recovering");
            e.into_inner()
        });
        if let Some(mut writer) = guard.take() {
            // Best-effort commit — if it fails, we still drop the writer to release the lock
            let _ = writer.commit();
            // writer is dropped here, releasing the Tantivy file lock
        }
        Ok(())
    }

    /// Returns true if this index has been shut down.
    pub fn is_shutdown(&self) -> bool {
        self.shutdown.load(Ordering::Acquire)
    }
}
