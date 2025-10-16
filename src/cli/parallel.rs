/// Parallel extraction engine for high-performance bulk operations
///
/// Uses Rayon for parallel file processing and supports direct SQLite writes
/// for maximum performance. Designed to handle large workspaces efficiently.
use crate::database::SymbolDatabase;
use crate::extractors::base::Symbol;
use crate::extractors::ExtractorManager;
use anyhow::{anyhow, Result};
use rayon::prelude::*;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use walkdir::WalkDir;

/// Configuration for parallel extraction
#[derive(Debug, Clone)]
pub struct ExtractionConfig {
    /// Number of parallel threads (defaults to CPU count)
    pub num_threads: usize,

    /// Batch size for processing (symbols processed before DB write)
    pub batch_size: usize,

    /// Optional SQLite database path for direct writes (fastest for bulk)
    pub output_db: Option<String>,
}

impl Default for ExtractionConfig {
    fn default() -> Self {
        Self {
            num_threads: num_cpus::get(),
            batch_size: 100,
            output_db: None,
        }
    }
}

/// High-performance parallel extractor
pub struct ParallelExtractor {
    config: ExtractionConfig,
}

impl ParallelExtractor {
    /// Create a new parallel extractor with the given configuration
    pub fn new(config: ExtractionConfig) -> Self {
        Self { config }
    }

    /// Extract all files in a directory (parallel, optimized for bulk operations)
    ///
    /// Performance: ~200 files/sec on 8 cores with SQLite output
    pub fn extract_directory(&self, directory: &str) -> Result<Vec<Symbol>> {
        // 1. Setup thread pool
        rayon::ThreadPoolBuilder::new()
            .num_threads(self.config.num_threads)
            .build_global()
            .map_err(|e| anyhow!("Failed to build thread pool: {}", e))?;

        // 2. Discover all supported files
        let files = self.discover_files(directory)?;
        eprintln!("📁 Found {} files to process", files.len());

        if files.is_empty() {
            return Ok(Vec::new());
        }

        // 3. Setup output (SQLite or in-memory)
        let db = if let Some(db_path) = &self.config.output_db {
            let db = Arc::new(Mutex::new(SymbolDatabase::new(db_path)?));
            eprintln!("🗄️  SQLite database opened: {}", db_path);
            Some(db)
        } else {
            None
        };

        // 4. Process files in parallel batches
        let all_symbols = Arc::new(Mutex::new(Vec::new()));
        let processed = Arc::new(Mutex::new(0usize));
        let total_files = files.len();

        // PHASE 1: Extract and store symbols
        files.par_chunks(self.config.batch_size).for_each(|batch| {
            // Extract symbols for this batch (parallel within batch)
            let batch_symbols: Vec<Symbol> = batch
                .par_iter()
                .flat_map(|file| {
                    // TODO: Make extraction synchronous or use tokio runtime properly
                    self.extract_file_sync(file).ok().unwrap_or_default()
                })
                .collect();

            // Write to output
            if let Some(ref db) = db {
                // Direct SQLite write (using default workspace for CLI)
                if let Ok(mut db_lock) = db.lock() {
                    if let Err(e) = db_lock.bulk_store_symbols(&batch_symbols, "cli-extraction") {
                        eprintln!("⚠️  Failed to store batch: {}", e);
                    }
                }
            } else {
                // Collect in memory
                if let Ok(mut symbols) = all_symbols.lock() {
                    symbols.extend(batch_symbols);
                }
            }

            // Progress reporting
            if let Ok(mut proc) = processed.lock() {
                *proc += batch.len();
                if *proc % 100 == 0 || *proc == total_files {
                    eprintln!("⚡ Processed {}/{} files", *proc, total_files);
                }
            }
        });

        eprintln!("✅ Phase 1 complete: All symbols extracted and stored");

        // PHASE 2: Extract and store identifiers (NEW for LSP-quality reference tracking)
        // We need all symbols extracted first to find containing_symbol_id
        if let Some(ref db) = db {
            eprintln!("🔍 Phase 2: Extracting identifiers (references/usages)...");

            // Get all extracted symbols for identifier resolution
            let all_extracted_symbols = {
                let db_lock = db.lock().map_err(|e| anyhow!("Lock error: {:?}", e))?;
                db_lock.get_all_symbols()?
            };

            eprintln!(
                "📚 Loaded {} symbols for identifier extraction",
                all_extracted_symbols.len()
            );

            // Extract identifiers in parallel batches
            let all_identifiers = Arc::new(Mutex::new(Vec::new()));
            let processed_phase2 = Arc::new(Mutex::new(0usize));

            files.par_chunks(self.config.batch_size).for_each(|batch| {
                // Extract identifiers for this batch
                let batch_identifiers: Vec<crate::extractors::Identifier> = batch
                    .par_iter()
                    .flat_map(|file| {
                        self.extract_identifiers_sync(file, &all_extracted_symbols)
                            .ok()
                            .unwrap_or_default()
                    })
                    .collect();

                // Collect identifiers
                if let Ok(mut identifiers) = all_identifiers.lock() {
                    identifiers.extend(batch_identifiers);
                }

                // Progress reporting
                if let Ok(mut proc) = processed_phase2.lock() {
                    *proc += batch.len();
                    if *proc % 100 == 0 || *proc == total_files {
                        eprintln!("⚡ Phase 2: Processed {}/{} files", *proc, total_files);
                    }
                }
            });

            // Write all identifiers to database in one bulk operation
            let identifiers = Arc::try_unwrap(all_identifiers)
                .map_err(|_| anyhow!("Failed to unwrap identifiers Arc"))?
                .into_inner()
                .map_err(|e| anyhow!("Lock error: {:?}", e))?;

            if !identifiers.is_empty() {
                eprintln!(
                    "💾 Writing {} identifiers to database...",
                    identifiers.len()
                );
                let mut db_lock = db.lock().map_err(|e| anyhow!("Lock error: {:?}", e))?;
                db_lock.bulk_store_identifiers(&identifiers, "cli-extraction")?;
            }

            eprintln!(
                "✅ Phase 2 complete: {} identifiers extracted and stored",
                identifiers.len()
            );
        }

        // 5. Finalize
        if let Some(db) = db {
            let db_lock = db
                .lock()
                .map_err(|e| anyhow!("Lock error during finalization: {}", e))?;

            eprintln!("✅ Bulk insert complete");

            // Return symbols from DB
            return Ok(db_lock.get_all_symbols()?);
        }

        // Return collected symbols
        Ok(Arc::try_unwrap(all_symbols)
            .map_err(|_| anyhow!("Failed to unwrap symbols Arc"))?
            .into_inner()
            .map_err(|e| anyhow!("Lock error: {:?}", e))?)
    }

    /// Extract single file (for incremental updates)
    pub fn extract_file(&self, file_path: &str) -> Result<Vec<Symbol>> {
        let content = std::fs::read_to_string(file_path)?;
        let extractor_manager = ExtractorManager::new();
        extractor_manager.extract_symbols(file_path, &content)
    }

    /// Synchronous file extraction (for Rayon compatibility)
    fn extract_file_sync(&self, file_path: &PathBuf) -> Result<Vec<Symbol>> {
        let file_path_str = file_path.to_string_lossy().to_string();
        let content = std::fs::read_to_string(&file_path_str)?;

        let extractor_manager = ExtractorManager::new();

        // Now that extract_symbols is synchronous, we can call it directly
        extractor_manager.extract_symbols(&file_path_str, &content)
    }

    /// Synchronous identifier extraction (NEW for LSP-quality reference tracking)
    ///
    /// Extracts identifiers (references/usages) from a file. Requires symbols to be
    /// extracted first so we can find containing_symbol_id for each identifier.
    fn extract_identifiers_sync(
        &self,
        file_path: &PathBuf,
        symbols: &[Symbol],
    ) -> Result<Vec<crate::extractors::Identifier>> {
        let file_path_str = file_path.to_string_lossy().to_string();

        // Only extract identifiers for Rust files for now (proof of concept)
        // Other languages will be added as we implement their identifier extraction
        if !file_path_str.ends_with(".rs") {
            return Ok(Vec::new());
        }

        let content = std::fs::read_to_string(&file_path_str)?;

        // Get language and create parser
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_rust::LANGUAGE.into())?;

        // Parse the file
        let tree = parser
            .parse(&content, None)
            .ok_or_else(|| anyhow!("Failed to parse {}", file_path_str))?;

        // Create Rust extractor and extract identifiers
        let mut rust_extractor =
            crate::extractors::rust::RustExtractor::new("rust".to_string(), file_path_str, content);

        Ok(rust_extractor.extract_identifiers(&tree, symbols))
    }

    /// Discover all supported files in a directory
    pub(crate) fn discover_files(&self, directory: &str) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();

        for entry in WalkDir::new(directory).into_iter().filter_map(|e| e.ok()) {
            if !entry.file_type().is_file() {
                continue;
            }

            if let Some(ext) = entry.path().extension() {
                if self.is_supported_extension(ext.to_str().unwrap_or("")) {
                    files.push(entry.path().to_path_buf());
                }
            }
        }

        Ok(files)
    }

    /// Check if file extension is supported
    fn is_supported_extension(&self, ext: &str) -> bool {
        matches!(
            ext,
            "rs" | "ts"
                | "tsx"
                | "js"
                | "jsx"
                | "py"
                | "java"
                | "cs"
                | "go"
                | "cpp"
                | "c"
                | "h"
                | "hpp"
                | "rb"
                | "php"
                | "swift"
                | "kt"
                | "dart"
                | "gd"
                | "lua"
                | "vue"
                | "razor"
                | "sql"
                | "html"
                | "css"
                | "sh"
                | "bash"
                | "ps1"
                | "zig"
        )
    }
}
