// File Watcher & Incremental Indexing System
//
// This module provides real-time file monitoring and incremental updates
// to both data stores: SQLite database (with FTS5 search) and FastEmbed vectors

use anyhow::{Context, Result};
use hex;
use notify::{Event, EventKind, RecursiveMode, Watcher};
use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::SystemTime;
use tokio::sync::{mpsc, Mutex as TokioMutex};
use tracing::{debug, error, info, warn};

use crate::database::SymbolDatabase;
use crate::embeddings::EmbeddingEngine;
use crate::extractors::ExtractorManager;

// Import VectorStore type
use tokio::sync::RwLock;
type VectorIndex = crate::embeddings::vector_store::VectorStore;

/// Manages incremental indexing with real-time file watching
pub struct IncrementalIndexer {
    watcher: Option<notify::RecommendedWatcher>,
    db: Arc<StdMutex<SymbolDatabase>>,
    embedding_engine: Arc<RwLock<Option<EmbeddingEngine>>>,
    extractor_manager: Arc<ExtractorManager>,

    // Vector store for HNSW semantic search (kept in sync with incremental updates)
    #[allow(dead_code)]
    vector_store: Option<Arc<RwLock<VectorIndex>>>,

    // Processing queues
    pub(crate) index_queue: Arc<TokioMutex<VecDeque<FileChangeEvent>>>,

    // File filters
    supported_extensions: HashSet<String>,
    ignore_patterns: Vec<glob::Pattern>,

    // Configuration
    workspace_root: PathBuf,
}

/// Represents a file system change event
#[derive(Debug, Clone)]
pub struct FileChangeEvent {
    pub path: PathBuf,
    pub change_type: FileChangeType,
    pub timestamp: SystemTime,
}

/// Types of file system changes we track
#[derive(Debug, Clone)]
pub enum FileChangeType {
    Created,
    Modified,
    Deleted,
    Renamed { from: PathBuf, to: PathBuf },
}

/// Statistics for incremental indexing performance
#[derive(Debug, Clone)]
pub struct IndexingStats {
    pub files_processed: u64,
    pub symbols_added: u64,
    pub symbols_updated: u64,
    pub symbols_deleted: u64,
    pub processing_time_ms: u64,
}

impl IncrementalIndexer {
    /// Create a new incremental indexer for the given workspace
    pub fn new(
        workspace_root: PathBuf,
        db: Arc<StdMutex<SymbolDatabase>>,
        embedding_engine: Arc<RwLock<Option<EmbeddingEngine>>>,
        extractor_manager: Arc<ExtractorManager>,
        vector_store: Option<Arc<RwLock<VectorIndex>>>,
    ) -> Result<Self> {
        let supported_extensions = Self::build_supported_extensions();
        let ignore_patterns = Self::build_ignore_patterns()?;

        Ok(Self {
            watcher: None,
            db,
            embedding_engine,
            extractor_manager,
            vector_store,
            index_queue: Arc::new(TokioMutex::new(VecDeque::new())),
            supported_extensions,
            ignore_patterns,
            workspace_root,
        })
    }

    /// Start watching the workspace for file changes
    pub async fn start_watching(&mut self) -> Result<()> {
        info!(
            "Starting file watcher for workspace: {}",
            self.workspace_root.display()
        );

        let (tx, mut rx) = mpsc::unbounded_channel::<notify::Result<Event>>();

        // Create the watcher
        let mut watcher = notify::recommended_watcher(move |res| {
            if let Err(e) = tx.send(res) {
                error!("Failed to send file event: {}", e);
            }
        })?;

        // Start watching the workspace
        watcher
            .watch(&self.workspace_root, RecursiveMode::Recursive)
            .context("Failed to start watching workspace")?;

        self.watcher = Some(watcher);

        // Start the event processing task
        // Clone the necessary components that are Sync
        let supported_extensions = self.supported_extensions.clone();
        let ignore_patterns = self.ignore_patterns.clone();
        let index_queue = self.index_queue.clone();

        tokio::spawn(async move {
            info!("🔍 File system event detector started");
            while let Some(event_result) = rx.recv().await {
                match event_result {
                    Ok(event) => {
                        debug!("📁 File system event detected: {:?}", event);
                        if let Err(e) = Self::process_file_system_event_static(
                            &supported_extensions,
                            &ignore_patterns,
                            index_queue.clone(),
                            event,
                        )
                        .await
                        {
                            error!("Error processing file system event: {}", e);
                        }
                    }
                    Err(e) => {
                        warn!("File watcher error: {}", e);
                    }
                }
            }
        });

        // Spawn background task to process queued events
        // Clone all the components needed for processing
        let db = self.db.clone();
        let embeddings = self.embedding_engine.clone();
        let extractor_manager = self.extractor_manager.clone();
        let vector_store = self.vector_store.clone();
        let queue_for_processing = self.index_queue.clone();
        let workspace_root = self.workspace_root.clone();

        tokio::spawn(async move {
            use tokio::time::{interval, Duration};
            let mut tick = interval(Duration::from_secs(1)); // Process queue every second

            info!("🔄 Background queue processor started");
            loop {
                tick.tick().await;

                // Process all items currently in the queue
                let queue_size = {
                    let queue = queue_for_processing.lock().await;
                    queue.len()
                };

                if queue_size > 0 {
                    debug!("📦 Processing {} queued file events", queue_size);
                }

                while let Some(event) = {
                    let mut queue = queue_for_processing.lock().await;
                    queue.pop_front()
                } {
                    info!("🔄 Background task processing: {:?}", event.path);
                    if let Err(e) = Self::handle_file_change_static(
                        event,
                        &db,
                        &embeddings,
                        &extractor_manager,
                        vector_store.as_ref(),
                        &workspace_root,
                    )
                    .await
                    {
                        error!("Failed to handle file change: {}", e);
                    }
                }
            }
        });

        info!("File watcher started successfully with background queue processing");
        Ok(())
    }

    /// Process a file system event (static version for thread safety)
    async fn process_file_system_event_static(
        supported_extensions: &HashSet<String>,
        ignore_patterns: &[glob::Pattern],
        index_queue: Arc<TokioMutex<VecDeque<FileChangeEvent>>>,
        event: Event,
    ) -> Result<()> {
        debug!("Processing file system event: {:?}", event);

        match event.kind {
            EventKind::Create(_) => {
                for path in event.paths {
                    if Self::should_index_file_static(&path, supported_extensions, ignore_patterns)
                    {
                        let change_event = FileChangeEvent {
                            path: path.clone(),
                            change_type: FileChangeType::Created,
                            timestamp: SystemTime::now(),
                        };
                        Self::queue_file_change_static(index_queue.clone(), change_event).await;
                    }
                }
            }
            EventKind::Modify(_) => {
                for path in event.paths {
                    if Self::should_index_file_static(&path, supported_extensions, ignore_patterns)
                    {
                        let change_event = FileChangeEvent {
                            path: path.clone(),
                            change_type: FileChangeType::Modified,
                            timestamp: SystemTime::now(),
                        };
                        Self::queue_file_change_static(index_queue.clone(), change_event).await;
                    }
                }
            }
            EventKind::Remove(_) => {
                for path in event.paths {
                    // Check ignore patterns before processing deletion (was missing!)
                    if Self::should_index_file_static(&path, supported_extensions, ignore_patterns)
                    {
                        let change_event = FileChangeEvent {
                            path: path.clone(),
                            change_type: FileChangeType::Deleted,
                            timestamp: SystemTime::now(),
                        };
                        Self::queue_file_change_static(index_queue.clone(), change_event).await;
                    }
                }
            }
            _ => {
                // Handle other events like renames if needed
                debug!("Ignoring event kind: {:?}", event.kind);
            }
        }

        Ok(())
    }

    /// Static version of should_index_file for thread safety
    fn should_index_file_static(
        path: &Path,
        supported_extensions: &HashSet<String>,
        ignore_patterns: &[glob::Pattern],
    ) -> bool {
        // Check if it's a file
        if !path.is_file() {
            return false;
        }

        // Check extension
        if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
            if !supported_extensions.contains(ext) {
                return false;
            }
        } else {
            return false; // No extension
        }

        // Check ignore patterns
        let path_str = path.to_string_lossy();
        for pattern in ignore_patterns {
            if pattern.matches(&path_str) {
                return false;
            }
        }

        true
    }

    /// Static version of queue_file_change for thread safety
    async fn queue_file_change_static(
        index_queue: Arc<TokioMutex<VecDeque<FileChangeEvent>>>,
        event: FileChangeEvent,
    ) {
        debug!("Queueing file change: {:?}", event);

        let mut queue = index_queue.lock().await;
        queue.push_back(event);

        // Note: The actual processing logic needs to be handled differently
        // since we can't access self from a static context
        // This will be triggered by a separate background task
    }

    /// Queue a file change event for processing
    #[allow(dead_code)]
    async fn queue_file_change(&self, event: FileChangeEvent) {
        debug!("Queueing file change: {:?}", event);

        let mut queue = self.index_queue.lock().await;
        queue.push_back(event);

        // Start processing if queue wasn't empty
        if queue.len() == 1 {
            drop(queue);
            self.process_queue().await;
        }
    }

    /// Process the file change queue
    #[allow(dead_code)]
    async fn process_queue(&self) {
        while let Some(event) = {
            let mut queue = self.index_queue.lock().await;
            queue.pop_front()
        } {
            if let Err(e) = self.handle_file_change(event).await {
                error!("Failed to handle file change: {}", e);
            }
        }
    }

    /// Handle file change with explicit dependencies (for background task)
    async fn handle_file_change_static(
        event: FileChangeEvent,
        db: &Arc<StdMutex<SymbolDatabase>>,
        embeddings: &Arc<RwLock<Option<EmbeddingEngine>>>,
        extractor_manager: &Arc<ExtractorManager>,
        vector_store: Option<&Arc<RwLock<VectorIndex>>>,
        workspace_root: &Path,
    ) -> Result<()> {
        let start_time = std::time::Instant::now();

        match event.change_type {
            FileChangeType::Created | FileChangeType::Modified => {
                Self::handle_file_created_or_modified_static(
                    event.path,
                    db,
                    embeddings,
                    extractor_manager,
                    vector_store,
                    workspace_root,
                )
                .await?;
            }
            FileChangeType::Deleted => {
                Self::handle_file_deleted_static(event.path, db, vector_store).await?;
            }
            FileChangeType::Renamed { from, to } => {
                Self::handle_file_renamed_static(from, to, db, embeddings, extractor_manager, vector_store, workspace_root).await?;
            }
        }

        let processing_time = start_time.elapsed();
        debug!("File change processed in {:?}", processing_time);

        Ok(())
    }

    /// Handle a specific file change event
    async fn handle_file_change(&self, event: FileChangeEvent) -> Result<()> {
        let start_time = std::time::Instant::now();

        match event.change_type {
            FileChangeType::Created | FileChangeType::Modified => {
                self.handle_file_created_or_modified(event.path).await?;
            }
            FileChangeType::Deleted => {
                self.handle_file_deleted(event.path).await?;
            }
            FileChangeType::Renamed { from, to } => {
                self.handle_file_renamed(from, to).await?;
            }
        }

        let processing_time = start_time.elapsed();
        debug!("File change processed in {:?}", processing_time);

        Ok(())
    }

    /// Handle file creation or modification with Blake3 change detection
    async fn handle_file_created_or_modified(&self, path: PathBuf) -> Result<()> {
        info!("Processing file: {}", path.display());

        // 1. Read file content and calculate hash
        let content = tokio::fs::read(&path)
            .await
            .context("Failed to read file content")?;

        let new_hash = blake3::hash(&content);

        // 2. Check if file actually changed using Blake3
        let path_str = path.to_string_lossy();
        let db = self.db.lock().unwrap();
        if let Some(old_hash_str) = db.get_file_hash(&path_str)? {
            let new_hash_str = hex::encode(new_hash.as_bytes());
            if new_hash_str == old_hash_str {
                debug!(
                    "File {} unchanged (Blake3 hash match), skipping",
                    path.display()
                );
                return Ok(());
            }
        }
        drop(db);

        // 3. Detect language and extract symbols with enhanced error handling
        let language = self.detect_language(&path)?;
        let content_str = String::from_utf8_lossy(&content);

        // Enhanced extraction with graceful error handling
        let symbols = match self
            .extractor_manager
            .extract_symbols(&path_str, &content_str)
        {
            Ok(symbols) => symbols,
            Err(e) => {
                error!("❌ Symbol extraction failed for {}: {}", path_str, e);
                warn!("⚠️  SAFEGUARD: Preserving existing symbols due to extraction failure");
                return Ok(()); // Skip update to preserve existing data
            }
        };

        info!(
            "Extracted {} symbols from {} ({})",
            symbols.len(),
            path.display(),
            language
        );

        // 4. Update SQLite database (transactionally) with enhanced safeguards
        let mut db = self.db.lock().unwrap();

        // Get existing symbols for comparison
        let existing_symbols = db.get_symbols_for_file(&path_str)?;

        // ENHANCED SAFEGUARD: Multiple checks to prevent data loss
        if symbols.is_empty() && !existing_symbols.is_empty() {
            warn!("⚠️  SAFEGUARD: Refusing to delete {} existing symbols from {} - extraction returned zero symbols",
                  existing_symbols.len(), path_str);
            warn!("⚠️  Possible causes: parser error, file corruption, or unsupported language changes");
            warn!(
                "⚠️  File size: {} bytes, Language: {}",
                content.len(),
                language
            );
            return Ok(()); // Skip update to preserve existing data
        }

        // ADDITIONAL SAFEGUARD: Warn if significant symbol count drop (>50% reduction)
        if !existing_symbols.is_empty() && !symbols.is_empty() {
            let existing_count = existing_symbols.len();
            let new_count = symbols.len();
            if new_count < existing_count / 2 {
                warn!(
                    "⚠️  SIGNIFICANT SYMBOL REDUCTION: {} -> {} symbols in {}",
                    existing_count, new_count, path_str
                );
                warn!("⚠️  This may indicate partial parsing failure. Proceeding with update but flagging for review.");
            }
        }

        db.begin_transaction()?;

        // Ensure file record exists (required for foreign key constraint)
        let file_info = crate::database::create_file_info(&path, &language)?;
        if let Err(e) = db.store_file_info(&file_info) {
            db.rollback_transaction()?;
            return Err(e);
        }

        // Remove old symbols for this file (now safe - either we have new symbols or file never had symbols)
        db.delete_symbols_for_file(&path_str)?;

        // Insert new symbols (file watcher only operates on primary workspace)
        db.store_symbols(&symbols)?;

        // Update file hash (store as hex string)
        let new_hash_str = hex::encode(new_hash.as_bytes());
        db.update_file_hash(&path_str, &new_hash_str)?;

        db.commit_transaction()?;
        drop(db);

        // 5. Update embeddings using async RwLock
        let _symbol_ids: Vec<String> = symbols.iter().map(|s| s.id.clone()).collect();
        {
            let mut embedding_guard = self.embedding_engine.write().await;
            if let Some(ref mut embedding_engine) = embedding_guard.as_mut() {
                if let Err(e) = embedding_engine
                    .upsert_file_embeddings(path_str.as_ref(), &symbols)
                    .await
                {
                    warn!("Failed to update embeddings for {}: {}", path_str, e);
                } else {
                    debug!(
                        "Updated cached embeddings for {} symbol(s) in {}",
                        symbols.len(),
                        path_str
                    );
                }
            }
        } // Release embedding_engine lock

        // 🔧 REFACTOR: Removed incremental VectorStore updates
        // SQLite is now the single source of truth - HNSW rebuilt from SQLite on demand
        // File watcher only updates SQLite; semantic search lazy-loads from disk

        info!("Successfully updated SQLite indexes for {}", path.display());
        Ok(())
    }

    /// Handle file deletion
    async fn handle_file_deleted(&self, path: PathBuf) -> Result<()> {
        info!("Handling file deletion: {}", path.display());

        let path_str = path.to_string_lossy();

        // Get symbol IDs before deleting (needed for embedding cleanup)
        let symbol_ids: Vec<String> = {
            let db = self.db.lock().unwrap();
            db.get_symbols_for_file(&path_str)?
                .into_iter()
                .map(|s| s.id)
                .collect()
        };

        // Remove from SQLite database
        let db = self.db.lock().unwrap();
        db.delete_symbols_for_file(&path_str)?;
        db.delete_file_record(&path_str)?;
        drop(db);

        // Remove from embeddings (database will handle the actual deletion)
        if !symbol_ids.is_empty() {
            let mut embedding_guard = self.embedding_engine.write().await;
            if let Some(ref mut embedding_engine) = embedding_guard.as_mut() {
                if let Err(e) = embedding_engine
                    .remove_embeddings_for_symbols(&symbol_ids)
                    .await
                {
                    warn!("Failed to remove embeddings for {}: {}", path_str, e);
                }
            }
        }

        // 🔧 REFACTOR: Removed incremental VectorStore updates
        // SQLite is now the single source of truth - HNSW rebuilt from SQLite on demand
        // File watcher only updates SQLite; semantic search lazy-loads from disk

        info!("Successfully removed SQLite indexes for {}", path.display());
        Ok(())
    }

    /// Handle file rename
    async fn handle_file_renamed(&self, from: PathBuf, to: PathBuf) -> Result<()> {
        info!(
            "Handling file rename: {} -> {}",
            from.display(),
            to.display()
        );

        // This is equivalent to delete + create
        self.handle_file_deleted(from).await?;
        self.handle_file_created_or_modified(to).await?;

        Ok(())
    }

    /// Check if a file should be indexed based on extension and ignore patterns
    #[allow(dead_code)]
    fn should_index_file(&self, path: &Path) -> bool {
        // Check if it's a file
        if !path.is_file() {
            return false;
        }

        // Check extension
        if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
            if !self.supported_extensions.contains(ext) {
                return false;
            }
        } else {
            return false; // No extension
        }

        // Check ignore patterns
        let path_str = path.to_string_lossy();
        for pattern in &self.ignore_patterns {
            if pattern.matches(&path_str) {
                return false;
            }
        }

        true
    }

    /// Detect programming language from file extension
    fn detect_language(&self, path: &Path) -> Result<String> {
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow::anyhow!("No file extension"))?;

        let language = match ext {
            "rs" => "rust",
            "ts" | "tsx" => "typescript",
            "js" | "jsx" => "javascript",
            "py" => "python",
            "java" => "java",
            "cs" => "csharp",
            "cpp" | "cxx" | "cc" => "cpp",
            "c" | "h" => "c",
            "go" => "go",
            "php" => "php",
            "rb" => "ruby",
            "swift" => "swift",
            "kt" => "kotlin",
            "lua" => "lua",
            "gd" => "gdscript",
            "sql" => "sql",
            "html" | "htm" => "html",
            "css" => "css",
            "vue" => "vue",
            "razor" => "razor",
            "ps1" => "powershell",
            "sh" | "bash" => "bash",
            "zig" => "zig",
            "dart" => "dart",
            _ => return Err(anyhow::anyhow!("Unsupported file extension: {}", ext)),
        };

        Ok(language.to_string())
    }

    /// Detect language by file extension (static version for testing)
    pub fn detect_language_by_extension(path: &Path) -> Result<String> {
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow::anyhow!("No file extension"))?;

        let language = match ext {
            "rs" => "rust",
            "ts" | "tsx" => "typescript",
            "js" | "jsx" => "javascript",
            "py" => "python",
            "java" => "java",
            "cs" => "csharp",
            "cpp" | "cxx" | "cc" => "cpp",
            "c" | "h" => "c",
            "go" => "go",
            "php" => "php",
            "rb" => "ruby",
            "swift" => "swift",
            "kt" => "kotlin",
            "lua" => "lua",
            "gd" => "gdscript",
            "sql" => "sql",
            "html" | "htm" => "html",
            "css" => "css",
            "vue" => "vue",
            "razor" => "razor",
            "ps1" => "powershell",
            "sh" | "bash" => "bash",
            "zig" => "zig",
            "dart" => "dart",
            _ => return Err(anyhow::anyhow!("Unsupported file extension: {}", ext)),
        };

        Ok(language.to_string())
    }

    /// Build set of supported file extensions
    pub(crate) fn build_supported_extensions() -> HashSet<String> {
        [
            "rs", "ts", "tsx", "js", "jsx", "py", "java", "cs", "cpp", "cxx", "cc", "c", "h", "go",
            "php", "rb", "swift", "kt", "lua", "gd", "sql", "html", "htm", "css", "vue", "razor",
            "ps1", "sh", "bash", "zig", "dart",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    /// Build ignore patterns for files/directories to skip
    pub(crate) fn build_ignore_patterns() -> Result<Vec<glob::Pattern>> {
        let patterns = [
            "**/node_modules/**",
            "**/target/**",
            "**/build/**",
            "**/dist/**",
            "**/.git/**",
            "**/.julie/**", // Don't watch our own data directory
            "**/*.min.js",
            "**/*.bundle.js",
            "**/*.map",
            "**/coverage/**",
            "**/.nyc_output/**",
            "**/tmp/**",
            "**/temp/**",
            "**/__pycache__/**",
            "**/*.pyc",
            "**/vendor/**",
            "**/node_modules.nosync/**",
        ];

        patterns
            .iter()
            .map(|p| {
                glob::Pattern::new(p)
                    .map_err(|e| anyhow::anyhow!("Invalid glob pattern {}: {}", p, e))
            })
            .collect()
    }

    //  ═══════════════════════════════════════════════════════════════════════════
    //  STATIC HANDLER METHODS (for background task without &self)
    //  ═══════════════════════════════════════════════════════════════════════════

    /// Static version of handle_file_created_or_modified for background processing
    async fn handle_file_created_or_modified_static(
        path: PathBuf,
        db: &Arc<StdMutex<SymbolDatabase>>,
        embeddings: &Arc<RwLock<Option<EmbeddingEngine>>>,
        extractor_manager: &Arc<ExtractorManager>,
        vector_store: Option<&Arc<RwLock<VectorIndex>>>,
        workspace_root: &Path,
    ) -> Result<()> {
        info!("Processing file: {}", path.display());

        // 1. Read file content and calculate hash
        let content = tokio::fs::read(&path).await.context("Failed to read file content")?;
        let new_hash = blake3::hash(&content);

        // 2. Check if file actually changed using Blake3
        let path_str = path.to_string_lossy();
        {
            let db_lock = db.lock().unwrap();
            if let Some(old_hash_str) = db_lock.get_file_hash(&path_str)? {
                let new_hash_str = hex::encode(new_hash.as_bytes());
                if new_hash_str == old_hash_str {
                    debug!("File {} unchanged (Blake3 hash match), skipping", path.display());
                    return Ok(());
                }
            }
        }

        // 3. Detect language and extract symbols
        let language = Self::detect_language_static(&path, workspace_root)?;
        let content_str = String::from_utf8_lossy(&content);

        let symbols = match extractor_manager.extract_symbols(&path_str, &content_str) {
            Ok(symbols) => symbols,
            Err(e) => {
                error!("❌ Symbol extraction failed for {}: {}", path_str, e);
                return Ok(()); // Skip update to preserve existing data
            }
        };

        info!("Extracted {} symbols from {} ({})", symbols.len(), path.display(), language);

        // 4. Update SQLite database
        {
            let mut db_lock = db.lock().unwrap();
            let existing_symbols = db_lock.get_symbols_for_file(&path_str)?;

            // Safeguard against data loss
            if symbols.is_empty() && !existing_symbols.is_empty() {
                warn!("⚠️  SAFEGUARD: Refusing to delete {} existing symbols from {}", existing_symbols.len(), path_str);
                return Ok(());
            }

            // Use transaction for atomic updates
            db_lock.begin_transaction()?;

            // Ensure file record exists (required for foreign key constraint)
            let file_info = crate::database::create_file_info(&path, &language)?;
            if let Err(e) = db_lock.store_file_info(&file_info) {
                db_lock.rollback_transaction()?;
                return Err(e);
            }

            // Delete old symbols for this file
            db_lock.delete_symbols_for_file(&path_str)?;

            // Insert new symbols (within the transaction)
            db_lock.store_symbols(&symbols)?;

            // Update file hash
            let new_hash_str = hex::encode(new_hash.as_bytes());
            db_lock.update_file_hash(&path_str, &new_hash_str)?;

            db_lock.commit_transaction()?;
        }

        // 5. Generate embeddings asynchronously (non-blocking)
        // Spawn background task so file save completes immediately
        let embeddings_clone = embeddings.clone();
        let vector_store_clone = vector_store.cloned();
        let symbols_for_embedding = symbols.clone();
        let path_for_log = path.clone();

        tokio::spawn(async move {
            info!("🧠 Generating embeddings for {} symbols in {}", symbols_for_embedding.len(), path_for_log.display());

            // Step 1: Generate embeddings with GPU
            let mut embedding_guard = embeddings_clone.write().await;
            let embeddings_result = if let Some(ref mut engine) = embedding_guard.as_mut() {
                match engine.embed_symbols_batch(&symbols_for_embedding) {
                    Ok(embeddings_vec) => {
                        info!("✅ Generated {} embeddings for {}", embeddings_vec.len(), path_for_log.display());
                        Some(embeddings_vec)
                    }
                    Err(e) => {
                        warn!("⚠️ Failed to generate embeddings for {}: {}", path_for_log.display(), e);
                        None
                    }
                }
            } else {
                warn!("⏭️ Embedding engine not initialized, skipping embeddings for {}", path_for_log.display());
                None
            };
            drop(embedding_guard); // Release embedding engine lock

            // Step 2: Update HNSW index incrementally (if embeddings generated successfully)
            if let (Some(embeddings_vec), Some(vector_store_arc)) = (embeddings_result, vector_store_clone) {
                info!("📊 Updating HNSW index with {} new vectors", embeddings_vec.len());

                let mut vs_guard = vector_store_arc.write().await;
                match vs_guard.insert_batch(&embeddings_vec) {
                    Ok(_) => {
                        info!("✅ HNSW index updated with {} vectors for {}", embeddings_vec.len(), path_for_log.display());
                    }
                    Err(e) => {
                        warn!("⚠️ Failed to update HNSW index for {}: {}", path_for_log.display(), e);
                    }
                }
            }
        });

        info!("Successfully indexed {}", path.display());
        Ok(())
    }

    /// Static version of handle_file_deleted for background processing
    async fn handle_file_deleted_static(
        path: PathBuf,
        db: &Arc<StdMutex<SymbolDatabase>>,
        _vector_store: Option<&Arc<RwLock<VectorIndex>>>,
    ) -> Result<()> {
        info!("Processing file deletion: {}", path.display());

        let path_str = path.to_string_lossy();
        let db_lock = db.lock().unwrap();

        // Handle transient DELETE events gracefully (e.g., editor save operations)
        // Editors often delete-then-recreate files, causing DELETE events before the file
        // was ever indexed. "no such table" errors are harmless in this case.
        match db_lock.delete_symbols_for_file(&path_str) {
            Ok(_) => {},
            Err(e) => {
                let err_msg = e.to_string();
                if err_msg.contains("no such table") {
                    // Transient state - file was never indexed, nothing to delete
                    info!("Skipping deletion for {} (not yet indexed)", path.display());
                    return Ok(());
                } else {
                    // Real error - propagate it
                    return Err(e);
                }
            }
        }

        match db_lock.delete_file_record(&path_str) {
            Ok(_) => {},
            Err(e) => {
                let err_msg = e.to_string();
                if err_msg.contains("no such table") {
                    // Transient state - file record never existed
                    info!("Skipping file record deletion for {} (not yet indexed)", path.display());
                    return Ok(());
                } else {
                    // Real error - propagate it
                    return Err(e);
                }
            }
        }

        info!("Successfully removed indexes for {}", path.display());
        Ok(())
    }

    /// Static version of handle_file_renamed for background processing
    async fn handle_file_renamed_static(
        from: PathBuf,
        to: PathBuf,
        db: &Arc<StdMutex<SymbolDatabase>>,
        embeddings: &Arc<RwLock<Option<EmbeddingEngine>>>,
        extractor_manager: &Arc<ExtractorManager>,
        vector_store: Option<&Arc<RwLock<VectorIndex>>>,
        workspace_root: &Path,
    ) -> Result<()> {
        info!("Handling file rename: {} -> {}", from.display(), to.display());

        // Delete + create
        Self::handle_file_deleted_static(from, db, vector_store).await?;
        Self::handle_file_created_or_modified_static(to, db, embeddings, extractor_manager, vector_store, workspace_root).await?;

        Ok(())
    }

    /// Static language detection helper
    fn detect_language_static(path: &Path, _workspace_root: &Path) -> Result<String> {
        Self::detect_language_by_extension(path)
    }

    /// Process any pending file changes from the queue
    pub async fn process_pending_changes(&self) -> Result<()> {
        // Process all items currently in the queue
        while let Some(event) = {
            let mut queue = self.index_queue.lock().await;
            queue.pop_front()
        } {
            if let Err(e) = self.handle_file_change(event).await {
                error!("Failed to handle file change: {}", e);
            }
        }
        Ok(())
    }

    /// Stop the file watcher
    pub async fn stop(&mut self) -> Result<()> {
        if let Some(watcher) = self.watcher.take() {
            drop(watcher);
            info!("File watcher stopped");
        }
        Ok(())
    }
}

// Tests moved to `src/tests/watcher_tests.rs`
