//! File processing for indexing
//! Handles reading, parsing, and extracting symbols from individual files

use crate::extractors::{Relationship, Symbol};
use crate::handler::JulieServerHandler;
use crate::tools::workspace::commands::ManageWorkspaceTool;
use crate::tools::workspace::LanguageParserPool;
use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, info, trace, warn};
use tree_sitter::Parser;

impl ManageWorkspaceTool {
    /// SQLite-only file processing with optimized parser reuse
    ///
    /// Tantivy removed - using SQLite FTS5 for search.
    pub(crate) async fn process_files_optimized(
        &self,
        handler: &JulieServerHandler,
        files_to_index: Vec<PathBuf>,
        is_primary_workspace: bool,
        total_files: &mut usize,
        workspace_id: String, // Pass workspace_id instead of re-looking it up
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
                            .process_file_with_parser(&file_path, &language, parser, &workspace_root)
                            .await
                        {
                            Ok((symbols, relationships, file_info)) => {
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
                                    crate::utils::paths::to_relative_unix_style(&file_path, &workspace_root)
                                        .unwrap_or_else(|_| file_path.to_string_lossy().to_string())
                                } else {
                                    // Already relative - use as-is (just normalize to Unix-style)
                                    file_path.to_string_lossy().replace('\\', "/")
                                };
                                files_to_clean.push(relative_path);

                                // Collect data for bulk storage
                                all_symbols.extend(symbols);
                                all_relationships.extend(relationships);
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
                                    crate::utils::paths::to_relative_unix_style(&file_path, &workspace_root)
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

                let mut db_lock = db.lock().unwrap();

                if let Err(e) = db_lock.incremental_update_atomic(
                    &files_to_clean,
                    &all_file_infos,
                    &all_symbols,
                    &all_relationships,
                    &workspace_id,
                ) {
                    warn!("Failed to perform atomic incremental update: {}", e);
                    return Err(e);
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

                let mut db_lock = db.lock().unwrap();

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
        parser: &mut Parser,
        workspace_root: &Path, // NEW: Phase 2 - workspace root for relative paths
    ) -> Result<(Vec<Symbol>, Vec<Relationship>, crate::database::FileInfo)> {
        // Read file content for symbol extraction
        // 🔥 CRITICAL: Canonicalize path first to resolve symlinks (macOS /var -> /private/var)
        let canonical_file_path = file_path.canonicalize()
            .unwrap_or_else(|_| file_path.to_path_buf());

        let content = std::fs::read_to_string(&canonical_file_path)
            .map_err(|e| anyhow::anyhow!("Failed to read file {:?}: {}", canonical_file_path, e))?;

        // Skip empty files for symbol extraction
        if content.trim().is_empty() {
            // Convert to relative path for storage (handle both absolute and relative paths)
            let relative_path = if file_path.is_absolute() {
                crate::utils::paths::to_relative_unix_style(file_path, workspace_root)?
            } else {
                file_path.to_string_lossy().replace('\\', "/")
            };
            return Ok((
                Vec::new(),
                Vec::new(),
                crate::database::FileInfo {
                    path: relative_path,
                    language: language.to_string(),
                    hash: "empty".to_string(),
                    size: 0,
                    last_modified: 0,
                    last_indexed: 0,
                    symbol_count: 0,
                    content: Some(String::new()),
                },
            ));
        }

        // Skip symbol extraction for CSS/HTML (text search only)
        if !self.should_extract_symbols(language) {
            debug!(
                "⏭️  Skipping symbol extraction for {} file (text search only): {}",
                language,
                file_path.display()
            );

            // 🔥 create_file_info now handles relative path conversion internally
            let file_info = crate::database::create_file_info(file_path, language, workspace_root)?;

            // Return file info, but no extracted symbols
            return Ok((Vec::new(), Vec::new(), file_info));
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

        // PERFORMANCE OPTIMIZATION: Use pre-initialized parser instead of creating new one
        let tree = parser
            .parse(&content, None)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse file: {}", relative_path))?;

        // Extract symbols and relationships using language-specific extractor
        // Pass relative path so symbols are stored with relative paths
        let (symbols, relationships) =
            self.extract_symbols_with_existing_tree(&tree, &relative_path, &content, language, workspace_root)?;

        // Calculate file info for database storage
        // 🔥 create_file_info now handles relative path conversion internally
        let file_info = crate::database::create_file_info(file_path, language, workspace_root)?;

        // Only log if there are many symbols to avoid spam
        if symbols.len() > 10 {
            debug!(
                "📊 Extracted {} symbols from {}",
                symbols.len(),
                relative_path
            );
        }

        // Return data for bulk operations (SQLite storage)
        Ok((symbols, relationships, file_info))
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
        trace!(
            "Processing file without parser: {:?} (language: {})",
            file_path,
            language
        );

        // Read file content for database storage
        // 🔥 CRITICAL: Canonicalize path first to resolve symlinks (macOS /var -> /private/var)
        let canonical_file_path = file_path.canonicalize()
            .unwrap_or_else(|_| file_path.to_path_buf());

        let content = std::fs::read_to_string(&canonical_file_path)
            .map_err(|e| anyhow::anyhow!("Failed to read file {:?}: {}", canonical_file_path, e))?;

        trace!("Read {} bytes from {:?}", content.len(), canonical_file_path);

        // Calculate file info for database storage
        // 🔥 create_file_info now handles relative path conversion internally
        let file_info = crate::database::create_file_info(file_path, language, workspace_root)?;

        // No symbols extracted (no parser available)
        Ok((Vec::new(), Vec::new(), file_info))
    }
}
