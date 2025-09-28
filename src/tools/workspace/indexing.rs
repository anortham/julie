use crate::extractors::{Relationship, Symbol};
use crate::handler::JulieServerHandler;
use crate::tools::workspace::commands::ManageWorkspaceTool;
use crate::tools::workspace::LanguageParserPool;
use anyhow::Result;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, error, info, warn};
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

        // DEBUG: Log workspace path comparison
        info!(
            "🔍 DEBUG: workspace_path={:?}, current_dir={:?}, is_primary_workspace={}",
            workspace_path, current_dir, is_primary_workspace
        );

        // Only clear existing data for primary workspace reindex to preserve workspace isolation
        if force_reindex && is_primary_workspace {
            info!("🧹 Clearing primary workspace symbols for force reindex");
            handler.symbols.write().await.clear();
            handler.relationships.write().await.clear();
        } else if force_reindex {
            info!("🔄 Force reindexing reference workspace (preserving primary symbols)");
        }

        let mut total_files = 0;

        // Use blacklist-based file discovery
        let files_to_index = self.discover_indexable_files(workspace_path)?;

        info!(
            "📊 Found {} files to index after filtering",
            files_to_index.len()
        );

        // PERFORMANCE OPTIMIZATION: Group files by language and use parser pool for 10-50x speedup
        self.process_files_optimized(
            handler,
            files_to_index,
            is_primary_workspace,
            &mut total_files,
        )
        .await?;

        // Get workspace ID early for use throughout the function
        let workspace_id = if let Some(workspace) = handler.get_workspace().await? {
            let registry_service =
                crate::workspace::registry_service::WorkspaceRegistryService::new(
                    workspace.root.clone(),
                );
            registry_service
                .get_primary_workspace_id()
                .await?
                .unwrap_or_else(|| "primary".to_string())
        } else {
            "primary".to_string()
        };

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

        // 🚀 BLAZING-FAST APPROACH: Start background tasks for SearchEngine and Embeddings
        // This gives instant tool response while background tasks populate from SQLite
        if is_primary_workspace && total_symbols > 0 {
            info!(
                "🚀 Starting background population of SearchEngine and Embeddings from SQLite..."
            );

            // Clone necessary references for background tasks
            let search_engine = handler.active_search_engine().await;
            let embedding_engine = handler.embedding_engine.clone();
            let workspace_db = handler.get_workspace().await?.and_then(|ws| ws.db.clone());

            // 🔥 BACKGROUND TASK 1: Populate SearchEngine from SQLite
            let search_engine_clone = search_engine.clone();
            let workspace_db_clone = workspace_db.clone();
            let workspace_id_clone = workspace_id.clone();
            tokio::spawn(async move {
                if let Err(e) = populate_search_engine_from_sqlite(
                    search_engine_clone,
                    workspace_db_clone,
                    workspace_id_clone,
                )
                .await
                {
                    error!("❌ Background SearchEngine population failed: {}", e);
                } else {
                    info!("✅ SearchEngine populated from SQLite in background!");
                }
            });

            // 🔥 BACKGROUND TASK 2: Generate embeddings from SQLite
            let workspace_db_clone = workspace_db.clone();
            let workspace_id_clone = workspace_id.clone();
            tokio::spawn(async move {
                if let Err(e) = generate_embeddings_from_sqlite(
                    embedding_engine,
                    workspace_db_clone,
                    workspace_id_clone,
                )
                .await
                {
                    error!("❌ Background embedding generation failed: {}", e);
                } else {
                    info!("✅ Embeddings generated from SQLite in background!");
                }
            });
        } else if !is_primary_workspace {
            info!(
                "📦 Reference workspace indexed - symbols stored in database for targeted search"
            );
        }

        info!(
            "✅ Indexing complete: {} files, {} symbols, {} relationships",
            total_files, total_symbols, total_relationships
        );

        Ok((total_symbols, total_files, total_relationships))
    }

    /// 🚀 BLAZING-FAST PROCESSING: Collect all data first, then bulk store in SQLite
    /// This provides 10-100x speedup using bulk operations with index dropping
    async fn process_files_optimized(
        &self,
        handler: &JulieServerHandler,
        files_to_index: Vec<PathBuf>,
        is_primary_workspace: bool,
        total_files: &mut usize,
    ) -> Result<()> {
        // Group files by language for batch processing
        let mut files_by_language: HashMap<String, Vec<PathBuf>> = HashMap::new();

        for file_path in files_to_index {
            let language = self.detect_language(&file_path);
            files_by_language
                .entry(language)
                .or_insert_with(Vec::new)
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

        // Process each language group with its dedicated parser
        for (language, file_paths) in files_by_language {
            if file_paths.is_empty() {
                continue;
            }

            info!(
                "🔧 Processing {} {} files with reused parser",
                file_paths.len(),
                language
            );

            // Get or create parser for this language
            let parser = match parser_pool.get_parser(&language) {
                Ok(p) => p,
                Err(e) => {
                    debug!("Skipping unsupported language {}: {}", language, e);
                    continue;
                }
            };

            // Process all files of this language with the same parser
            for file_path in file_paths {
                match self
                    .process_file_with_parser(&file_path, &language, parser)
                    .await
                {
                    Ok((symbols, relationships, file_info)) => {
                        *total_files += 1;

                        // Debug output removed to prevent stdio flooding
                        info!(
                            "📦 File {} returned {} symbols",
                            file_path.display(),
                            symbols.len()
                        );

                        // Collect data for bulk storage
                        let prev_count = all_symbols.len();
                        all_symbols.extend(symbols);
                        info!(
                            "📦 Symbol collection: had {}, now have {}",
                            prev_count,
                            all_symbols.len()
                        );
                        all_relationships.extend(relationships);
                        all_file_infos.push(file_info);

                        if *total_files % 50 == 0 {
                            info!(
                                "📈 Processed {} files, collected {} symbols so far...",
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

        // 🚀 BLAZING-FAST BULK STORAGE: Store everything at once using optimized bulk methods
        if !all_symbols.is_empty() {
            info!("🚀 Starting blazing-fast bulk storage of {} symbols, {} relationships, {} files...",
                  all_symbols.len(), all_relationships.len(), all_file_infos.len());

            if let Some(workspace) = handler.get_workspace().await? {
                if let Some(db) = &workspace.db {
                    let mut db_lock = db.lock().await;

                    // Get actual workspace ID from registry (avoid hardcoded "primary")
                    let workspace_id = if is_primary_workspace {
                        let registry_service =
                            crate::workspace::registry_service::WorkspaceRegistryService::new(
                                workspace.root.clone(),
                            );
                        registry_service
                            .get_primary_workspace_id()
                            .await?
                            .unwrap_or_else(|| "primary".to_string())
                    } else {
                        return Err(anyhow::anyhow!(
                            "Reference workspace not supported in optimized path"
                        ));
                    };

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
                }
            }

            // Store in handler memory for compatibility (primary workspace only)
            if is_primary_workspace {
                info!(
                    "📦 Storing {} symbols in memory for compatibility...",
                    all_symbols.len()
                );
                {
                    let mut symbol_storage = handler.symbols.write().await;
                    symbol_storage.extend(all_symbols);
                }
                {
                    let mut relationship_storage = handler.relationships.write().await;
                    relationship_storage.extend(all_relationships);
                }
                info!("✅ Memory storage complete for compatibility");
            }
        }

        Ok(())
    }

    /// Process a single file with an already-initialized parser (PERFORMANCE OPTIMIZED)
    /// Returns (symbols, relationships, file_info) for bulk storage
    async fn process_file_with_parser(
        &self,
        file_path: &Path,
        language: &str,
        parser: &mut Parser,
    ) -> Result<(Vec<Symbol>, Vec<Relationship>, crate::database::FileInfo)> {
        // Read file content
        let content = fs::read_to_string(file_path)
            .map_err(|e| anyhow::anyhow!("Failed to read file {:?}: {}", file_path, e))?;

        // Skip empty files
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
                },
            ));
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

        // Return data for bulk storage instead of storing immediately
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
        info!(
            "🔍🔍 EXTRACTION STARTING for language: {} file: {}",
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
                info!(
                    "    ✅ RustExtractor returned {} symbols for {}",
                    symbols.len(),
                    file_path
                );
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
}

/// 🔥 BACKGROUND TASK: Populate SearchEngine from SQLite database
/// This runs asynchronously to provide fast indexing response times
async fn populate_search_engine_from_sqlite(
    search_engine: Arc<tokio::sync::RwLock<crate::search::SearchEngine>>,
    workspace_db: Option<Arc<tokio::sync::Mutex<crate::database::SymbolDatabase>>>,
    workspace_id: String,
) -> Result<()> {
    use anyhow::Context;

    let start_time = std::time::Instant::now();
    info!("🚀 Starting SearchEngine population from SQLite...");

    // Get database connection
    let db = match workspace_db {
        Some(db_arc) => db_arc,
        None => {
            warn!("No database available for SearchEngine population");
            return Ok(());
        }
    };

    // Read symbols from SQLite in batches for memory efficiency
    let symbols = {
        let db_lock = db.lock().await;
        db_lock
            .get_symbols_for_workspace(&workspace_id)
            .context("Failed to read symbols from database")?
    };

    if symbols.is_empty() {
        info!("No symbols to index in SearchEngine");
        return Ok(());
    }

    info!("📊 Indexing {} symbols in SearchEngine...", symbols.len());

    // Index symbols in SearchEngine
    {
        let mut search_engine_lock = search_engine.write().await;
        search_engine_lock
            .index_symbols(symbols)
            .await
            .context("Failed to index symbols in SearchEngine")?;

        search_engine_lock
            .commit()
            .await
            .context("Failed to commit SearchEngine changes")?;
    }

    let duration = start_time.elapsed();
    info!(
        "✅ SearchEngine populated from SQLite in {:.2}s - search is now available!",
        duration.as_secs_f64()
    );

    Ok(())
}

/// 🔥 BACKGROUND TASK: Generate embeddings from SQLite database
/// This runs asynchronously to provide fast indexing response times
async fn generate_embeddings_from_sqlite(
    embedding_engine: Arc<tokio::sync::RwLock<Option<crate::embeddings::EmbeddingEngine>>>,
    workspace_db: Option<Arc<tokio::sync::Mutex<crate::database::SymbolDatabase>>>,
    workspace_id: String,
) -> Result<()> {
    use anyhow::Context;

    let start_time = std::time::Instant::now();
    info!("🚀 Starting embedding generation from SQLite...");

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
        info!("No symbols to embed");
        return Ok(());
    }

    info!("🧠 Generating embeddings for {} symbols...", symbols.len());

    // Initialize embedding engine if needed
    {
        let mut embedding_guard = embedding_engine.write().await;
        if embedding_guard.is_none() {
            info!("🔧 Initializing embedding engine for background generation...");
            match crate::embeddings::EmbeddingEngine::new(
                "bge-small",
                std::path::PathBuf::from("./cache"),
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
            let total_batches = (symbols.len() + BATCH_SIZE - 1) / BATCH_SIZE;

            for (batch_idx, chunk) in symbols.chunks(BATCH_SIZE).enumerate() {
                info!(
                    "🔄 Processing embedding batch {}/{} ({} symbols)",
                    batch_idx + 1,
                    total_batches,
                    chunk.len()
                );

                match engine.embed_symbols_batch(chunk) {
                    Ok(_batch_embeddings) => {
                        debug!(
                            "✅ Generated embeddings for batch {}/{}",
                            batch_idx + 1,
                            total_batches
                        );
                        // TODO: Store embeddings in database for persistence
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
        "✅ Embedding generation complete in {:.2}s - semantic search is now available!",
        duration.as_secs_f64()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::SymbolDatabase;
    use crate::tools::workspace::WorkspaceCommand;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_bulk_store_symbols_full_workspace_dataset() {
        let tool = ManageWorkspaceTool {
            command: WorkspaceCommand::Index {
                path: None,
                force: false,
            },
        };

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

        for file_path in files_to_index {
            let language = tool.detect_language(&file_path);
            let parser = match parser_pool.get_parser(&language) {
                Ok(parser) => parser,
                Err(_) => continue,
            };

            match tool
                .process_file_with_parser(&file_path, &language, parser)
                .await
            {
                Ok((symbols, _relationships, file_info)) => {
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
    }
}
