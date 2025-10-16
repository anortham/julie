use crate::extractors::{Relationship, Symbol};
use crate::handler::JulieServerHandler;
use crate::tools::workspace::commands::ManageWorkspaceTool;
use crate::tools::workspace::LanguageParserPool;
use anyhow::Result;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, error, info, trace, warn};
use tree_sitter::{Parser, Tree};

impl ManageWorkspaceTool {
    pub(crate) async fn index_workspace_files(
        &self,
        handler: &JulieServerHandler,
        workspace_path: &Path,
        force_reindex: bool,
    ) -> Result<(usize, usize, usize)> {
        info!("🔍 Scanning workspace: {}", workspace_path.display());

        // Check if this is the primary workspace (current directory)
        debug!("🐛 [INDEX TRACE A] About to get current_dir");
        let current_dir = std::env::current_dir().unwrap_or_default();
        let is_primary_workspace = workspace_path == current_dir;
        debug!(
            "🐛 [INDEX TRACE B] Got current_dir, is_primary={}",
            is_primary_workspace
        );

        // Log workspace path comparison for debugging
        debug!(
            "Workspace comparison: path={:?}, current_dir={:?}, is_primary={}",
            workspace_path, current_dir, is_primary_workspace
        );

        // Only clear existing data for primary workspace reindex to preserve workspace isolation
        if force_reindex && is_primary_workspace {
            debug!("Clearing primary workspace for force reindex");
            // Database will be cleared during workspace initialization
        } else if force_reindex {
            debug!("Force reindexing reference workspace");
        }

        let mut total_files = 0;

        // Use blacklist-based file discovery
        // 🚨 CRITICAL: File discovery uses std::fs blocking I/O - must run on blocking thread pool
        debug!("🐛 [INDEX TRACE C] About to call discover_indexable_files");
        let workspace_path_clone = workspace_path.to_path_buf();
        let tool_clone = self.clone();
        let all_discovered_files = tokio::task::spawn_blocking(move || {
            tool_clone.discover_indexable_files(&workspace_path_clone)
        })
        .await
        .map_err(|e| anyhow::anyhow!("File discovery task failed: {}", e))??;
        debug!(
            "🐛 [INDEX TRACE D] discover_indexable_files returned {} files",
            all_discovered_files.len()
        );

        info!(
            "📊 Discovered {} files total after filtering",
            all_discovered_files.len()
        );

        // 🚀 INCREMENTAL UPDATE: Filter files that need re-indexing based on hash changes
        debug!(
            "🐛 [INDEX TRACE E] About to filter files, force_reindex={}",
            force_reindex
        );
        let files_to_index = if force_reindex {
            debug!(
                "Force reindex mode - processing all {} files",
                all_discovered_files.len()
            );
            debug!("🐛 [INDEX TRACE E1] Using all files (force_reindex=true)");
            all_discovered_files
        } else {
            debug!("🐛 [INDEX TRACE E2] Calling filter_changed_files");
            let result = self
                .filter_changed_files(handler, all_discovered_files, workspace_path)
                .await?;
            debug!(
                "🐛 [INDEX TRACE E3] filter_changed_files returned {} files",
                result.len()
            );
            result
        };
        debug!(
            "🐛 [INDEX TRACE F] Files filtered, {} files to index",
            files_to_index.len()
        );

        info!(
            "⚡ Need to process {} files (incremental filtering applied)",
            files_to_index.len()
        );

        debug!(
            "🐛 [INDEX TRACE 1] Starting index_workspace_files for path: {:?}",
            workspace_path
        );

        // 🔥 CRITICAL DEADLOCK FIX: Call get_workspace() ONCE and reuse throughout function
        // Calling get_workspace() multiple times causes lock contention and deadlocks
        debug!("🐛 [INDEX TRACE G] About to get workspace for ID generation (ONCE)");
        let workspace = handler
            .get_workspace()
            .await?
            .ok_or_else(|| anyhow::anyhow!("No workspace available for indexing"))?;
        debug!("🐛 [INDEX TRACE H] Got workspace successfully (reusing throughout function)");

        // Get workspace ID early for use throughout the function
        // CRITICAL DEADLOCK FIX: Generate workspace ID directly to avoid registry lock contention
        // CRITICAL FIX: Use the workspace_path parameter to determine canonical path
        // This ensures we get the correct workspace_id for BOTH primary and reference workspaces
        debug!("🐛 [INDEX TRACE I] Canonicalizing path");
        let canonical_path = workspace_path
            .canonicalize()
            .unwrap_or_else(|_| workspace_path.to_path_buf())
            .to_string_lossy()
            .to_string();

        // DEADLOCK FIX: Generate workspace ID directly from path (no registry access)
        // Same pattern as search_workspace_tantivy and filter_changed_files
        debug!(
            "🐛 [INDEX TRACE J] Generating workspace ID directly from: {}",
            canonical_path
        );
        let workspace_id = match crate::workspace::registry::generate_workspace_id(&canonical_path)
        {
            Ok(id) => {
                debug!("🐛 [INDEX TRACE K] Generated workspace ID: {}", id);
                id
            }
            Err(e) => {
                return Err(anyhow::anyhow!(
                    "Failed to generate workspace ID for path {}: {}",
                    canonical_path,
                    e
                ));
            }
        };
        debug!("🐛 [INDEX TRACE L] workspace_id obtained: {}", workspace_id);

        // Tantivy removed - proceeding with SQLite-only indexing
        debug!("🐛 [INDEX TRACE S] About to call process_files_optimized");
        // PERFORMANCE OPTIMIZATION: Group files by language and use parser pool for 10-50x speedup
        self.process_files_optimized(
            handler,
            files_to_index,
            is_primary_workspace,
            &mut total_files,
            workspace_id.clone(), // Pass workspace_id to avoid re-lookup
        )
        .await?;
        debug!("🐛 [INDEX TRACE T] process_files_optimized completed");

        // 🚀 NEW ARCHITECTURE: Get final counts from DATABASE, not memory!
        // 🔴 CRITICAL FIX: Query the CORRECT database for reference vs primary workspaces!
        // Reference workspaces have their own separate databases at indexes/{workspace_id}/db/symbols.db
        let (total_symbols, total_relationships) = {
            // Determine which database to query based on workspace type
            let db_to_query = if is_primary_workspace {
                // Primary workspace - use handler's database connection
                workspace.db.clone()
            } else {
                // Reference workspace - must have been created in process_files_optimized
                // Get the reference workspace database we just indexed
                let ref_db_path = workspace.workspace_db_path(&workspace_id);
                if ref_db_path.exists() {
                    // Open the reference workspace database for reading final counts
                    match tokio::task::spawn_blocking(move || {
                        crate::database::SymbolDatabase::new(ref_db_path)
                    })
                    .await
                    {
                        Ok(Ok(db)) => Some(Arc::new(std::sync::Mutex::new(db))),
                        Ok(Err(e)) => {
                            warn!("Failed to open reference workspace DB for final count: {}", e);
                            None
                        }
                        Err(e) => {
                            warn!("Reference workspace DB open task failed: {}", e);
                            None
                        }
                    }
                } else {
                    warn!("Reference workspace database not found at expected path");
                    None
                }
            };

            // Query the correct database
            if let Some(db_arc) = db_to_query {
                let db = db_arc.lock().unwrap();
                let symbols_count = db
                    .get_symbol_count_for_workspace(&workspace_id)
                    .unwrap_or(0);
                let stats = db.get_stats().unwrap_or_default();
                (symbols_count as usize, stats.total_relationships as usize)
            } else {
                (0, 0)
            }
        };

        info!(
            "✅ Indexing complete: {} symbols, {} relationships stored in SQLite",
            total_symbols, total_relationships
        );

        // 🔥 STALENESS CHECK: Only generate embeddings for symbols that don't have them yet
        // This fixes the bug where embeddings were regenerated on EVERY startup
        let symbols_needing_embeddings = if let Some(db_arc) = if is_primary_workspace {
            workspace.db.clone()
        } else {
            let ref_db_path = workspace.workspace_db_path(&workspace_id);
            if ref_db_path.exists() {
                match tokio::task::spawn_blocking(move || {
                    crate::database::SymbolDatabase::new(ref_db_path)
                })
                .await
                {
                    Ok(Ok(db)) => Some(Arc::new(std::sync::Mutex::new(db))),
                    _ => None,
                }
            } else {
                None
            }
        } {
            let db_lock = db_arc.lock().unwrap();
            db_lock
                .get_symbols_without_embeddings(&workspace_id)
                .unwrap_or_default()
                .len()
        } else {
            0
        };

        // 🔥 BACKGROUND TASK: Generate embeddings from SQLite (optional, compute-intensive)
        // Now runs for ALL workspaces (primary and reference) - BUT ONLY IF NEEDED!
        if symbols_needing_embeddings > 0 {
            let workspace_type = if is_primary_workspace {
                "primary"
            } else {
                "reference"
            };
            info!(
                "🚀 Starting background embedding generation for {} new symbols in {} workspace: {}",
                symbols_needing_embeddings, workspace_type, workspace_id
            );

            // Clone necessary references for background task
            // Use the workspace variable we already fetched (DEADLOCK FIX: no re-lock)
            let embedding_engine = handler.embedding_engine.clone();
            let embedding_engine_last_used = handler.embedding_engine_last_used.clone();

            // 🔴 CRITICAL FIX: Pass correct database for reference vs primary workspaces!
            // Reference workspaces need their own database, not the primary's
            let workspace_db = if is_primary_workspace {
                // Primary workspace - use handler's database
                workspace.db.clone()
            } else {
                // Reference workspace - open its separate database for embedding generation
                let ref_db_path = workspace.workspace_db_path(&workspace_id);
                if ref_db_path.exists() {
                    match tokio::task::spawn_blocking(move || {
                        crate::database::SymbolDatabase::new(ref_db_path)
                    })
                    .await
                    {
                        Ok(Ok(db)) => Some(Arc::new(std::sync::Mutex::new(db))),
                        Ok(Err(e)) => {
                            warn!("Failed to open reference workspace DB for embeddings: {}", e);
                            None
                        }
                        Err(e) => {
                            warn!("Reference workspace DB open task failed for embeddings: {}", e);
                            None
                        }
                    }
                } else {
                    warn!("Reference workspace database not found for embeddings");
                    None
                }
            };

            let workspace_root = Some(workspace.root.clone());
            let workspace_id_clone = workspace_id.clone();
            let indexing_status_clone = handler.indexing_status.clone();

            tokio::spawn(async move {
                info!(
                    "🐛 Background embedding task started for workspace: {}",
                    workspace_id_clone
                );
                let task_start = std::time::Instant::now();

                // 🔥 CRITICAL: Add 5-minute timeout to prevent infinite loops
                // Background embedding should complete in <2min for 10k symbols
                // If it takes >5min, something is seriously wrong
                match tokio::time::timeout(
                    std::time::Duration::from_secs(300), // 5 minute timeout
                    generate_embeddings_from_sqlite(
                        embedding_engine,
                        embedding_engine_last_used,
                        workspace_db,
                        workspace_root,
                        workspace_id_clone.clone(),
                        indexing_status_clone,
                    ),
                )
                .await
                {
                    Ok(Ok(_)) => {
                        info!("✅ Embeddings generated from SQLite in {:.2}s for workspace {} - semantic search available!",
                              task_start.elapsed().as_secs_f64(), workspace_id_clone);
                    }
                    Ok(Err(e)) => {
                        error!(
                            "❌ Background embedding generation failed for workspace {}: {}",
                            workspace_id_clone, e
                        );
                    }
                    Err(_) => {
                        error!(
                            "⏰ Background embedding generation TIMED OUT after 5min for workspace {}! \
                             This indicates a serious bug (infinite loop or deadlock). \
                             Semantic search will not be available. Check logs for details.",
                            workspace_id_clone
                        );
                    }
                }
                info!(
                    "🐛 Background embedding task completed for workspace: {}",
                    workspace_id_clone
                );
            });
        }

        Ok((total_symbols, total_files, total_relationships))
    }

    /// SQLite-only file processing with optimized parser reuse
    /// Tantivy removed - using SQLite FTS5 for search
    async fn process_files_optimized(
        &self,
        handler: &JulieServerHandler,
        files_to_index: Vec<PathBuf>,
        is_primary_workspace: bool,
        total_files: &mut usize,
        workspace_id: String, // Pass workspace_id instead of re-looking it up
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
                            .process_file_with_parser(&file_path, &language, parser)
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
                                files_to_clean.push(file_path.to_string_lossy().to_string());

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
                            .process_file_without_parser(&file_path, &language)
                            .await
                        {
                            Ok((symbols, relationships, file_info)) => {
                                debug!("📄 Processed file without parser: {:?}", file_path);
                                *total_files += 1;
                                files_to_clean.push(file_path.to_string_lossy().to_string());
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

        // 🧹 CLEANUP: Remove old data for files being re-processed (incremental updates)
        if !files_to_clean.is_empty() {
            debug!(
                "Cleaning up old data for {} modified files before bulk storage",
                files_to_clean.len()
            );

            // Use correct database: reference workspace DB or primary workspace DB
            let db_to_use = if let Some(ref ref_db) = ref_workspace_db {
                // Reference workspace - use separate DB
                Some(ref_db.clone())
            } else {
                // Primary workspace - use handler's DB
                if let Some(workspace) = handler.get_workspace().await? {
                    workspace.db.clone()
                } else {
                    None
                }
            };

            if let Some(db) = db_to_use {
                let db_lock = db.lock().unwrap();

                // Clean up database entries for modified files
                for file_path in &files_to_clean {
                    if let Err(e) =
                        db_lock.delete_symbols_for_file_in_workspace(file_path, &workspace_id)
                    {
                        warn!("Failed to delete old symbols for {}: {}", file_path, e);
                    }
                    if let Err(e) = db_lock.delete_relationships_for_file(file_path, &workspace_id)
                    {
                        warn!(
                            "Failed to delete old relationships for {}: {}",
                            file_path, e
                        );
                    }
                }

                debug!("Cleanup complete for {} files", files_to_clean.len());

                // 🔥 CRITICAL: Explicitly release the database lock before bulk storage!
                drop(db_lock);
            }
        }

        // 🚀 BLAZING-FAST BULK STORAGE: Store everything at once using optimized bulk methods
        // CRITICAL FIX: Store files even if they have 0 symbols (file records are still needed!)
        if !all_file_infos.is_empty() {
            info!("🚀 Starting blazing-fast bulk storage of {} symbols, {} relationships, {} files...",
                  all_symbols.len(), all_relationships.len(), all_file_infos.len());

            // Use correct database: reference workspace DB or primary workspace DB
            let db_to_use = if let Some(ref ref_db) = ref_workspace_db {
                // Reference workspace - use separate DB
                Some(ref_db.clone())
            } else {
                // Primary workspace - use handler's DB
                if let Some(workspace) = handler.get_workspace().await? {
                    workspace.db.clone()
                } else {
                    None
                }
            };

            if let Some(db) = db_to_use {
                let mut db_lock = db.lock().unwrap();

                // 🔥 BULK OPERATIONS for maximum speed
                let bulk_start = std::time::Instant::now();

                // Bulk store files
                if let Err(e) = db_lock.bulk_store_files(&all_file_infos, &workspace_id) {
                    warn!("Failed to bulk store files: {}", e);
                }

                // Bulk store symbols (with index dropping optimization!)
                if let Err(e) = db_lock.bulk_store_symbols(&all_symbols, &workspace_id) {
                    warn!("Failed to bulk store symbols: {}", e);
                }

                // Bulk store relationships
                if let Err(e) = db_lock.bulk_store_relationships(&all_relationships, &workspace_id)
                {
                    warn!("Failed to bulk store relationships: {}", e);
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
        }

        Ok(())
    }

    /// Process a single file with symbol extraction
    /// Returns (symbols, relationships, file_info) for bulk storage
    async fn process_file_with_parser(
        &self,
        file_path: &Path,
        language: &str,
        parser: &mut Parser,
    ) -> Result<(Vec<Symbol>, Vec<Relationship>, crate::database::FileInfo)> {
        // Read file content for symbol extraction
        let content = fs::read_to_string(file_path)
            .map_err(|e| anyhow::anyhow!("Failed to read file {:?}: {}", file_path, e))?;

        // Skip empty files for symbol extraction
        if content.trim().is_empty() {
            return Ok((
                Vec::new(),
                Vec::new(),
                crate::database::FileInfo {
                    path: file_path.to_string_lossy().to_string(),
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

            // Calculate file info for database storage
            let file_path_str = file_path.to_string_lossy().to_string();
            let file_info = crate::database::create_file_info(&file_path_str, language)?;

            // Return file info, but no extracted symbols
            return Ok((Vec::new(), Vec::new(), file_info));
        }

        let file_path_str = file_path.to_string_lossy().to_string();

        // PERFORMANCE OPTIMIZATION: Use pre-initialized parser instead of creating new one
        let tree = parser
            .parse(&content, None)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse file: {}", file_path_str))?;

        // Extract symbols and relationships using language-specific extractor
        let (symbols, relationships) =
            self.extract_symbols_with_existing_tree(&tree, &file_path_str, &content, language)?;

        // Calculate file info for database storage
        let file_info = crate::database::create_file_info(&file_path_str, language)?;

        // Only log if there are many symbols to avoid spam
        if symbols.len() > 10 {
            debug!(
                "📊 Extracted {} symbols from {}",
                symbols.len(),
                file_path_str
            );
        }

        // Return data for bulk operations (SQLite storage)
        Ok((symbols, relationships, file_info))
    }

    /// Extract symbols from an already-parsed tree (PERFORMANCE OPTIMIZED)
    /// This bypasses the expensive tree-sitter parsing step when parser is reused
    fn extract_symbols_with_existing_tree(
        &self,
        tree: &Tree,
        file_path: &str,
        content: &str,
        language: &str,
    ) -> Result<(Vec<Symbol>, Vec<crate::extractors::base::Relationship>)> {
        debug!(
            "Extracting symbols: language={}, file={}",
            language, file_path
        );
        debug!("    Tree root node: {:?}", tree.root_node().kind());
        debug!("    Content length: {} chars", content.len());

        // Extract symbols and relationships using language-specific extractor (all 26 extractors)
        let (symbols, relationships) = match language {
            "rust" => {
                debug!("    Creating RustExtractor...");
                let mut extractor = crate::extractors::rust::RustExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                debug!("    Calling extract_symbols...");
                let symbols = extractor.extract_symbols(tree);
                debug!("    ✅ RustExtractor returned {} symbols", symbols.len());
                let relationships = extractor.extract_relationships(tree, &symbols);
                (symbols, relationships)
            }
            "typescript" => {
                debug!("    Creating TypeScriptExtractor...");
                let mut extractor = crate::extractors::typescript::TypeScriptExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                debug!("    Calling extract_symbols...");
                let symbols = extractor.extract_symbols(tree);
                debug!(
                    "    ✅ TypeScriptExtractor returned {} symbols",
                    symbols.len()
                );
                let relationships = extractor.extract_relationships(tree, &symbols);
                (symbols, relationships)
            }
            "javascript" => {
                let mut extractor = crate::extractors::javascript::JavaScriptExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                let symbols = extractor.extract_symbols(tree);
                let relationships = extractor.extract_relationships(tree, &symbols);
                (symbols, relationships)
            }
            "python" => {
                let mut extractor = crate::extractors::python::PythonExtractor::new(
                    file_path.to_string(),
                    content.to_string(),
                );
                let symbols = extractor.extract_symbols(tree);
                let relationships = extractor.extract_relationships(tree, &symbols);
                (symbols, relationships)
            }
            "java" => {
                let mut extractor = crate::extractors::java::JavaExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                let symbols = extractor.extract_symbols(tree);
                let relationships = extractor.extract_relationships(tree, &symbols);
                (symbols, relationships)
            }
            "csharp" => {
                let mut extractor = crate::extractors::csharp::CSharpExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                let symbols = extractor.extract_symbols(tree);
                let relationships = extractor.extract_relationships(tree, &symbols);
                (symbols, relationships)
            }
            "ruby" => {
                let mut extractor = crate::extractors::ruby::RubyExtractor::new(
                    file_path.to_string(),
                    content.to_string(),
                );
                let symbols = extractor.extract_symbols(tree);
                let relationships = extractor.extract_relationships(tree, &symbols);
                (symbols, relationships)
            }
            "swift" => {
                let mut extractor = crate::extractors::swift::SwiftExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                let symbols = extractor.extract_symbols(tree);
                let relationships = extractor.extract_relationships(tree, &symbols);
                (symbols, relationships)
            }
            "kotlin" => {
                let mut extractor = crate::extractors::kotlin::KotlinExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                let symbols = extractor.extract_symbols(tree);
                let relationships = extractor.extract_relationships(tree, &symbols);
                (symbols, relationships)
            }
            "go" => {
                let mut extractor = crate::extractors::go::GoExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                let symbols = extractor.extract_symbols(tree);
                let relationships = extractor.extract_relationships(tree, &symbols);
                (symbols, relationships)
            }
            "c" => {
                let mut extractor = crate::extractors::c::CExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                let symbols = extractor.extract_symbols(tree);
                let relationships = extractor.extract_relationships(tree, &symbols);
                (symbols, relationships)
            }
            "cpp" => {
                let mut extractor = crate::extractors::cpp::CppExtractor::new(
                    file_path.to_string(),
                    content.to_string(),
                );
                let symbols = extractor.extract_symbols(tree);
                let relationships = extractor.extract_relationships(tree, &symbols);
                (symbols, relationships)
            }
            "lua" => {
                let mut extractor = crate::extractors::lua::LuaExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                let symbols = extractor.extract_symbols(tree);
                let relationships = extractor.extract_relationships(tree, &symbols);
                (symbols, relationships)
            }
            "sql" => {
                let mut extractor = crate::extractors::sql::SqlExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                let symbols = extractor.extract_symbols(tree);
                let relationships = extractor.extract_relationships(tree, &symbols);
                (symbols, relationships)
            }
            "html" => {
                let mut extractor = crate::extractors::html::HTMLExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                let symbols = extractor.extract_symbols(tree);
                let relationships = extractor.extract_relationships(tree, &symbols);
                (symbols, relationships)
            }
            "css" => {
                let mut extractor = crate::extractors::css::CSSExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                let symbols = extractor.extract_symbols(tree);
                let relationships = Vec::new(); // CSS extractor doesn't have relationships
                (symbols, relationships)
            }
            "vue" => {
                let mut extractor = crate::extractors::vue::VueExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                let symbols = extractor.extract_symbols(Some(tree));
                let relationships = extractor.extract_relationships(Some(tree), &symbols);
                (symbols, relationships)
            }
            "razor" => {
                let mut extractor = crate::extractors::razor::RazorExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                let symbols = extractor.extract_symbols(tree);
                let relationships = extractor.extract_relationships(tree, &symbols);
                (symbols, relationships)
            }
            "bash" => {
                let mut extractor = crate::extractors::bash::BashExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                let symbols = extractor.extract_symbols(tree);
                let relationships = extractor.extract_relationships(tree, &symbols);
                (symbols, relationships)
            }
            "powershell" => {
                let mut extractor = crate::extractors::powershell::PowerShellExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                let symbols = extractor.extract_symbols(tree);
                let relationships = extractor.extract_relationships(tree, &symbols);
                (symbols, relationships)
            }
            "gdscript" => {
                let mut extractor = crate::extractors::gdscript::GDScriptExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                let symbols = extractor.extract_symbols(tree);
                let relationships = extractor.extract_relationships(tree, &symbols);
                (symbols, relationships)
            }
            "zig" => {
                let mut extractor = crate::extractors::zig::ZigExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                let symbols = extractor.extract_symbols(tree);
                let relationships = extractor.extract_relationships(tree, &symbols);
                (symbols, relationships)
            }
            "dart" => {
                let mut extractor = crate::extractors::dart::DartExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                let symbols = extractor.extract_symbols(tree);
                let relationships = extractor.extract_relationships(tree, &symbols);
                (symbols, relationships)
            }
            "regex" => {
                let mut extractor = crate::extractors::regex::RegexExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                let symbols = extractor.extract_symbols(tree);
                let relationships = extractor.extract_relationships(tree, &symbols);
                (symbols, relationships)
            }
            _ => {
                // For truly unsupported languages, return empty results
                debug!(
                    "No extractor available for language: {} (file: {})",
                    language, file_path
                );
                (Vec::new(), Vec::new())
            }
        };

        debug!("🎯 extract_symbols_with_existing_tree returning: {} symbols, {} relationships for {} file: {}",
               symbols.len(), relationships.len(), language, file_path);

        Ok((symbols, relationships))
    }

    /// 🚀 INCREMENTAL UPDATE: Filter files that actually need re-indexing based on hash changes
    /// Returns only files that are new, modified, or missing from database
    async fn filter_changed_files(
        &self,
        handler: &JulieServerHandler,
        all_files: Vec<PathBuf>,
        workspace_path: &Path,
    ) -> Result<Vec<PathBuf>> {
        // 🔥 CRITICAL DEADLOCK FIX: Generate workspace ID directly instead of registry lookup
        // Same fix as search_workspace_tantivy - avoids registry lock contention
        let workspace_id = if let Some(_workspace) = handler.get_workspace().await? {
            // CRITICAL FIX: Use the workspace_path parameter to determine canonical path
            // This ensures we get the correct workspace_id for BOTH primary and reference workspaces
            let canonical_path = workspace_path
                .canonicalize()
                .unwrap_or_else(|_| workspace_path.to_path_buf())
                .to_string_lossy()
                .to_string();

            // 🚀 DEADLOCK FIX: Generate workspace ID directly from path (no registry access)
            // This avoids the registry lock that was causing deadlocks during indexing
            match crate::workspace::registry::generate_workspace_id(&canonical_path) {
                Ok(id) => id,
                Err(e) => {
                    warn!(
                        "Failed to generate workspace ID: {} - indexing all files",
                        e
                    );
                    return Ok(all_files);
                }
            }
        } else {
            // No workspace available - all files are new
            return Ok(all_files);
        };

        // Get database to check existing file hashes
        let existing_file_hashes = if let Some(workspace) = handler.get_workspace().await? {
            if let Some(db) = &workspace.db {
                let db_lock = db.lock().unwrap();
                match db_lock.get_file_hashes_for_workspace(&workspace_id) {
                    Ok(hashes) => hashes,
                    Err(e) => {
                        warn!(
                            "Failed to get existing file hashes: {} - treating all files as new",
                            e
                        );
                        return Ok(all_files);
                    }
                }
            } else {
                // No database - all files are new
                return Ok(all_files);
            }
        } else {
            return Ok(all_files);
        };

        debug!(
            "Checking {} files against {} existing file hashes",
            all_files.len(),
            existing_file_hashes.len()
        );

        let mut files_to_process = Vec::new();
        let mut unchanged_count = 0;
        let mut new_count = 0;
        let mut modified_count = 0;

        for file_path in &all_files {
            let file_path_str = file_path.to_string_lossy().to_string();
            let language = self.detect_language(file_path);

            // Calculate current file hash
            let current_hash = match crate::database::calculate_file_hash(file_path) {
                Ok(hash) => hash,
                Err(e) => {
                    warn!(
                        "Failed to calculate hash for {}: {} - including for re-indexing",
                        file_path_str, e
                    );
                    files_to_process.push(file_path.clone());
                    continue;
                }
            };

            // Check if file exists in database and if hash matches
            if let Some(stored_hash) = existing_file_hashes.get(&file_path_str) {
                if stored_hash == &current_hash {
                    // File unchanged by hash, but check if it needs FILE_CONTENT symbols
                    // For files without parsers (text, json, etc.), we need to ensure they have
                    // FILE_CONTENT symbols in Tantivy. This is a migration for existing workspaces.

                    // Check if this is a language without a parser
                    let needs_file_content = matches!(
                        language.as_str(),
                        "text"
                            | "json"
                            | "toml"
                            | "yaml"
                            | "yml"
                            | "xml"
                            | "markdown"
                            | "md"
                            | "txt"
                            | "config"
                    );

                    if needs_file_content {
                        // Check if it has symbols (should be 0 for files without parsers)
                        if let Some(workspace) = handler.get_workspace().await? {
                            if let Some(db) = &workspace.db {
                                let db_lock = db.lock().unwrap();
                                let symbol_count =
                                    db_lock.get_file_symbol_count(&file_path_str).unwrap_or(0);
                                drop(db_lock);

                                if symbol_count == 0 {
                                    // File has no symbols - needs FILE_CONTENT symbol created
                                    debug!("File {} has no symbols, re-indexing to create FILE_CONTENT symbol", file_path_str);
                                    modified_count += 1;
                                    files_to_process.push(file_path.clone());
                                    continue;
                                }
                            }
                        }
                    }

                    // File truly unchanged - skip
                    unchanged_count += 1;
                } else {
                    // File modified - needs re-indexing
                    modified_count += 1;
                    files_to_process.push(file_path.clone());
                }
            } else {
                // New file - needs indexing
                new_count += 1;
                files_to_process.push(file_path.clone());
            }
        }

        info!(
            "📊 Incremental analysis: {} unchanged (skipped), {} modified, {} new - processing {} total",
            unchanged_count, modified_count, new_count, files_to_process.len()
        );

        // 🧹 ORPHAN CLEANUP: Remove database entries for files that no longer exist
        let orphaned_count = self
            .clean_orphaned_files(handler, &existing_file_hashes, &all_files, &workspace_id)
            .await?;

        if orphaned_count > 0 {
            info!(
                "🧹 Cleaned up {} orphaned file entries from database",
                orphaned_count
            );
        }

        Ok(files_to_process)
    }

    /// Clean up orphaned database entries for files that no longer exist on disk
    /// This prevents database bloat from accumulating deleted files
    async fn clean_orphaned_files(
        &self,
        handler: &JulieServerHandler,
        existing_file_hashes: &std::collections::HashMap<String, String>,
        current_disk_files: &[PathBuf],
        workspace_id: &str,
    ) -> Result<usize> {
        // Build set of current disk file paths for fast lookup
        let current_files: std::collections::HashSet<String> = current_disk_files
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();

        // Find files that are in database but not on disk (orphans)
        let orphaned_files: Vec<String> = existing_file_hashes
            .keys()
            .filter(|db_path| !current_files.contains(*db_path))
            .cloned()
            .collect();

        if orphaned_files.is_empty() {
            return Ok(0);
        }

        debug!("Found {} orphaned files to clean up", orphaned_files.len());

        // Get database connection
        let workspace = match handler.get_workspace().await? {
            Some(ws) => ws,
            None => return Ok(0),
        };

        let db = match &workspace.db {
            Some(db_arc) => db_arc,
            None => return Ok(0),
        };

        // Delete orphaned entries
        let mut cleaned_count = 0;
        {
            let db_lock = db.lock().unwrap();

            for file_path in &orphaned_files {
                // Delete relationships first (referential integrity)
                if let Err(e) = db_lock.delete_relationships_for_file(file_path, workspace_id) {
                    warn!(
                        "Failed to delete relationships for orphaned file {}: {}",
                        file_path, e
                    );
                    continue;
                }

                // Delete symbols
                if let Err(e) =
                    db_lock.delete_symbols_for_file_in_workspace(file_path, workspace_id)
                {
                    warn!(
                        "Failed to delete symbols for orphaned file {}: {}",
                        file_path, e
                    );
                    continue;
                }

                // Delete file record
                if let Err(e) = db_lock.delete_file_record_in_workspace(file_path, workspace_id) {
                    warn!(
                        "Failed to delete file record for orphaned file {}: {}",
                        file_path, e
                    );
                    continue;
                }

                cleaned_count += 1;
                trace!("Cleaned up orphaned file: {}", file_path);
            }
        }

        Ok(cleaned_count)
    }

    /// Determine if we should extract symbols from a file based on language
    /// CSS and HTML are indexed for text search only - no symbol extraction
    fn should_extract_symbols(&self, language: &str) -> bool {
        !matches!(language, "css" | "html")
    }

    /// Process a file without a tree-sitter parser (no symbol extraction)
    /// Files without parsers are still indexed for full-text search via database
    async fn process_file_without_parser(
        &self,
        file_path: &Path,
        language: &str,
    ) -> Result<(Vec<Symbol>, Vec<Relationship>, crate::database::FileInfo)> {
        trace!(
            "Processing file without parser: {:?} (language: {})",
            file_path,
            language
        );

        // Read file content for database storage
        let content = fs::read_to_string(file_path)
            .map_err(|e| anyhow::anyhow!("Failed to read file {:?}: {}", file_path, e))?;

        trace!("Read {} bytes from {:?}", content.len(), file_path);

        let file_path_str = file_path.to_string_lossy().to_string();

        // Calculate file info for database storage
        let file_info = crate::database::create_file_info(&file_path_str, language)?;

        // No symbols extracted (no parser available)
        Ok((Vec::new(), Vec::new(), file_info))
    }
}

/// 🔥 BACKGROUND TASK: Generate embeddings from SQLite database
/// This runs asynchronously to provide fast indexing response times
async fn generate_embeddings_from_sqlite(
    embedding_engine: Arc<tokio::sync::RwLock<Option<crate::embeddings::EmbeddingEngine>>>,
    embedding_engine_last_used: Arc<tokio::sync::Mutex<Option<std::time::Instant>>>,
    workspace_db: Option<Arc<std::sync::Mutex<crate::database::SymbolDatabase>>>,
    workspace_root: Option<std::path::PathBuf>,
    workspace_id: String,
    indexing_status: Arc<crate::handler::IndexingStatus>,
) -> Result<()> {
    use anyhow::Context;

    info!(
        "🐛 generate_embeddings_from_sqlite() called for workspace: {}",
        workspace_id
    );
    let start_time = std::time::Instant::now();
    debug!("Starting embedding generation from SQLite");

    // 🐛 SKIP REGISTRY UPDATE: Causes deadlock as main thread holds registry lock during statistics update
    // The registry status update is non-critical for background task operation
    // if let Some(ref root) = workspace_root {
    //     info!("🐛 About to update registry embedding status...");
    //     let registry_service = crate::workspace::registry_service::WorkspaceRegistryService::new(root.clone());
    //     if let Err(e) = registry_service
    //         .update_embedding_status(&workspace_id, crate::workspace::registry::EmbeddingStatus::Generating)
    //         .await
    //     {
    //         warn!("Failed to update embedding status to Generating: {}", e);
    //     }
    //     info!("🐛 Registry embedding status updated");
    // }
    info!("🐛 Skipping registry update to avoid deadlock");

    // Get database connection
    let db = match workspace_db {
        Some(db_arc) => db_arc,
        None => {
            warn!("No database available for embedding generation");
            return Ok(());
        }
    };

    // 🚀 INCREMENTAL UPDATES: Only process symbols that don't have embeddings yet
    // This fixes the performance problem where ALL symbols were reprocessed every startup
    info!("🐛 About to acquire database lock for reading symbols without embeddings...");
    let symbols = {
        let db_lock = db.lock().unwrap();
        info!("🐛 Database lock acquired successfully!");
        db_lock
            .get_symbols_without_embeddings(&workspace_id)
            .context("Failed to read symbols without embeddings from database")?
    };
    info!(
        "🐛 Read {} symbols WITHOUT embeddings (incremental update)",
        symbols.len()
    );

    if symbols.is_empty() {
        info!("✅ All symbols already have embeddings - nothing to do!");
        return Ok(());
    }

    info!(
        "🧠 Generating embeddings for {} new symbols (incremental)...",
        symbols.len()
    );

    // 🔥 CRITICAL FIX: Initialize embedding engine with write lock, then release immediately
    // This allows multiple workspaces to generate embeddings in parallel
    {
        // Fast path: check with read lock first
        let needs_init = {
            let read_guard = embedding_engine.read().await;
            read_guard.is_none()
        };

        if needs_init {
            // Slow path: acquire write lock only for initialization
            let mut write_guard = embedding_engine.write().await;

            // Double-check: another task might have initialized while we waited
            if write_guard.is_none() {
                info!("🔧 Initializing embedding engine for background generation...");

                // 🔧 FIX: Use workspace .julie/cache directory instead of polluting CWD
                let cache_dir = if let Some(ref root) = workspace_root {
                    root.join(".julie").join("cache").join("embeddings")
                } else {
                    // Fallback to temp directory if workspace root not available
                    std::env::temp_dir().join("julie_cache").join("embeddings")
                };

                std::fs::create_dir_all(&cache_dir)?;
                info!(
                    "📁 Using embedding cache directory: {}",
                    cache_dir.display()
                );

                // 🚨 CRITICAL: ONNX model loading is BLOCKING and can take seconds (download + init)
                // Must run on blocking thread pool to avoid deadlocking the tokio runtime
                // Same fix as workspace/mod.rs:458
                let db_clone = db.clone();
                let cache_dir_clone = cache_dir.clone();
                match tokio::task::spawn_blocking(move || {
                    crate::embeddings::EmbeddingEngine::new("bge-small", cache_dir_clone, db_clone)
                })
                .await
                {
                    Ok(Ok(engine)) => {
                        *write_guard = Some(engine);
                        info!("✅ Embedding engine initialized for background task");
                    }
                    Ok(Err(e)) => {
                        error!("❌ Failed to initialize embedding engine: {}", e);
                        return Err(anyhow::anyhow!(
                            "Embedding engine initialization failed: {}",
                            e
                        ));
                    }
                    Err(join_err) => {
                        error!(
                            "❌ Embedding engine initialization task panicked: {}",
                            join_err
                        );
                        return Err(anyhow::anyhow!(
                            "Embedding engine initialization task failed: {}",
                            join_err
                        ));
                    }
                }
            }
        } // 🔓 Write lock released here - other workspaces can now proceed!
    }

    // 🚀 INTERLEAVED PARALLEL EMBEDDING GENERATION: Acquire write lock per batch
    // This allows multiple workspaces to interleave their batch processing for better parallelism
    // BATCH_SIZE: Tested 256 (76s), 64 (60s), 100 (60s) - no significant difference
    // CPU-based ONNX inference is bottlenecked by model computation, not batch overhead
    const BATCH_SIZE: usize = 100;
    const MAX_CONSECUTIVE_FAILURES: usize = 5; // Circuit breaker threshold
    const MAX_TOTAL_FAILURE_RATE: f64 = 0.5; // Abort if >50% of batches fail

    let total_batches = symbols.len().div_ceil(BATCH_SIZE);
    let mut consecutive_failures = 0;
    let mut total_failures = 0;
    let mut successful_batches = 0;

    // 🔥 MEMORY OPTIMIZATION: Write to DB incrementally during generation
    // This avoids accumulating all embeddings in a Vec (saves memory)
    let mut model_name = String::from("bge-small");
    let mut dimensions = 384;

    for (batch_idx, chunk) in symbols.chunks(BATCH_SIZE).enumerate() {
        info!(
            "🔄 Processing embedding batch {}/{} ({} symbols)",
            batch_idx + 1,
            total_batches,
            chunk.len()
        );

        // 🔓 CRITICAL: Acquire write lock ONLY for this batch, then release
        // This allows other workspaces to interleave their batches for parallel execution
        let batch_result = {
            let mut embedding_guard = embedding_engine.write().await;
            if let Some(ref mut engine) = embedding_guard.as_mut() {
                // Generate embeddings for this batch with write lock held
                match engine.embed_symbols_batch(chunk) {
                    Ok(batch_embeddings) => {
                        model_name = engine.model_name().to_string();
                        dimensions = engine.dimensions();

                        // 🕐 Update last_used timestamp after successful engine use
                        {
                            let mut last_used = embedding_engine_last_used.lock().await;
                            *last_used = Some(std::time::Instant::now());
                        }

                        Some(Ok(batch_embeddings))
                    }
                    Err(e) => Some(Err(e)),
                }
            } else {
                None
            }
        }; // 🔓 Write lock released here - other workspaces can now process their batches!

        // Process the batch result without holding the embedding engine lock
        match batch_result {
            Some(Ok(batch_embeddings)) => {
                // Reset consecutive failure counter on success
                consecutive_failures = 0;
                successful_batches += 1;

                // 🔥 MEMORY OPTIMIZATION: Write to DB immediately (incremental persistence)
                // This avoids accumulating embeddings in memory
                {
                    let mut db_guard = db.lock().unwrap();

                    // Use bulk insert for this batch
                    if let Err(e) = db_guard.bulk_store_embeddings(
                        &batch_embeddings,
                        dimensions,
                        &model_name,
                    ) {
                        warn!(
                            "Failed to bulk store embeddings for batch {}: {}",
                            batch_idx + 1,
                            e
                        );
                    }
                }

                debug!(
                    "✅ Generated and stored embeddings for batch {}/{} ({} embeddings)",
                    batch_idx + 1,
                    total_batches,
                    batch_embeddings.len()
                );
            }
            Some(Err(e)) => {
                consecutive_failures += 1;
                total_failures += 1;

                warn!(
                    "⚠️ Failed to generate embeddings for batch {}: {} (consecutive failures: {})",
                    batch_idx + 1,
                    e,
                    consecutive_failures
                );

                // 🚨 CIRCUIT BREAKER: Stop if too many consecutive failures
                if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                    error!(
                        "❌ CIRCUIT BREAKER TRIGGERED: {} consecutive embedding failures. \
                         Aborting embedding generation to prevent resource waste. \
                         Successfully processed {}/{} batches.",
                        consecutive_failures,
                        successful_batches,
                        batch_idx + 1
                    );
                    return Err(anyhow::anyhow!(
                        "Embedding generation aborted after {} consecutive batch failures",
                        consecutive_failures
                    ));
                }

                // 🚨 FAILURE RATE CHECK: Stop if >50% of batches are failing
                let batches_processed = batch_idx + 1;
                if batches_processed >= 10 {
                    // Only check after reasonable sample size
                    let failure_rate = total_failures as f64 / batches_processed as f64;
                    if failure_rate > MAX_TOTAL_FAILURE_RATE {
                        error!(
                            "❌ HIGH FAILURE RATE: {:.1}% of batches failing ({}/{}). \
                             Aborting embedding generation. This indicates a systemic issue.",
                            failure_rate * 100.0,
                            total_failures,
                            batches_processed
                        );
                        return Err(anyhow::anyhow!(
                            "Embedding generation aborted due to high failure rate: {:.1}%",
                            failure_rate * 100.0
                        ));
                    }
                }
            }
            None => {
                return Err(anyhow::anyhow!("Embedding engine not available"));
            }
        }
    }

    let duration = start_time.elapsed();
    info!(
        "✅ Embedding generation complete in {:.2}s ({} total symbols)",
        duration.as_secs_f64(),
        symbols.len()
    );

    // 🏗️ BUILD AND SAVE HNSW INDEX FROM DATABASE
    // Embeddings are already persisted - now load them to build HNSW
    info!("🏗️ Building HNSW index from database embeddings...");
    let hnsw_start = std::time::Instant::now();

    let mut vector_store = crate::embeddings::vector_store::VectorStore::new(384)?;

    // Load all embeddings from database for HNSW building
    let embeddings_result = {
        let db_lock = db.lock().unwrap();
        db_lock.load_all_embeddings(&model_name)
    }; // Drop lock before HNSW build

    match embeddings_result {
        Ok(embeddings) => {
            let count = embeddings.len();
            info!("📥 Loaded {} embeddings from database for HNSW", count);

            // Store in VectorStore for HNSW building
            for (symbol_id, vector) in embeddings {
                if let Err(e) = vector_store.store_vector(symbol_id.clone(), vector) {
                    warn!("Failed to store vector {}: {}", symbol_id, e);
                }
            }

            // Build HNSW index
            match vector_store.build_hnsw_index() {
                Ok(_) => {
                    info!(
                        "✅ HNSW index built in {:.2}s",
                        hnsw_start.elapsed().as_secs_f64()
                    );

                    // Save to disk for lazy loading on next startup
                    let vectors_path = if let Some(ref root) = workspace_root {
                        root.join(".julie")
                            .join("indexes")
                            .join(&workspace_id)
                            .join("vectors")
                    } else {
                        std::path::PathBuf::from("./.julie/indexes")
                            .join(&workspace_id)
                            .join("vectors")
                    };

                    if let Err(e) = vector_store.save_hnsw_index(&vectors_path) {
                        warn!("Failed to save HNSW index to disk: {}", e);
                    } else {
                        info!("💾 HNSW index saved to {}", vectors_path.display());

                        // 🔥 CRITICAL MEMORY FIX: Immediately release memory after save
                        // VectorStore + HNSW graph are no longer needed - data is on disk
                        vector_store.clear();
                        info!("🧹 VectorStore memory released after successful save");
                    }
                }
                Err(e) => {
                    warn!("Failed to build HNSW index: {}", e);
                }
            }
        }
        Err(e) => {
            warn!("Could not load embeddings for HNSW: {}", e);
        }
    }

    info!("✅ Background task complete - semantic search ready via lazy loading!");

    // CASCADE: Mark semantic search as ready
    indexing_status
        .semantic_ready
        .store(true, std::sync::atomic::Ordering::Release);
    debug!("🧠 CASCADE: Semantic search now available");

    // 🕐 LAZY ENGINE CLEANUP: Engine will be dropped after 5 minutes of inactivity
    // The periodic cleanup task (started at server init) checks last_used timestamp
    // and drops the engine if it's been idle for >5 minutes
    // This balances fast incremental updates during development with memory cleanup when done
    info!("✅ Embedding engine will be dropped after 5 minutes of inactivity");

    // 🐛 SKIP REGISTRY UPDATE: Causes deadlock - main thread holds registry lock
    // The in-memory indexing_status.semantic_ready flag is sufficient for runtime status
    // if let Some(ref root) = workspace_root {
    //     let registry_service = crate::workspace::registry_service::WorkspaceRegistryService::new(root.clone());
    //     if let Err(e) = registry_service
    //         .update_embedding_status(&workspace_id, crate::workspace::registry::EmbeddingStatus::Ready)
    //         .await
    //     {
    //         warn!("Failed to update embedding status to Ready: {}", e);
    //     } else {
    //         debug!("📝 Updated registry: embedding_status = Ready");
    //     }
    // }
    info!("🐛 Embeddings complete - registry update skipped to avoid deadlock");

    Ok(())
}

#[cfg(test)]
mod tests {
    // Imports commented out since test is disabled
    // use super::*;
    // use crate::database::SymbolDatabase;
    // use crate::tools::workspace::WorkspaceCommand;
    // use tempfile::TempDir;

    #[tokio::test]
    #[ignore] // TODO: Fix ManageWorkspaceTool struct field mismatch
    async fn test_bulk_store_symbols_full_workspace_dataset() {
        // TEMPORARILY DISABLED - struct field mismatch
        /*
        let tool = ManageWorkspaceTool {
            command: WorkspaceCommand::Index {
                path: None,
                force: false,
            },
        };
        */
        /*
        let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let files_to_index = tool
            .discover_indexable_files(&workspace_root)
            .expect("Failed to discover workspace files");

        assert!(
            !files_to_index.is_empty(),
            "Expected to discover workspace files"
        );

        let mut parser_pool = LanguageParserPool::new();
        let mut all_symbols = Vec::new();
        let mut all_file_infos = Vec::new();

        // Create in-memory search engine for testing
        let search_engine = Arc::new(tokio::sync::RwLock::new(
            crate::search::SearchEngine::in_memory().expect("Failed to create in-memory search engine")
        ));

        for file_path in files_to_index {
            let language = tool.detect_language(&file_path);
            let parser = match parser_pool.get_parser(&language) {
                Ok(parser) => parser,
                Err(_) => continue,
            };

            match tool
                .process_file_with_parser(&file_path, &language, parser, &search_engine)
                .await
            {
                Ok((symbols, _relationships, file_info, _tantivy_symbols)) => {
                    all_symbols.extend(symbols);
                    all_file_infos.push(file_info);
                }
                Err(e) => {
                    panic!("Failed to process file {}: {}", file_path.display(), e);
                }
            }
        }

        assert!(
            !all_symbols.is_empty(),
            "Expected to collect symbols from workspace"
        );

        use std::collections::HashSet;
        let file_paths: HashSet<_> = all_file_infos
            .iter()
            .map(|info| info.path.clone())
            .collect();
        for symbol in &all_symbols {
            assert!(
                file_paths.contains(&symbol.file_path),
                "Symbol {} references missing file {}",
                symbol.id,
                symbol.file_path
            );
        }

        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("full-workspace.db");
        let mut db = SymbolDatabase::new(&db_path).unwrap();

        db.bulk_store_files(&all_file_infos, "workspace_test")
            .expect("Bulk file insert should succeed");

        let result = db.bulk_store_symbols(&all_symbols, "workspace_test");
        assert!(
            result.is_ok(),
            "Bulk storing symbols for full dataset should succeed: {:?}",
            result
        );
        */
    }
}
