//! File processing for indexing
//! Handles reading, parsing, and extracting symbols from individual files

use crate::extractors::{Relationship, Symbol};
use crate::handler::JulieServerHandler;
use crate::tools::workspace::LanguageParserPool;
use crate::tools::workspace::commands::ManageWorkspaceTool;
use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, info, trace, warn};
use tree_sitter::Parser;

impl ManageWorkspaceTool {
    /// SQLite-only file processing with optimized parser reuse
    ///
    /// Uses SQLite FTS5 for full-text search indexing.
    pub(crate) async fn process_files_optimized(
        &self,
        handler: &JulieServerHandler,
        files_to_index: Vec<PathBuf>,
        is_primary_workspace: bool,
        total_files: &mut usize,
        workspace_id: String,  // Pass workspace_id instead of re-looking it up
        workspace_path: &Path, // Path of workspace being indexed (primary OR reference)
    ) -> Result<()> {
        // Group files by language for batch processing
        let mut files_by_language: HashMap<String, Vec<PathBuf>> = HashMap::new();

        for file_path in files_to_index {
            let language = self.detect_language(&file_path);

            files_by_language
                .entry(language)
                .or_default()
                .push(file_path);
        }

        // Create parser pool for maximum performance
        let mut parser_pool = LanguageParserPool::new();

        info!(
            "🚀 Processing {} languages with optimized parser reuse",
            files_by_language.len()
        );

        // Phase 2: Use workspace_path for relative path storage (works for primary AND reference workspaces)
        let workspace_root = workspace_path;

        // 🔥 CRITICAL FIX: Open correct database for reference vs primary workspaces
        // Reference workspaces need their own separate database at indexes/{workspace_id}/db/symbols.db
        // Primary workspace uses the existing handler.get_workspace().db connection
        let ref_workspace_db = if !is_primary_workspace {
            // This is a REFERENCE workspace - open its separate database
            let primary_workspace = handler
                .get_workspace()
                .await?
                .ok_or_else(|| anyhow::anyhow!("No workspace initialized"))?;

            let ref_db_path = primary_workspace.workspace_db_path(&workspace_id);
            debug!(
                "🗄️ Opening reference workspace DB: {}",
                ref_db_path.display()
            );

            // Create the db/ directory if it doesn't exist
            if let Some(parent_dir) = ref_db_path.parent() {
                std::fs::create_dir_all(parent_dir).map_err(|e| {
                    anyhow::anyhow!(
                        "Failed to create database directory {}: {}",
                        parent_dir.display(),
                        e
                    )
                })?;
                debug!("📁 Created database directory: {}", parent_dir.display());
            }

            // 🚨 CRITICAL: Wrap blocking file I/O in spawn_blocking
            let ref_db_path_clone = ref_db_path.clone();
            let db = tokio::task::spawn_blocking(move || {
                crate::database::SymbolDatabase::new(ref_db_path_clone)
            })
            .await
            .map_err(|e| anyhow::anyhow!("Failed to spawn database open task: {}", e))??;

            Some(Arc::new(std::sync::Mutex::new(db)))
        } else {
            // Primary workspace - will use handler.get_workspace().db (existing connection)
            None
        };

        // 🔥 COLLECT ALL DATA FIRST for bulk operations
        let mut all_symbols = Vec::new();
        let mut all_relationships = Vec::new();
        let mut all_identifiers = Vec::new();
        let mut all_types = Vec::new();
        let mut all_file_infos = Vec::new();
        let mut files_to_clean = Vec::new(); // Track files that need cleanup before re-indexing

        // Process each language group with its dedicated parser
        for (language, file_paths) in files_by_language {
            if file_paths.is_empty() {
                continue;
            }

            debug!(
                "Processing {} {} files with reused parser",
                file_paths.len(),
                language
            );

            // Try to get a parser for this language
            match parser_pool.get_parser(&language) {
                Ok(parser) => {
                    // Has parser: full symbol extraction + text indexing for all files
                    for file_path in file_paths {
                        match self
                            .process_file_with_parser(
                                &file_path,
                                &language,
                                parser,
                                &workspace_root,
                            )
                            .await
                        {
                            Ok((symbols, relationships, identifiers, types, file_info)) => {
                                *total_files += 1;

                                // Per-file processing details at trace level
                                trace!(
                                    "File {} extracted {} symbols",
                                    file_path.display(),
                                    symbols.len()
                                );

                                // Track this file for cleanup (remove old symbols/data before adding new)
                                // MUST use relative path to match how symbols are stored in database
                                let relative_path = if file_path.is_absolute() {
                                    crate::utils::paths::to_relative_unix_style(
                                        &file_path,
                                        &workspace_root,
                                    )
                                    .unwrap_or_else(|_| file_path.to_string_lossy().to_string())
                                } else {
                                    // Already relative - use as-is (just normalize to Unix-style)
                                    file_path.to_string_lossy().replace('\\', "/")
                                };
                                files_to_clean.push(relative_path);

                                // Collect data for bulk storage
                                all_symbols.extend(symbols);
                                all_relationships.extend(relationships);
                                all_identifiers.extend(identifiers);
                                all_types.extend(types.into_iter().map(|(_, v)| v));
                                all_file_infos.push(file_info);

                                if (*total_files).is_multiple_of(50) {
                                    debug!(
                                        "Progress: {} files processed, {} symbols collected",
                                        total_files,
                                        all_symbols.len()
                                    );
                                }
                            }
                            Err(e) => {
                                warn!("Failed to process file {:?}: {}", file_path, e);
                            }
                        }
                    }
                }
                Err(e) => {
                    // No parser: index files for text search only (no symbol extraction)
                    debug!(
                        "No parser for {} ({}) - indexing {} files for text search only",
                        language,
                        e,
                        file_paths.len()
                    );
                    for file_path in file_paths {
                        match self
                            .process_file_without_parser(&file_path, &language, &workspace_root)
                            .await
                        {
                            Ok((symbols, relationships, file_info)) => {
                                debug!("📄 Processed file without parser: {:?}", file_path);
                                *total_files += 1;
                                // MUST use relative path to match how symbols are stored in database
                                let relative_path = if file_path.is_absolute() {
                                    crate::utils::paths::to_relative_unix_style(
                                        &file_path,
                                        &workspace_root,
                                    )
                                    .unwrap_or_else(|_| file_path.to_string_lossy().to_string())
                                } else {
                                    // Already relative - use as-is (just normalize to Unix-style)
                                    file_path.to_string_lossy().replace('\\', "/")
                                };
                                files_to_clean.push(relative_path);
                                all_symbols.extend(symbols); // Will be empty
                                all_relationships.extend(relationships); // Will be empty
                                all_file_infos.push(file_info);
                            }
                            Err(e) => {
                                warn!(
                                    "Failed to process file without parser {:?}: {}",
                                    file_path, e
                                );
                            }
                        }
                    }
                }
            }
        }

        // Get database handle
        let db_to_use = if let Some(ref ref_db) = ref_workspace_db {
            Some(ref_db.clone())
        } else {
            if let Some(workspace) = handler.get_workspace().await? {
                workspace.db.clone()
            } else {
                None
            }
        };

        if let Some(db) = db_to_use {
            let bulk_start = std::time::Instant::now();

            // 🔥 ATOMIC INCREMENTAL UPDATE: Use new method that wraps cleanup + insert in ONE transaction
            // This prevents the critical corruption window where cleanup commits but insert never happens
            if !files_to_clean.is_empty() {
                info!(
                    "🔐 Starting ATOMIC incremental update: {} files to clean, {} symbols, {} relationships, {} files",
                    files_to_clean.len(),
                    all_symbols.len(),
                    all_relationships.len(),
                    all_file_infos.len()
                );

                let mut db_lock = match db.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => {
                        warn!(
                            "Database mutex poisoned during atomic incremental update, recovering: {}",
                            poisoned
                        );
                        poisoned.into_inner()
                    }
                };

                if let Err(e) = db_lock.incremental_update_atomic(
                    &files_to_clean,
                    &all_file_infos,
                    &all_symbols,
                    &all_relationships,
                    &all_identifiers,
                    &all_types,
                    &workspace_id,
                ) {
                    warn!("Failed to perform atomic incremental update: {}", e);
                    return Err(e);
                }

                // Count documentation symbols for logging
                let doc_count = all_symbols
                    .iter()
                    .filter(|s| s.language == "markdown")
                    .count();

                if doc_count > 0 {
                    debug!(
                        "📚 Stored {} documentation symbols in symbols table",
                        doc_count
                    );
                }

                drop(db_lock);
            } else {
                // Fresh indexing (no files to clean) - use standard bulk operations
                // Each bulk operation is already atomic from our previous fixes
                info!(
                    "🚀 Starting fresh bulk storage of {} symbols, {} relationships, {} files...",
                    all_symbols.len(),
                    all_relationships.len(),
                    all_file_infos.len()
                );

                let mut db_lock = match db.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => {
                        warn!(
                            "Database mutex poisoned during fresh bulk storage, recovering: {}",
                            poisoned
                        );
                        poisoned.into_inner()
                    }
                };

                // Bulk store files
                if let Err(e) = db_lock.bulk_store_files(&all_file_infos) {
                    warn!("Failed to bulk store files: {}", e);
                }

                // Bulk store symbols (with index dropping optimization!)
                if let Err(e) = db_lock.bulk_store_symbols(&all_symbols, &workspace_id) {
                    warn!("Failed to bulk store symbols: {}", e);
                }

                // Bulk store relationships
                if let Err(e) = db_lock.bulk_store_relationships(&all_relationships) {
                    warn!("Failed to bulk store relationships: {}", e);
                }

                // Phase 4: Bulk store identifiers
                if let Err(e) = db_lock.bulk_store_identifiers(&all_identifiers, &workspace_id) {
                    warn!("Failed to bulk store identifiers: {}", e);
                }

                // Phase 4: Bulk store types
                if let Err(e) = db_lock.bulk_store_types(&all_types, &workspace_id) {
                    warn!("Failed to bulk store types: {}", e);
                }

                // Count documentation symbols for logging
                let doc_count = all_symbols
                    .iter()
                    .filter(|s| s.language == "markdown")
                    .count();

                if doc_count > 0 {
                    debug!(
                        "📚 Stored {} documentation symbols in symbols table",
                        doc_count
                    );
                }

                drop(db_lock);
            }

            let bulk_duration = bulk_start.elapsed();
            info!(
                "✅ Bulk storage complete in {:.2}s - data now persisted in SQLite!",
                bulk_duration.as_secs_f64()
            );

            // Mark SQLite FTS5 as ready
            handler
                .indexing_status
                .sqlite_fts_ready
                .store(true, std::sync::atomic::Ordering::Release);
            debug!("🔍 SQLite FTS5 search now available");
        }

        Ok(())
    }

    /// Process a single file with symbol extraction
    ///
    /// Returns (symbols, relationships, file_info) for bulk storage.
    ///
    /// # Phase 2: Relative Unix-Style Path Storage
    /// Now requires workspace_root for relative path storage in extractors
    pub(crate) async fn process_file_with_parser(
        &self,
        file_path: &Path,
        language: &str,
        _parser: &mut Parser, // Unused: Creating new parser inside spawn_blocking for Send requirement
        workspace_root: &Path, // NEW: Phase 2 - workspace root for relative paths
    ) -> Result<(
        Vec<Symbol>,
        Vec<Relationship>,
        Vec<crate::extractors::Identifier>,
        HashMap<String, crate::extractors::base::TypeInfo>,
        crate::database::FileInfo,
    )> {
        // 🚨 CRITICAL FIX: Wrap ALL blocking filesystem I/O in spawn_blocking to prevent tokio deadlock
        // When processing hundreds of large files (500KB+), blocking I/O in async functions
        // starves the tokio runtime and causes silent hangs (discovered in PsychiatricIntake workspace)
        let file_path_clone = file_path.to_path_buf();
        let language_clone = language.to_string();
        let workspace_root_clone = workspace_root.to_path_buf();

        let (_canonical_file_path, content, file_info) = tokio::task::spawn_blocking(move || {
            // 🔍 DEBUG: Log that we're inside spawn_blocking
            tracing::trace!("🔄 Inside spawn_blocking for: {:?}", file_path_clone);
            // Blocking operation 1: canonicalize (resolves symlinks: macOS /var -> /private/var)
            tracing::trace!("🔧 Canonicalizing path...");
            let canonical = file_path_clone
                .canonicalize()
                .unwrap_or_else(|_| file_path_clone.clone());
            tracing::trace!("✅ Canonicalized: {:?}", canonical);

            // Blocking operation 2: read file content
            tracing::trace!("📖 Reading file content...");
            let file_content = std::fs::read_to_string(&canonical)
                .map_err(|e| anyhow::anyhow!("Failed to read file {:?}: {}", canonical, e))?;
            tracing::trace!("✅ Read {} bytes", file_content.len());

            // Blocking operation 3: create file info (does metadata, hash, etc)
            // This also reads the file, but we do it here to keep ALL blocking I/O in one place
            tracing::trace!("📊 Creating file info...");
            let info = crate::database::create_file_info(
                &file_path_clone,
                &language_clone,
                &workspace_root_clone,
            )?;
            tracing::trace!("✅ File info created");

            Ok::<_, anyhow::Error>((canonical, file_content, info))
        })
        .await
        .map_err(|e| anyhow::anyhow!("Failed to spawn blocking file I/O task: {}", e))??;

        tracing::trace!("✅ spawn_blocking completed for: {:?}", file_path);

        // Skip empty files for symbol extraction
        if content.trim().is_empty() {
            // Return empty symbol list but include file_info (already created in spawn_blocking)
            return Ok((
                Vec::new(),
                Vec::new(),
                Vec::new(),
                HashMap::new(),
                file_info,
            ));
        }

        // Skip symbol extraction for CSS/HTML (text search only)
        if !self.should_extract_symbols(language) {
            debug!(
                "⏭️  Skipping symbol extraction for {} file (text search only): {}",
                language,
                file_path.display()
            );

            // Return file info without symbols (file_info already created in spawn_blocking)
            return Ok((
                Vec::new(),
                Vec::new(),
                Vec::new(),
                HashMap::new(),
                file_info,
            ));
        }

        // 🚨 CRITICAL: Skip symbol extraction for very large files (likely data/minified)
        // These files cause exponential CPU usage in tree-sitter traversal (demo-data.js: 158KB = hang)
        // Note: Legitimate Rust files with good docs can be 100-200KB (e.g., candle-core/src/tensor.rs = 112KB)
        const MAX_FILE_SIZE_FOR_SYMBOLS: usize = 500_000; // 500KB limit
        if content.len() > MAX_FILE_SIZE_FOR_SYMBOLS {
            warn!(
                "⏭️  Skipping symbol extraction for large file ({} bytes > {}KB limit): {} - indexing for text search only",
                content.len(),
                MAX_FILE_SIZE_FOR_SYMBOLS / 1024,
                file_path.display()
            );
            return Ok((
                Vec::new(),
                Vec::new(),
                Vec::new(),
                HashMap::new(),
                file_info,
            ));
        }

        // 🔥 CRITICAL: Convert to relative Unix-style path for storage
        // File paths from discovery might be absolute OR relative - handle both
        let relative_path = if file_path.is_absolute() {
            // Absolute path - convert to relative
            crate::utils::paths::to_relative_unix_style(file_path, workspace_root)?
        } else {
            // Already relative - use as-is (just normalize to Unix-style)
            file_path.to_string_lossy().replace('\\', "/")
        };

        // 🚨 CRITICAL FIX: Tree-sitter parsing is CPU-intensive and blocks the runtime
        // Must wrap in spawn_blocking for large files (discovered with 158KB demo-data.js)
        let language_clone2 = language.to_string();
        let relative_path_clone = relative_path.clone();
        let content_clone = content.clone();
        let workspace_root_clone2 = workspace_root.to_path_buf();

        let results = {
            use std::time::Duration;
            let parse_start = std::time::Instant::now();

            // Spawn with a timeout for very large files
            let task = tokio::task::spawn_blocking(move || {
                // Create a new parser inside spawn_blocking (Parser isn't Send, so we can't move it in)
                let mut local_parser = tree_sitter::Parser::new();
                let tree_sitter_lang = crate::language::get_tree_sitter_language(&language_clone2)?;
                local_parser
                    .set_language(&tree_sitter_lang)
                    .map_err(|e| anyhow::anyhow!("Failed to set parser language: {}", e))?;

                let tree = local_parser.parse(&content_clone, None).ok_or_else(|| {
                    anyhow::anyhow!("Failed to parse file: {}", relative_path_clone)
                })?;

                let parse_elapsed = parse_start.elapsed();

                // Extract symbols - this is also CPU-intensive and can take minutes for large data files
                let extract_start = std::time::Instant::now();
                let result = crate::tools::workspace::ManageWorkspaceTool::extract_symbols_static(
                    &tree,
                    &relative_path_clone,
                    &content_clone,
                    &language_clone2,
                    &workspace_root_clone2,
                )?;

                let extract_elapsed = extract_start.elapsed();

                // Log timing for slow files (useful for performance analysis)
                if parse_elapsed.as_millis() > 50 || extract_elapsed.as_millis() > 100 {
                    debug!(
                        "Slow file processing: {} - parse: {:?}, extraction: {:?}",
                        relative_path_clone, parse_elapsed, extract_elapsed
                    );
                }

                Ok::<_, anyhow::Error>(result)
            });

            // Wait with a 30-second timeout for extraction
            match tokio::time::timeout(Duration::from_secs(30), task).await {
                Ok(Ok(result)) => result?,
                Ok(Err(e)) => {
                    return Err(anyhow::anyhow!("Spawn blocking task panicked: {}", e));
                }
                Err(_) => {
                    warn!(
                        "⏱️  Symbol extraction timed out after 30s for file: {} - skipping",
                        relative_path
                    );
                    return Ok((
                        Vec::new(),
                        Vec::new(),
                        Vec::new(),
                        HashMap::new(),
                        file_info,
                    ));
                }
            }
        };

        // file_info already created in spawn_blocking above - no need to recreate

        // Destructure ExtractionResults into all 4 fields
        let symbols = results.symbols;
        let relationships = results.relationships;
        let identifiers = results.identifiers;
        let types = results.types;

        // Only log if there are many symbols to avoid spam
        if symbols.len() > 10 {
            debug!(
                "📊 Extracted {} symbols from {}",
                symbols.len(),
                relative_path
            );
        }

        // Return data for bulk operations (SQLite storage)
        Ok((symbols, relationships, identifiers, types, file_info))
    }

    /// Process a file without a tree-sitter parser (no symbol extraction)
    ///
    /// Files without parsers are still indexed for full-text search via database.
    pub(crate) async fn process_file_without_parser(
        &self,
        file_path: &Path,
        language: &str,
        workspace_root: &Path, // NEW: Required for relative path conversion
    ) -> Result<(Vec<Symbol>, Vec<Relationship>, crate::database::FileInfo)> {
        tracing::trace!(
            "📂 Processing file without parser: {:?} (language: {})",
            file_path,
            language
        );

        // 🚨 CRITICAL FIX: Wrap ALL blocking filesystem I/O in spawn_blocking to prevent tokio deadlock
        let file_path_clone = file_path.to_path_buf();
        let language_clone = language.to_string();
        let workspace_root_clone = workspace_root.to_path_buf();

        let (_canonical_file_path, content, file_info) = tokio::task::spawn_blocking(move || {
            tracing::trace!(
                "🔄 Inside spawn_blocking (no parser) for: {:?}",
                file_path_clone
            );
            // Blocking operation 1: canonicalize (resolves symlinks: macOS /var -> /private/var)
            let canonical = file_path_clone
                .canonicalize()
                .unwrap_or_else(|_| file_path_clone.clone());

            // Blocking operation 2: read file content
            let file_content = std::fs::read_to_string(&canonical)
                .map_err(|e| anyhow::anyhow!("Failed to read file {:?}: {}", canonical, e))?;

            // Blocking operation 3: create file info (does metadata, hash, etc)
            let info = crate::database::create_file_info(
                &file_path_clone,
                &language_clone,
                &workspace_root_clone,
            )?;

            Ok::<_, anyhow::Error>((canonical, file_content, info))
        })
        .await
        .map_err(|e| anyhow::anyhow!("Failed to spawn blocking file I/O task: {}", e))??;

        trace!("Read {} bytes from file without parser", content.len());

        // No symbols extracted (no parser available), but file_info created in spawn_blocking above
        Ok((Vec::new(), Vec::new(), file_info))
    }
}
