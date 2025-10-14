/// julie-codesearch: Optimized code intelligence for CodeSearch MCP
///
/// Scans codebases, extracts symbols, stores file content and relationships in SQLite.
/// Designed for integration with CodeSearch MCP server for semantic code queries.
///
/// Commands:
/// - scan: Full directory scan with symbol extraction and content storage
/// - update: Incremental single-file update for FileWatcher integration
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use julie::database::SymbolDatabase;
use std::path::PathBuf;
use std::time::Instant;
use tracing::{debug, info};

#[derive(Parser)]
#[command(name = "julie-codesearch")]
#[command(about = "Optimized code intelligence for CodeSearch MCP", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Scan entire directory and build SQLite database
    Scan {
        /// Directory to scan recursively
        #[arg(short, long)]
        dir: PathBuf,

        /// SQLite database path
        #[arg(short = 'b', long)]
        db: PathBuf,

        /// Custom ignore patterns (comma-separated globs)
        #[arg(short, long)]
        ignore: Option<String>,

        /// Number of parallel threads (defaults to CPU count)
        #[arg(short, long)]
        threads: Option<usize>,

        /// Optional log file path for debug logging
        #[arg(short, long)]
        log: Option<PathBuf>,
    },

    /// Update single file (incremental)
    Update {
        /// File path to update
        #[arg(short, long)]
        file: PathBuf,

        /// SQLite database path
        #[arg(short = 'b', long)]
        db: PathBuf,

        /// Optional log file path for debug logging
        #[arg(short, long)]
        log: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Extract log path from commands
    let log_path = match &cli.command {
        Commands::Scan { log, .. } => log.clone(),
        Commands::Update { log, .. } => log.clone(),
    };

    // Initialize tracing with optional file logging
    init_logging(log_path.as_ref())?;

    match cli.command {
        Commands::Scan {
            dir,
            db,
            ignore,
            threads,
            log: _,
        } => scan_directory(dir, db, ignore, threads),
        Commands::Update { file, db, log: _ } => update_file(file, db),
    }
}

/// Initialize logging with optional file output
fn init_logging(log_path: Option<&PathBuf>) -> Result<()> {
    use tracing_subscriber::fmt::writer::MakeWriterExt;

    if let Some(log_file) = log_path {
        // With log file: info+ to file, warn+ to stderr
        let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

        let file_appender = tracing_appender::rolling::never(
            log_file
                .parent()
                .unwrap_or_else(|| std::path::Path::new(".")),
            log_file
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("julie-codesearch.log"),
        );

        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_writer(file_appender.and(std::io::stderr.with_max_level(tracing::Level::WARN)))
            .init();

        eprintln!("📝 Debug logging enabled: {:?}", log_file);
    } else {
        // No log file: warn+ to stderr only (unless RUST_LOG overrides)
        let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));

        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_writer(std::io::stderr)
            .init();
    }

    Ok(())
}

// ========================================
// Helper Functions
// ========================================

/// Result of processing a single file
struct FileResult {
    path: String,
    language: String,
    hash: String,
    size: u64,
    last_modified: u64,
    content: String,
    symbols: Vec<julie::extractors::base::Symbol>,
}

/// Discover all non-binary files in directory, excluding ignored patterns
/// Includes ALL text-based files (not just ones with Tree-sitter parsers)
fn discover_files(dir: &PathBuf, ignore_patterns: &[String]) -> Result<Vec<PathBuf>> {
    use walkdir::WalkDir;

    let mut files = Vec::new();

    for entry in WalkDir::new(dir)
        .follow_links(false) // Don't follow symlinks for safety
        .into_iter()
        .filter_entry(|e| !should_ignore_path(e.path(), ignore_patterns))
    {
        let entry = entry?;

        if !entry.file_type().is_file() {
            continue;
        }

        // Include file if:
        // 1. It has no extension (like Makefile, Dockerfile)
        // 2. It has a non-binary extension
        let should_include = match entry.path().extension() {
            None => true, // Include files without extensions
            Some(ext) => {
                let ext_str = ext.to_str().unwrap_or("");
                !is_binary_extension(ext_str)
            }
        };

        if should_include {
            files.push(entry.path().to_path_buf());
        }
    }

    Ok(files)
}

/// Check if a path should be ignored based on patterns
fn should_ignore_path(path: &std::path::Path, patterns: &[String]) -> bool {
    let path_str = path.to_string_lossy().replace('\\', "/");

    for pattern in patterns {
        // Handle glob patterns
        if pattern.contains('*') {
            // Convert glob pattern to simple matching
            // **/foo/** means "foo" anywhere in path
            // **/*.ext means files ending with .ext
            let pattern_clean = pattern.replace("**/", "").replace("/**", "");

            if pattern.starts_with("**/") && pattern.ends_with("/**") {
                // Directory pattern like **/node_modules/**
                if path_str.contains(&pattern_clean) {
                    return true;
                }
            } else if pattern.starts_with("**/") {
                // File pattern like **/*.log
                let ext_pattern = pattern_clean.trim_start_matches("*.");
                if path_str.ends_with(&format!(".{}", ext_pattern)) {
                    return true;
                }
            }
        } else {
            // Exact match
            if path_str.contains(pattern) {
                return true;
            }
        }
    }

    false
}

/// Check if file extension is binary (should be excluded from indexing)
fn is_binary_extension(ext: &str) -> bool {
    matches!(
        ext,
        // Executables and libraries
        "exe" | "dll" | "so" | "dylib" | "lib" | "a" | "o" | "obj" | "pdb" |
        // Archives
        "zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar" |
        // Media files
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "ico" | "svg" | "webp" |
        "mp3" | "mp4" | "avi" | "mov" | "wmv" | "flv" | "webm" | "mkv" |
        // Database files
        "db" | "sqlite" | "mdf" | "ldf" | "bak" |
        // Other binary formats
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx"
    )
}

/// Detect language from file extension
fn detect_language(path: &PathBuf) -> String {
    match path.extension().and_then(|e| e.to_str()) {
        Some("rs") => "rust",
        Some("ts") | Some("tsx") => "typescript",
        Some("js") | Some("jsx") => "javascript",
        Some("py") => "python",
        Some("java") => "java",
        Some("cs") => "csharp",
        Some("go") => "go",
        Some("cpp") | Some("cc") | Some("cxx") => "cpp",
        Some("c") => "c",
        Some("h") | Some("hpp") => "c", // Assume C for headers
        Some("rb") => "ruby",
        Some("php") => "php",
        Some("swift") => "swift",
        Some("kt") => "kotlin",
        Some("dart") => "dart",
        Some("gd") => "gdscript",
        Some("lua") => "lua",
        Some("vue") => "vue",
        Some("razor") => "razor",
        Some("sql") => "sql",
        Some("html") => "html",
        Some("css") => "css",
        Some("sh") | Some("bash") => "bash",
        Some("ps1") => "powershell",
        Some("zig") => "zig",
        _ => "unknown",
    }
    .to_string()
}

/// Process a single file: hash, check changes, extract symbols, store
fn process_file(
    file_path: &PathBuf,
    existing_hashes: &std::collections::HashMap<String, String>,
    _workspace_id: &str,
) -> Result<Option<FileResult>> {
    use julie::database::calculate_file_hash;
    use julie::extractors::ExtractorManager;

    let path_str = file_path.to_string_lossy().to_string();

    // Get file metadata
    let metadata = std::fs::metadata(file_path)
        .with_context(|| format!("Failed to get metadata: {:?}", file_path))?;

    let size = metadata.len();
    let last_modified = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // Read content
    let content = std::fs::read_to_string(file_path)
        .with_context(|| format!("Failed to read file: {:?}", file_path))?;

    // Calculate Blake3 hash
    let new_hash = calculate_file_hash(file_path)?;

    // Check if file changed
    if let Some(old_hash) = existing_hashes.get(&path_str) {
        if old_hash == &new_hash {
            // File unchanged, skip
            return Ok(None);
        }
    }

    // Extract symbols
    let extractor_manager = ExtractorManager::new();
    let symbols = extractor_manager
        .extract_symbols(&path_str, &content)
        .unwrap_or_default();

    let language = detect_language(file_path);

    Ok(Some(FileResult {
        path: path_str,
        language,
        hash: new_hash,
        size,
        last_modified,
        content,
        symbols,
    }))
}

fn scan_directory(
    dir: PathBuf,
    db: PathBuf,
    ignore: Option<String>,
    threads: Option<usize>,
) -> Result<()> {
    use julie::extractors::ExtractorManager;
    use rayon::prelude::*;
    use std::sync::{Arc, Mutex};

    let start = Instant::now();

    info!("🔍 Scanning directory: {:?}", dir);
    info!("💾 Database: {:?}", db);

    // Open/create database
    let mut database =
        SymbolDatabase::new(&db).with_context(|| format!("Failed to open database: {:?}", db))?;

    // Configure ignore patterns (use only what's passed via --ignore parameter)
    // No built-in defaults - caller controls all ignore patterns for single source of truth
    let ignore_patterns: Vec<String> = match ignore {
        Some(patterns) => patterns.split(',').map(|s| s.trim().to_string()).collect(),
        None => Vec::new(),
    };

    if !ignore_patterns.is_empty() {
        info!("🚫 Ignoring patterns: {:?}", ignore_patterns);
    }

    // Use directory path as workspace ID
    let workspace_id = dir.to_string_lossy().to_string().replace('\\', "/"); // Normalize path separators

    // Get existing file hashes for change detection
    eprintln!("📊 Loading existing file hashes...");
    let existing_hashes = database
        .get_file_hashes_for_workspace(&workspace_id)
        .unwrap_or_default();
    eprintln!(
        "📊 Found {} existing files in database",
        existing_hashes.len()
    );

    // Discover all files
    eprintln!("🔍 Discovering files...");
    let files = discover_files(&dir, &ignore_patterns)?;
    eprintln!("📁 Found {} files", files.len());

    if files.is_empty() {
        eprintln!("⚠️  No files found");
        return Ok(());
    }

    // Setup thread pool
    let num_threads = threads.unwrap_or_else(num_cpus::get);
    rayon::ThreadPoolBuilder::new()
        .num_threads(num_threads)
        .build_global()
        .ok(); // Ignore error if already initialized

    eprintln!("🚀 Processing with {} threads...", num_threads);

    // Process files in parallel
    let processed = Arc::new(Mutex::new(0usize));
    let skipped = Arc::new(Mutex::new(0usize));
    let total_files = files.len();
    let all_results = Arc::new(Mutex::new(Vec::new()));

    files.par_iter().for_each(|file_path| {
        match process_file(file_path, &existing_hashes, &workspace_id) {
            Ok(Some(file_result)) => {
                // Collect results for batch storage
                if let Ok(mut results) = all_results.lock() {
                    results.push(file_result);
                }

                if let Ok(mut proc) = processed.lock() {
                    *proc += 1;
                    if *proc % 50 == 0 || *proc == total_files {
                        eprintln!("⚡ Processed {}/{} files", *proc, total_files);
                    }
                }
            }
            Ok(None) => {
                // File unchanged, skipped
                if let Ok(mut skip) = skipped.lock() {
                    *skip += 1;
                }
            }
            Err(e) => {
                info!("⚠️  Error processing {:?}: {}", file_path, e);
            }
        }
    });

    let proc_count = *processed.lock().unwrap();
    let skip_count = *skipped.lock().unwrap();

    // Extract results
    let results = Arc::try_unwrap(all_results)
        .map_err(|_| anyhow::anyhow!("Failed to unwrap results Arc"))?
        .into_inner()
        .map_err(|e| anyhow::anyhow!("Lock error: {:?}", e))?;

    // Store file metadata and symbols
    if !results.is_empty() {
        eprintln!(
            "💾 Storing {} files and symbols in database...",
            results.len()
        );

        // Store file metadata
        for result in &results {
            database.store_file_with_content(
                &result.path,
                &result.language,
                &result.hash,
                result.size,
                result.last_modified,
                &result.content,
                &workspace_id,
            )?;
        }

        // Collect and store all symbols
        let all_symbols: Vec<_> = results.into_iter().flat_map(|r| r.symbols).collect();
        if !all_symbols.is_empty() {
            database.bulk_store_symbols(&all_symbols, &workspace_id)?;
        }
        eprintln!("💾 Stored {} symbols", all_symbols.len());

        // PHASE 2: Extract identifiers (references/usages) for LSP-quality reference tracking
        eprintln!("\n🔍 Phase 2: Extracting identifiers (references/usages)...");

        // Load all symbols for identifier resolution context
        let all_extracted_symbols = database.get_all_symbols()?;
        eprintln!(
            "📚 Loaded {} symbols for identifier extraction",
            all_extracted_symbols.len()
        );

        // Extract identifiers from files in parallel (ALL supported languages)
        let all_identifiers = Arc::new(Mutex::new(Vec::new()));
        let processed_phase2 = Arc::new(Mutex::new(0usize));
        let extractor_manager = ExtractorManager::new();

        files.par_iter().for_each(|file_path| {
            let file_path_str = file_path.to_string_lossy().to_string();

            // Read file content
            let content = match std::fs::read_to_string(file_path) {
                Ok(c) => c,
                Err(e) => {
                    debug!("⚠️  Failed to read file {:?}: {}", file_path, e);
                    return;
                }
            };

            // Extract identifiers using ExtractorManager (language-aware)
            match extractor_manager.extract_identifiers(
                &file_path_str,
                &content,
                &all_extracted_symbols,
            ) {
                Ok(identifiers) => {
                    if !identifiers.is_empty() {
                        if let Ok(mut all_ids) = all_identifiers.lock() {
                            all_ids.extend(identifiers);
                        }
                    }

                    if let Ok(mut proc) = processed_phase2.lock() {
                        *proc += 1;
                        if *proc % 50 == 0 {
                            eprintln!("⚡ Phase 2: Processed {} files", *proc);
                        }
                    }
                }
                Err(e) => {
                    debug!(
                        "⚠️  Error extracting identifiers from {:?}: {}",
                        file_path, e
                    );
                }
            }
        });

        // Store identifiers in database
        let identifiers = Arc::try_unwrap(all_identifiers)
            .map_err(|_| anyhow::anyhow!("Failed to unwrap identifiers Arc"))?
            .into_inner()
            .map_err(|e| anyhow::anyhow!("Lock error: {:?}", e))?;

        if !identifiers.is_empty() {
            eprintln!(
                "💾 Writing {} identifiers to database...",
                identifiers.len()
            );
            database.bulk_store_identifiers(&identifiers, &workspace_id)?;
            eprintln!(
                "✅ Phase 2 complete: {} identifiers extracted and stored",
                identifiers.len()
            );
        } else {
            eprintln!("ℹ️  No identifiers extracted");
        }

        // PHASE 3: Extract relationships (inheritance, implements, etc.) for cross-file analysis
        eprintln!("\n🔗 Phase 3: Extracting relationships (inheritance, implements)...");

        // Extract relationships from files in parallel (ALL supported languages)
        let all_relationships = Arc::new(Mutex::new(Vec::new()));
        let processed_phase3 = Arc::new(Mutex::new(0usize));

        files.par_iter().for_each(|file_path| {
            let file_path_str = file_path.to_string_lossy().to_string();

            // Read file content
            let content = match std::fs::read_to_string(file_path) {
                Ok(c) => c,
                Err(e) => {
                    debug!("⚠️  Failed to read file {:?}: {}", file_path, e);
                    return;
                }
            };

            // Extract relationships using ExtractorManager (language-aware)
            match extractor_manager.extract_relationships(
                &file_path_str,
                &content,
                &all_extracted_symbols,
            ) {
                Ok(relationships) => {
                    if !relationships.is_empty() {
                        if let Ok(mut all_rels) = all_relationships.lock() {
                            all_rels.extend(relationships);
                        }
                    }

                    if let Ok(mut proc) = processed_phase3.lock() {
                        *proc += 1;
                        if *proc % 50 == 0 {
                            eprintln!("⚡ Phase 3: Processed {} files", *proc);
                        }
                    }
                }
                Err(e) => {
                    debug!(
                        "⚠️  Error extracting relationships from {:?}: {}",
                        file_path, e
                    );
                }
            }
        });

        // Store relationships in database
        let relationships = Arc::try_unwrap(all_relationships)
            .map_err(|_| anyhow::anyhow!("Failed to unwrap relationships Arc"))?
            .into_inner()
            .map_err(|e| anyhow::anyhow!("Lock error: {:?}", e))?;

        if !relationships.is_empty() {
            eprintln!(
                "💾 Writing {} relationships to database...",
                relationships.len()
            );
            database.bulk_store_relationships(&relationships, &workspace_id)?;
            eprintln!(
                "✅ Phase 3 complete: {} relationships extracted and stored",
                relationships.len()
            );
        } else {
            eprintln!("ℹ️  No relationships extracted");
        }
    }

    let elapsed = start.elapsed();
    eprintln!("\n✅ Scan complete in {:.2}s", elapsed.as_secs_f64());
    eprintln!("   📊 Processed: {} files", proc_count);
    eprintln!("   ⏭️  Skipped: {} files (unchanged)", skip_count);

    Ok(())
}

fn update_file(file: PathBuf, db: PathBuf) -> Result<()> {
    use julie::database::calculate_file_hash;
    use julie::extractors::ExtractorManager;

    let start = Instant::now();

    info!("📝 Updating file: {:?}", file);
    info!("💾 Database: {:?}", db);

    // Check if file exists
    if !file.exists() {
        anyhow::bail!("File does not exist: {:?}", file);
    }

    // Check if file is binary (we only index text files)
    if let Some(ext) = file.extension().and_then(|e| e.to_str()) {
        if is_binary_extension(ext) {
            anyhow::bail!("Binary file extension not supported: {}", ext);
        }
    }

    // Open database
    let mut database =
        SymbolDatabase::new(&db).with_context(|| format!("Failed to open database: {:?}", db))?;

    let path_str = file.to_string_lossy().to_string();
    let language = detect_language(&file);

    // Get file metadata
    let metadata =
        std::fs::metadata(&file).with_context(|| format!("Failed to get metadata: {:?}", file))?;

    let size = metadata.len();
    let last_modified = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // Read content
    let content = std::fs::read_to_string(&file)
        .with_context(|| format!("Failed to read file: {:?}", file))?;

    // Calculate Blake3 hash
    let new_hash = calculate_file_hash(&file)?;

    // Check if file changed
    let existing_hash = database.get_file_hash(&path_str)?;
    if let Some(old_hash) = &existing_hash {
        if old_hash == &new_hash {
            let elapsed = start.elapsed();
            eprintln!(
                "⏭️  File unchanged, skipped in {:.2}ms",
                elapsed.as_secs_f64() * 1000.0
            );
            return Ok(());
        }
    }

    // Use file path's parent as workspace ID (consistent with scan command)
    let workspace_id = file
        .parent()
        .map(|p| p.to_string_lossy().to_string().replace('\\', "/"))
        .unwrap_or_else(|| ".".to_string());

    info!("🔄 File changed, extracting symbols...");

    // Delete old symbols for this file
    if existing_hash.is_some() {
        database.delete_symbols_for_file(&path_str)?;
        info!("🗑️  Deleted old symbols");
    }

    // Extract symbols
    let extractor_manager = ExtractorManager::new();
    let symbols = extractor_manager
        .extract_symbols(&path_str, &content)
        .unwrap_or_default();

    info!("🔍 Extracted {} symbols", symbols.len());

    // Update file metadata
    database.store_file_with_content(
        &path_str,
        &language,
        &new_hash,
        size,
        last_modified,
        &content,
        &workspace_id,
    )?;

    // Insert new symbols
    if !symbols.is_empty() {
        database.bulk_store_symbols(&symbols, &workspace_id)?;
    }

    let elapsed = start.elapsed();
    let action = if existing_hash.is_some() {
        "Updated"
    } else {
        "Added"
    };
    eprintln!(
        "✅ {} in {:.2}ms ({} symbols)",
        action,
        elapsed.as_secs_f64() * 1000.0,
        symbols.len()
    );

    Ok(())
}
