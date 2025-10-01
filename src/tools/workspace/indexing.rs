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
        let current_dir = std::env::current_dir().unwrap_or_default();
        let is_primary_workspace = workspace_path == current_dir;

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
        let all_discovered_files = self.discover_indexable_files(workspace_path)?;

        info!(
            "📊 Discovered {} files total after filtering",
            all_discovered_files.len()
        );

        // 🚀 INCREMENTAL UPDATE: Filter files that need re-indexing based on hash changes
        let files_to_index = if force_reindex {
            debug!("Force reindex mode - processing all {} files", all_discovered_files.len());
            all_discovered_files
        } else {
            self.filter_changed_files(handler, all_discovered_files, workspace_path).await?
        };

        info!(
            "⚡ Need to process {} files (incremental filtering applied)",
            files_to_index.len()
        );

        // Get workspace ID early for use throughout the function
        // CRITICAL: Ensure workspace is properly registered before indexing
        let workspace_id = if let Some(workspace) = handler.get_workspace().await? {
            let registry_service =
                crate::workspace::registry_service::WorkspaceRegistryService::new(
                    workspace.root.clone(),
                );

            // Look up the workspace ID by path (works for both primary and reference workspaces)
            let workspace_path_str = workspace_path.to_string_lossy().to_string();
            if let Some(workspace_entry) = registry_service.get_workspace_by_path(&workspace_path_str).await? {
                debug!("Using existing workspace ID: {} for path: {}", workspace_entry.id, workspace_path_str);
                workspace_entry.id
            } else {
                // Register workspace if not registered yet
                let workspace_path_str = workspace.root.to_string_lossy().to_string();
                let entry = registry_service
                    .register_workspace(workspace_path_str, crate::workspace::registry::WorkspaceType::Primary)
                    .await?;
                info!("🏷️ Registered primary workspace with ID: {}", entry.id);
                entry.id
            }
        } else {
            return Err(anyhow::anyhow!("No workspace available for indexing"));
        };

        // Get SearchEngine for single-pass indexing (Tantivy + SQLite together)
        match handler.active_search_engine().await {
            Ok(search_engine) => {
                // PERFORMANCE OPTIMIZATION: Group files by language and use parser pool for 10-50x speedup
                self.process_files_optimized(
                    handler,
                    files_to_index,
                    is_primary_workspace,
                    &mut total_files,
                    search_engine,
                    workspace_id.clone(),  // Pass workspace_id to avoid re-lookup
                )
                .await?;
            }
            Err(e) => {
                debug!("Search engine unavailable during indexing: {}", e);
                // For now, return error since indexing requires search engine
                // TODO: Implement fallback indexing without search engine
                return Err(e);
            }
        }

        // 🚀 NEW ARCHITECTURE: Get final counts from DATABASE, not memory!
        let (total_symbols, total_relationships) =
            if let Some(workspace) = handler.get_workspace().await? {
                if let Some(db_arc) = &workspace.db {
                    let db = db_arc.lock().await;
                    let symbols_count = db
                        .get_symbol_count_for_workspace(&workspace_id)
                        .unwrap_or(0);
                    // Debug output removed to prevent stdio flooding
                    let stats = db.get_stats().unwrap_or_default();
                    (symbols_count as usize, stats.total_relationships as usize)
                } else {
                    (0, 0)
                }
            } else {
                (0, 0)
            };

        info!(
            "✅ Indexing complete: {} symbols, {} relationships stored in SQLite",
            total_symbols, total_relationships
        );

        // 🔥 BACKGROUND TASK: Generate embeddings from SQLite (optional, compute-intensive)
        if is_primary_workspace && total_symbols > 0 {
            info!("🚀 Starting background embedding generation from SQLite...");

            // Clone necessary references for background task
            let embedding_engine = handler.embedding_engine.clone();
            let workspace = handler.get_workspace().await?;
            let workspace_db = workspace.as_ref().and_then(|ws| ws.db.clone());
            let workspace_root = workspace.as_ref().map(|ws| ws.root.clone());
            let workspace_id_clone = workspace_id.clone();
            let indexing_status_clone = handler.indexing_status.clone();

            tokio::spawn(async move {
                let task_start = std::time::Instant::now();
                match generate_embeddings_from_sqlite(
                    embedding_engine,
                    workspace_db,
                    workspace_root,
                    workspace_id_clone,
                    indexing_status_clone,
                )
                .await
                {
                    Ok(_) => {
                        info!("✅ Embeddings generated from SQLite in {:.2}s - semantic search available!",
                              task_start.elapsed().as_secs_f64());
                    }
                    Err(e) => {
                        error!("❌ Background embedding generation failed: {}", e);
                    }
                }
            });
        } else if !is_primary_workspace {
            info!(
                "📦 Reference workspace indexed - symbols stored in database for targeted search"
            );
        }

        Ok((total_symbols, total_files, total_relationships))
    }

    /// 🚀 SINGLE-PASS PROCESSING: Index in Tantivy + SQLite simultaneously
    /// This eliminates redundant file reads and provides immediate search availability
    async fn process_files_optimized(
        &self,
        handler: &JulieServerHandler,
        files_to_index: Vec<PathBuf>,
        is_primary_workspace: bool,
        total_files: &mut usize,
        search_engine: Arc<tokio::sync::RwLock<crate::search::SearchEngine>>,
        workspace_id: String,  // Pass workspace_id instead of re-looking it up
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

        // 🔥 COLLECT ALL DATA FIRST for bulk operations
        let mut all_symbols = Vec::new();
        let mut all_relationships = Vec::new();
        let mut all_file_infos = Vec::new();
        let mut files_to_clean = Vec::new(); // Track files that need cleanup before re-indexing
        let mut all_tantivy_symbols = Vec::new(); // Collect ALL symbols for single Tantivy transaction

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
                            .process_file_with_parser(&file_path, &language, parser, &search_engine)
                            .await
                        {
                    Ok((symbols, relationships, file_info, tantivy_symbols)) => {
                        *total_files += 1;

                        // Per-file processing details at trace level
                        trace!(
                            "File {} extracted {} symbols, {} for Tantivy",
                            file_path.display(),
                            symbols.len(),
                            tantivy_symbols.len()
                        );

                        // Track this file for cleanup (remove old symbols/data before adding new)
                        files_to_clean.push(file_path.to_string_lossy().to_string());

                        // Collect data for bulk storage
                        all_symbols.extend(symbols);
                        all_relationships.extend(relationships);
                        all_file_infos.push(file_info);
                        all_tantivy_symbols.extend(tantivy_symbols); // Collect for single big transaction

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
            debug!("No parser for {} ({}) - indexing {} files for text search only", language, e, file_paths.len());
            for file_path in file_paths {
                match self.process_file_without_parser(&file_path, &language).await {
                    Ok((symbols, relationships, file_info, tantivy_symbols)) => {
                        debug!("📄 Processed file without parser: {:?} - created {} tantivy symbols", file_path, tantivy_symbols.len());
                        for sym in &tantivy_symbols {
                            debug!("  └─ Symbol: {} (kind: {:?}, code_context length: {})",
                                sym.name, sym.kind, sym.code_context.as_ref().map(|c| c.len()).unwrap_or(0));
                        }
                        *total_files += 1;
                        files_to_clean.push(file_path.to_string_lossy().to_string());
                        all_symbols.extend(symbols); // Will be empty
                        all_relationships.extend(relationships); // Will be empty
                        all_file_infos.push(file_info);
                        all_tantivy_symbols.extend(tantivy_symbols); // File content for text search
                    }
                    Err(e) => {
                        warn!("Failed to process file without parser {:?}: {}", file_path, e);
                    }
                }
            }
        }
    }
}

        // 🧹 CLEANUP: Remove old data for files being re-processed (incremental updates)
        // Run cleanup if we have files to clean AND either extracted symbols OR tantivy symbols
        if !files_to_clean.is_empty() && (!all_symbols.is_empty() || !all_tantivy_symbols.is_empty()) {
            debug!("Cleaning up old data for {} modified files before bulk storage", files_to_clean.len());

            if let Some(workspace) = handler.get_workspace().await? {
                if let Some(db) = &workspace.db {
                    let db_lock = db.lock().await;

                    // Use the workspace_id that was passed in (already validated)

                    // Clean up database entries for modified files
                    for file_path in &files_to_clean {
                        if let Err(e) = db_lock.delete_symbols_for_file_in_workspace(file_path, &workspace_id) {
                            warn!("Failed to delete old symbols for {}: {}", file_path, e);
                        }
                        if let Err(e) = db_lock.delete_relationships_for_file(file_path, &workspace_id) {
                            warn!("Failed to delete old relationships for {}: {}", file_path, e);
                        }
                    }

                    // Clean up Tantivy entries for modified files
                    {
                        let mut search_engine_lock = search_engine.write().await;
                        for file_path in &files_to_clean {
                            if let Err(e) = search_engine_lock.delete_file_symbols(file_path).await {
                                warn!("Failed to delete old Tantivy entries for {}: {}", file_path, e);
                            }
                        }
                    }

                    debug!("Cleanup complete for {} files", files_to_clean.len());
                }
            }
        }

        // 🚀 BLAZING-FAST BULK STORAGE: Store everything at once using optimized bulk methods
        if !all_symbols.is_empty() {
            info!("🚀 Starting blazing-fast bulk storage of {} symbols, {} relationships, {} files...",
                  all_symbols.len(), all_relationships.len(), all_file_infos.len());

            if let Some(workspace) = handler.get_workspace().await? {
                if let Some(db) = &workspace.db {
                    let mut db_lock = db.lock().await;

                    // Use the workspace_id that was passed in (already validated and registered)

                    // 🔥 BULK OPERATIONS for maximum speed
                    let bulk_start = std::time::Instant::now();
                    // Debug output removed

                    // Bulk store files
                    if let Err(e) = db_lock.bulk_store_files(&all_file_infos, &workspace_id) {
                        warn!("Failed to bulk store files: {}", e);
                    } else {
                        // Debug output removed
                    }

                    // Bulk store symbols (with index dropping optimization!)
                    // Debug output removed
                    if let Err(e) = db_lock.bulk_store_symbols(&all_symbols, &workspace_id) {
                        warn!("Failed to bulk store symbols: {}", e);
                    } else {
                        // Debug output removed
                    }

                    // Bulk store relationships
                    if let Err(e) =
                        db_lock.bulk_store_relationships(&all_relationships, &workspace_id)
                    {
                        warn!("Failed to bulk store relationships: {}", e);
                    }

                    let bulk_duration = bulk_start.elapsed();
                    info!(
                        "✅ Bulk storage complete in {:.2}s - data now persisted in SQLite!",
                        bulk_duration.as_secs_f64()
                    );

                    // CASCADE: Mark SQLite FTS5 as ready
                    handler.indexing_status.sqlite_fts_ready.store(true, std::sync::atomic::Ordering::Release);
                    debug!("🔍 CASCADE: SQLite FTS5 search now available");
                }
            }

            // 🔥 CASCADE: Launch background Tantivy indexing from SQLite
            info!("🚀 CASCADE: SQLite storage complete - launching background Tantivy indexing...");

            // Clone necessary references for background task
            let search_engine_clone = search_engine.clone();
            let workspace_db = handler.get_workspace().await?.and_then(|ws| ws.db.clone());
            let workspace_id_clone = workspace_id.clone();
            let indexing_status_clone = handler.indexing_status.clone();

            tokio::spawn(async move {
                match build_tantivy_from_sqlite(
                    search_engine_clone,
                    workspace_db,
                    workspace_id_clone,
                    indexing_status_clone,
                )
                .await
                {
                    Ok(_) => {
                        // Success message logged by build_tantivy_from_sqlite
                    }
                    Err(e) => {
                        error!("❌ CASCADE: Background Tantivy indexing failed: {}", e);
                    }
                }
            });

            // Store in handler memory for compatibility (primary workspace only)
            if is_primary_workspace {
                debug!(
                    "Storing {} symbols in memory for compatibility",
                    all_symbols.len()
                );
                // Symbols and relationships already persisted to SQLite database
                // No need for in-memory storage - all reads now query database directly
                debug!("Database storage complete");
            }
        }

        Ok(())
    }

    /// Process a single file with single-pass indexing (Tantivy + symbol extraction)
    /// Returns (symbols, relationships, file_info, tantivy_symbols) for bulk storage
    async fn process_file_with_parser(
        &self,
        file_path: &Path,
        language: &str,
        parser: &mut Parser,
        _search_engine: &Arc<tokio::sync::RwLock<crate::search::SearchEngine>>,
    ) -> Result<(Vec<Symbol>, Vec<Relationship>, crate::database::FileInfo, Vec<Symbol>)> {
        // Read file content ONCE for both Tantivy and symbol extraction
        let content = fs::read_to_string(file_path)
            .map_err(|e| anyhow::anyhow!("Failed to read file {:?}: {}", file_path, e))?;

        let file_path_str = file_path.to_string_lossy().to_string();

        // 🚀 COLLECT: Prepare full file content symbol for batch Tantivy indexing
        let mut tantivy_symbols = Vec::new();
        if !content.trim().is_empty() {
            let file_content_symbol = crate::extractors::Symbol {
                id: format!("file_content_{}", file_path_str.replace(['/', '\\'], "_")),
                name: format!("FILE_CONTENT_{}", std::path::Path::new(&file_path_str).file_name().unwrap_or_default().to_string_lossy()),
                kind: crate::extractors::SymbolKind::Module, // Use Module for files
                language: "text".to_string(), // Generic text for full content
                file_path: file_path_str.clone(),
                signature: Some(format!("Full content of {}", file_path_str)),
                doc_comment: None,
                code_context: Some(content.clone()), // Put full file content in code_context
                start_line: 1,
                end_line: content.lines().count() as u32,
                start_column: 1,
                end_column: 1,
                start_byte: 0,
                end_byte: content.len() as u32,
                visibility: None,
                parent_id: None,
                metadata: None,
                semantic_group: Some("file_content".to_string()),
                confidence: Some(1.0),
            };

            // Collect file content symbol for batch indexing (no immediate commit)
            tantivy_symbols.push(file_content_symbol);
        }

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
                    content: Some(String::new()), // CASCADE: Empty content
                },
                tantivy_symbols, // Return empty tantivy symbols for empty files
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

            // Return file info and tantivy symbols (for text search), but no extracted symbols
            return Ok((Vec::new(), Vec::new(), file_info, tantivy_symbols));
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

        // 🚀 COLLECT: Add extracted symbols to Tantivy batch (no immediate commit)
        if !symbols.is_empty() {
            tantivy_symbols.extend(symbols.clone());
        }

        // Only log if there are many symbols to avoid spam
        if symbols.len() > 10 {
            debug!(
                "📊 Extracted {} symbols from {}",
                symbols.len(),
                file_path_str
            );
        }

        // Return data for bulk operations (SQLite + Tantivy batch)
        Ok((symbols, relationships, file_info, tantivy_symbols))
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
        // Get workspace ID for database queries
        let workspace_id = if let Some(workspace) = handler.get_workspace().await? {
            let registry_service =
                crate::workspace::registry_service::WorkspaceRegistryService::new(
                    workspace.root.clone(),
                );

            // Look up the workspace ID by path (works for both primary and reference workspaces)
            let workspace_path_str = workspace_path.to_string_lossy().to_string();
            if let Some(workspace_entry) = registry_service.get_workspace_by_path(&workspace_path_str).await? {
                workspace_entry.id
            } else {
                // No workspace registered yet - all files are new
                debug!("No existing workspace found - indexing all {} files", all_files.len());
                return Ok(all_files);
            }
        } else {
            // No workspace available - all files are new
            return Ok(all_files);
        };

        // Get database to check existing file hashes
        let existing_file_hashes = if let Some(workspace) = handler.get_workspace().await? {
            if let Some(db) = &workspace.db {
                let db_lock = db.lock().await;
                match db_lock.get_file_hashes_for_workspace(&workspace_id) {
                    Ok(hashes) => hashes,
                    Err(e) => {
                        warn!("Failed to get existing file hashes: {} - treating all files as new", e);
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

        debug!("Checking {} files against {} existing file hashes", all_files.len(), existing_file_hashes.len());

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
                    warn!("Failed to calculate hash for {}: {} - including for re-indexing", file_path_str, e);
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
                        "text" | "json" | "toml" | "yaml" | "yml" | "xml" | "markdown" | "md" | "txt" | "config"
                    );

                    if needs_file_content {
                        // Check if it has symbols (should be 0 for files without parsers)
                        if let Some(workspace) = handler.get_workspace().await? {
                            if let Some(db) = &workspace.db {
                                let db_lock = db.lock().await;
                                let symbol_count = db_lock.get_file_symbol_count(&file_path_str).unwrap_or(0);
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
        let orphaned_count = self.clean_orphaned_files(
            handler,
            &existing_file_hashes,
            &all_files,
            &workspace_id
        ).await?;

        if orphaned_count > 0 {
            info!("🧹 Cleaned up {} orphaned file entries from database", orphaned_count);
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
            let db_lock = db.lock().await;

            for file_path in &orphaned_files {
                // Delete relationships first (referential integrity)
                if let Err(e) = db_lock.delete_relationships_for_file(file_path, workspace_id) {
                    warn!("Failed to delete relationships for orphaned file {}: {}", file_path, e);
                    continue;
                }

                // Delete symbols
                if let Err(e) = db_lock.delete_symbols_for_file_in_workspace(file_path, workspace_id) {
                    warn!("Failed to delete symbols for orphaned file {}: {}", file_path, e);
                    continue;
                }

                // Delete file record
                if let Err(e) = db_lock.delete_file_record_in_workspace(file_path, workspace_id) {
                    warn!("Failed to delete file record for orphaned file {}: {}", file_path, e);
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

    /// Process a file without a tree-sitter parser (text indexing only, no symbol extraction)
    /// This ensures ALL non-binary files are searchable, even without language extractors
    async fn process_file_without_parser(
        &self,
        file_path: &Path,
        language: &str,
    ) -> Result<(Vec<Symbol>, Vec<Relationship>, crate::database::FileInfo, Vec<Symbol>)> {
        trace!("Processing file without parser: {:?} (language: {})", file_path, language);

        // Read file content for text indexing
        let content = fs::read_to_string(file_path)
            .map_err(|e| anyhow::anyhow!("Failed to read file {:?}: {}", file_path, e))?;

        trace!("Read {} bytes from {:?}", content.len(), file_path);

        let file_path_str = file_path.to_string_lossy().to_string();

        // Create file_content_symbol for Tantivy text search
        let mut tantivy_symbols = Vec::new();
        if !content.trim().is_empty() {
            let file_content_symbol = crate::extractors::Symbol {
                id: format!("file_content_{}", file_path_str.replace(['/', '\\'], "_")),
                name: format!("FILE_CONTENT_{}", std::path::Path::new(&file_path_str).file_name().unwrap_or_default().to_string_lossy()),
                kind: crate::extractors::SymbolKind::Module,
                language: "text".to_string(), // Generic text for full content
                file_path: file_path_str.clone(),
                signature: Some(format!("Full content of {}", file_path_str)),
                doc_comment: None,
                code_context: Some(content.clone()), // Full file content for text search
                start_line: 1,
                end_line: content.lines().count() as u32,
                start_column: 1,
                end_column: 1,
                start_byte: 0,
                end_byte: content.len() as u32,
                visibility: None,
                parent_id: None,
                metadata: None,
                semantic_group: Some("file_content".to_string()),
                confidence: Some(1.0),
            };
            trace!("Created FILE_CONTENT symbol: {} for {:?}", file_content_symbol.name, file_path);
            tantivy_symbols.push(file_content_symbol);
        } else {
            warn!("⚠️  File {:?} has empty content, skipping FILE_CONTENT symbol creation", file_path);
        }

        // Calculate file info for database storage
        let file_info = crate::database::create_file_info(&file_path_str, language)?;

        trace!("Returning from process_file_without_parser: {} tantivy_symbols", tantivy_symbols.len());

        // Return: FILE_CONTENT symbols for BOTH database storage AND Tantivy indexing
        // Clone is necessary because both database and Tantivy need the symbols
        let database_symbols = tantivy_symbols.clone();
        Ok((database_symbols, Vec::new(), file_info, tantivy_symbols))
    }
}

/// 🔥 CASCADE BACKGROUND TASK: Build Tantivy index from SQLite database
/// This runs asynchronously after SQLite storage completes, enabling fast startup
async fn build_tantivy_from_sqlite(
    search_engine: Arc<tokio::sync::RwLock<crate::search::SearchEngine>>,
    workspace_db: Option<Arc<tokio::sync::Mutex<crate::database::SymbolDatabase>>>,
    workspace_id: String,
    indexing_status: Arc<crate::handler::IndexingStatus>,
) -> Result<()> {
    use anyhow::Context;

    let start_time = std::time::Instant::now();
    debug!("CASCADE: Building Tantivy index from SQLite");

    // Get database connection
    let db = match workspace_db {
        Some(db_arc) => db_arc,
        None => {
            warn!("No database available for Tantivy indexing");
            return Ok(());
        }
    };

    // 1. Pull symbols from SQLite
    let symbols = {
        let db_lock = db.lock().await;
        db_lock
            .get_symbols_for_workspace(&workspace_id)
            .context("Failed to read symbols from database")?
    };

    // 2. Pull file contents from SQLite (for FILE_CONTENT symbols)
    let file_contents = {
        let db_lock = db.lock().await;
        db_lock
            .get_all_file_contents(&workspace_id)
            .context("Failed to read file contents from database")?
    };

    if symbols.is_empty() && file_contents.is_empty() {
        debug!("No symbols or files to index in Tantivy");
        return Ok(());
    }

    debug!(
        "Indexing {} symbols + {} file contents into Tantivy",
        symbols.len(),
        file_contents.len()
    );

    // 3. Create FILE_CONTENT symbols from file contents
    let mut file_content_symbols = Vec::new();
    for (path, content) in file_contents {
        let file_content_symbol = crate::extractors::Symbol {
            id: format!("file_content_{}", path.replace(['/', '\\'], "_")),
            name: format!("FILE_CONTENT_{}", std::path::Path::new(&path).file_name().unwrap_or_default().to_string_lossy()),
            kind: crate::extractors::SymbolKind::Module, // Use Module for files
            language: "text".to_string(), // Generic text for full content
            file_path: path.clone(),
            signature: Some(format!("Full content of {}", path)),
            doc_comment: None,
            code_context: Some(content.clone()), // Put full file content in code_context
            start_line: 1,
            end_line: content.lines().count() as u32,
            start_column: 1,
            end_column: 1,
            start_byte: 0,
            end_byte: content.len() as u32,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: Some("file_content".to_string()),
            confidence: Some(1.0),
        };

        file_content_symbols.push(file_content_symbol);
    }

    debug!("Created {} FILE_CONTENT symbols", file_content_symbols.len());

    // 4. Combine all symbols
    let mut all_symbols = symbols;
    all_symbols.extend(file_content_symbols);

    debug!("Total symbols for Tantivy: {}", all_symbols.len());

    // 5. Index into Tantivy
    {
        let mut search_engine_lock = search_engine.write().await;
        search_engine_lock
            .index_symbols(all_symbols)
            .await
            .context("Failed to index symbols in Tantivy")?;
    }

    let duration = start_time.elapsed();
    info!(
        "✅ CASCADE: Tantivy index built from SQLite in {:.2}s - advanced search available!",
        duration.as_secs_f64()
    );

    // CASCADE: Mark Tantivy as ready
    indexing_status.tantivy_ready.store(true, std::sync::atomic::Ordering::Release);
    debug!("🚀 CASCADE: Tantivy search now available");

    Ok(())
}

/// 🔥 BACKGROUND TASK: Generate embeddings from SQLite database
/// This runs asynchronously to provide fast indexing response times
async fn generate_embeddings_from_sqlite(
    embedding_engine: Arc<tokio::sync::RwLock<Option<crate::embeddings::EmbeddingEngine>>>,
    workspace_db: Option<Arc<tokio::sync::Mutex<crate::database::SymbolDatabase>>>,
    workspace_root: Option<std::path::PathBuf>,
    workspace_id: String,
    indexing_status: Arc<crate::handler::IndexingStatus>,
) -> Result<()> {
    use anyhow::Context;

    let start_time = std::time::Instant::now();
    debug!("Starting embedding generation from SQLite");

    // Update registry status to "Generating"
    if let Some(ref root) = workspace_root {
        let registry_service = crate::workspace::registry_service::WorkspaceRegistryService::new(root.clone());
        if let Err(e) = registry_service
            .update_embedding_status(&workspace_id, crate::workspace::registry::EmbeddingStatus::Generating)
            .await
        {
            warn!("Failed to update embedding status to Generating: {}", e);
        }
    }

    // Get database connection
    let db = match workspace_db {
        Some(db_arc) => db_arc,
        None => {
            warn!("No database available for embedding generation");
            return Ok(());
        }
    };

    // Read symbols from SQLite
    let symbols = {
        let db_lock = db.lock().await;
        db_lock
            .get_symbols_for_workspace(&workspace_id)
            .context("Failed to read symbols from database")?
    };

    if symbols.is_empty() {
        debug!("No symbols to embed");
        return Ok(());
    }

    info!("🧠 Generating embeddings for {} symbols...", symbols.len());

    // Initialize embedding engine if needed
    {
        let mut embedding_guard = embedding_engine.write().await;
        if embedding_guard.is_none() {
            info!("🔧 Initializing embedding engine for background generation...");

            // 🔧 FIX: Use workspace .julie/cache directory instead of polluting CWD
            let cache_dir = if let Some(ref root) = workspace_root {
                root.join(".julie").join("cache").join("embeddings")
            } else {
                // Fallback to temp directory if workspace root not available
                std::env::temp_dir().join("julie_cache").join("embeddings")
            };

            std::fs::create_dir_all(&cache_dir)?;
            info!("📁 Using embedding cache directory: {}", cache_dir.display());

            match crate::embeddings::EmbeddingEngine::new(
                "bge-small",
                cache_dir,
                db.clone(),
            ) {
                Ok(engine) => {
                    *embedding_guard = Some(engine);
                    info!("✅ Embedding engine initialized for background task");
                }
                Err(e) => {
                    error!("❌ Failed to initialize embedding engine: {}", e);
                    return Err(anyhow::anyhow!(
                        "Embedding engine initialization failed: {}",
                        e
                    ));
                }
            }
        }
    }

    // Generate embeddings in batches
    {
        let mut embedding_guard = embedding_engine.write().await;
        if let Some(ref mut engine) = embedding_guard.as_mut() {
            const BATCH_SIZE: usize = 100;
            let total_batches = symbols.len().div_ceil(BATCH_SIZE);

            for (batch_idx, chunk) in symbols.chunks(BATCH_SIZE).enumerate() {
                info!(
                    "🔄 Processing embedding batch {}/{} ({} symbols)",
                    batch_idx + 1,
                    total_batches,
                    chunk.len()
                );

                match engine.embed_symbols_batch(chunk) {
                    Ok(batch_embeddings) => {
                        // Persist embeddings to database
                        {
                            let db_guard = db.lock().await;
                            let model_name = engine.model_name();
                            let dimensions = engine.dimensions();

                            for (symbol_id, embedding) in batch_embeddings {
                                // Store the vector data
                                if let Err(e) = db_guard.store_embedding_vector(
                                    &symbol_id,
                                    &embedding,
                                    dimensions,
                                    model_name,
                                ) {
                                    warn!("Failed to persist vector for {}: {}", symbol_id, e);
                                }

                                // Store the metadata linking symbol to vector
                                if let Err(e) = db_guard.store_embedding_metadata(
                                    &symbol_id,
                                    &symbol_id,  // Using symbol_id as vector_id
                                    model_name,
                                    None,  // embedding_hash not computed
                                ) {
                                    warn!("Failed to persist embedding metadata for {}: {}", symbol_id, e);
                                }
                            }
                        }

                        debug!(
                            "✅ Generated and stored embeddings for batch {}/{}",
                            batch_idx + 1,
                            total_batches
                        );
                    }
                    Err(e) => {
                        warn!(
                            "⚠️ Failed to generate embeddings for batch {}: {}",
                            batch_idx + 1,
                            e
                        );
                        // Continue with next batch rather than failing completely
                    }
                }
            }
        } else {
            return Err(anyhow::anyhow!("Embedding engine not available"));
        }
    }

    let duration = start_time.elapsed();
    info!(
        "✅ Embedding generation complete in {:.2}s",
        duration.as_secs_f64()
    );

    // 🏗️ BUILD AND SAVE HNSW INDEX
    info!("🏗️ Building HNSW index from fresh embeddings...");
    let hnsw_start = std::time::Instant::now();

    let mut vector_store = crate::embeddings::vector_store::VectorStore::new(384)?;

    {
        let db_lock = db.lock().await;
        match db_lock.load_all_embeddings("bge-small") {
            Ok(embeddings) => {
                let count = embeddings.len();
                info!("📥 Loading {} embeddings for HNSW", count);

                for (symbol_id, vector) in embeddings {
                    if let Err(e) = vector_store.store_vector(symbol_id.clone(), vector) {
                        warn!("Failed to store vector {}: {}", symbol_id, e);
                    }
                }

                // Build HNSW index
                match vector_store.build_hnsw_index() {
                    Ok(_) => {
                        info!("✅ HNSW index built in {:.2}s", hnsw_start.elapsed().as_secs_f64());

                        // Save to disk for lazy loading on next startup
                        let vectors_path = std::path::PathBuf::from("./.julie/vectors");
                        if let Err(e) = vector_store.save_hnsw_index(&vectors_path) {
                            warn!("Failed to save HNSW index to disk: {}", e);
                        } else {
                            info!("💾 HNSW index saved to vectors/ for fast loading");
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
    }

    info!("✅ Background task complete - semantic search ready via lazy loading!");

    // CASCADE: Mark semantic search as ready
    indexing_status.semantic_ready.store(true, std::sync::atomic::Ordering::Release);
    debug!("🧠 CASCADE: Semantic search now available");

    // Update registry status to "Ready"
    if let Some(ref root) = workspace_root {
        let registry_service = crate::workspace::registry_service::WorkspaceRegistryService::new(root.clone());
        if let Err(e) = registry_service
            .update_embedding_status(&workspace_id, crate::workspace::registry::EmbeddingStatus::Ready)
            .await
        {
            warn!("Failed to update embedding status to Ready: {}", e);
        } else {
            debug!("📝 Updated registry: embedding_status = Ready");
        }
    }

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
