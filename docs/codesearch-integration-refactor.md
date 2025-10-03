# Julie CLI Extraction: Refactoring Plan for CodeSearch Integration

**Status**: Design Phase
**Goal**: Extract Julie's tree-sitter and semantic capabilities as standalone CLIs
**Critical Requirement**: ZERO disruption to Julie MCP server development

---

## 🎯 Architecture Goals

1. **Non-Disruptive**: Julie MCP server continues evolving independently
2. **High Performance**: Parallel extraction, bulk operations, minimal IPC overhead
3. **Flexible Output**: Support JSON (single file), NDJSON (streaming), SQLite (bulk)
4. **Maintainable**: Shared library code, thin CLI wrappers

---

## 📐 Current Julie Structure

```
julie/
├── src/
│   ├── lib.rs              # Library crate (extractors, embeddings, search, DB)
│   ├── main.rs             # MCP server binary
│   ├── extractors/         # 26 language extractors (32K LOC)
│   ├── embeddings/         # FastEmbed + HNSW
│   ├── database/           # SQLite operations
│   ├── search/             # Tantivy engine
│   ├── tools/              # MCP tools
│   ├── workspace/          # Workspace management
│   └── handler.rs          # MCP handler
└── Cargo.toml
```

**Key Insight**: Everything we need is already in `lib.rs` - extractors, DB, embeddings are library code, not server-specific.

---

## 🔧 Refactored Structure

```
julie/
├── src/
│   ├── lib.rs              # ✅ UNCHANGED - library crate
│   ├── main.rs             # ✅ UNCHANGED - MCP server
│   │
│   ├── bin/                # 🆕 NEW - CLI binaries
│   │   ├── extract.rs      # julie-extract CLI
│   │   └── semantic.rs     # julie-semantic CLI
│   │
│   ├── cli/                # 🆕 NEW - Shared CLI utilities
│   │   ├── mod.rs
│   │   ├── output.rs       # Output formatting (JSON, NDJSON, SQLite)
│   │   ├── parallel.rs     # Parallel processing helpers
│   │   └── progress.rs     # Progress reporting
│   │
│   ├── extractors/         # ✅ UNCHANGED
│   ├── embeddings/         # ✅ UNCHANGED
│   ├── database/           # ✅ UNCHANGED
│   ├── search/             # ✅ UNCHANGED
│   ├── tools/              # ✅ UNCHANGED
│   ├── workspace/          # ✅ UNCHANGED
│   └── handler.rs          # ✅ UNCHANGED
│
└── Cargo.toml              # 🔧 UPDATED - add CLI dependencies
```

**Result**: Julie server unchanged, new CLIs use existing library code.

---

## 📋 Phase 1: Library Preparation (No Breaking Changes)

### 1.1 Add CLI Module (New Code Only)

**File**: `src/cli/mod.rs`

```rust
/// Shared CLI utilities for julie-extract and julie-semantic
/// These modules are ONLY used by CLI binaries, not by the MCP server

pub mod output;
pub mod parallel;
pub mod progress;

pub use output::{OutputFormat, OutputWriter};
pub use parallel::{ParallelExtractor, ExtractionConfig};
pub use progress::{ProgressReporter, ProgressEvent};
```

**File**: `src/cli/output.rs`

```rust
use crate::extractors::Symbol;
use anyhow::Result;
use std::io::Write;

#[derive(Debug, Clone)]
pub enum OutputFormat {
    Json,           // Single JSON array (for single file)
    Ndjson,         // Newline-delimited JSON (for streaming)
    Sqlite(String), // SQLite database path (for bulk)
}

pub struct OutputWriter {
    format: OutputFormat,
    writer: Box<dyn Write>,
}

impl OutputWriter {
    pub fn new(format: OutputFormat) -> Result<Self> {
        let writer: Box<dyn Write> = match &format {
            OutputFormat::Sqlite(_) => Box::new(std::io::sink()), // DB writes handled separately
            _ => Box::new(std::io::stdout()),
        };

        Ok(Self { format, writer })
    }

    /// Write single symbol (for streaming)
    pub fn write_symbol(&mut self, symbol: &Symbol) -> Result<()> {
        match self.format {
            OutputFormat::Ndjson => {
                writeln!(self.writer, "{}", serde_json::to_string(symbol)?)?;
            }
            _ => {} // Buffered for batch write
        }
        Ok(())
    }

    /// Write batch of symbols
    pub fn write_batch(&mut self, symbols: &[Symbol]) -> Result<()> {
        match &self.format {
            OutputFormat::Json => {
                writeln!(self.writer, "{}", serde_json::to_string_pretty(symbols)?)?;
            }
            OutputFormat::Ndjson => {
                for symbol in symbols {
                    self.write_symbol(symbol)?;
                }
            }
            OutputFormat::Sqlite(path) => {
                // Handled by ParallelExtractor (direct DB writes)
            }
        }
        Ok(())
    }
}
```

**File**: `src/cli/parallel.rs`

```rust
use crate::extractors::{ExtractorManager, Symbol};
use crate::database::SymbolDatabase;
use rayon::prelude::*;
use std::sync::{Arc, Mutex};
use std::path::PathBuf;
use anyhow::Result;

pub struct ExtractionConfig {
    pub num_threads: usize,
    pub batch_size: usize,
    pub output_db: Option<String>,
}

pub struct ParallelExtractor {
    config: ExtractionConfig,
    extractor_manager: ExtractorManager,
}

impl ParallelExtractor {
    pub fn new(config: ExtractionConfig) -> Self {
        Self {
            config,
            extractor_manager: ExtractorManager::new(),
        }
    }

    /// Extract all files in directory (parallel, optimized for bulk operations)
    pub async fn extract_directory(&self, directory: &str) -> Result<Vec<Symbol>> {
        // 1. Setup thread pool
        rayon::ThreadPoolBuilder::new()
            .num_threads(self.config.num_threads)
            .build_global()?;

        // 2. Discover all files
        let files = self.discover_files(directory)?;
        eprintln!("📁 Found {} files to process", files.len());

        // 3. Setup output (SQLite or in-memory)
        let db = if let Some(db_path) = &self.config.output_db {
            let db = Arc::new(Mutex::new(SymbolDatabase::new(db_path)?));

            // CRITICAL: Begin bulk insert mode (drops indexes for speed)
            db.lock().unwrap().begin_bulk_insert()?;
            eprintln!("🗄️  SQLite bulk mode enabled");

            Some(db)
        } else {
            None
        };

        // 4. Process files in parallel batches
        let all_symbols = Arc::new(Mutex::new(Vec::new()));
        let processed = Arc::new(Mutex::new(0usize));

        files.par_chunks(self.config.batch_size).for_each(|batch| {
            // Extract symbols for this batch (parallel within batch)
            let batch_symbols: Vec<Symbol> = batch
                .par_iter()
                .flat_map(|file| {
                    self.extractor_manager
                        .extract_symbols_sync(file)
                        .ok()
                        .unwrap_or_default()
                })
                .collect();

            // Write to output
            if let Some(db) = &db {
                // Direct SQLite write (fastest for bulk)
                db.lock().unwrap()
                    .bulk_store_symbols(&batch_symbols)
                    .expect("Failed to store symbols");
            } else {
                // Collect in memory
                all_symbols.lock().unwrap().extend(batch_symbols);
            }

            // Progress reporting
            let mut proc = processed.lock().unwrap();
            *proc += batch.len();
            eprintln!("⚡ Processed {}/{} files", *proc, files.len());
        });

        // 5. Finalize
        if let Some(db) = db {
            // CRITICAL: End bulk mode (rebuilds indexes)
            db.lock().unwrap().end_bulk_insert()?;
            eprintln!("✅ Bulk insert complete, indexes rebuilt");

            // Return symbols from DB
            return Ok(db.lock().unwrap().get_all_symbols()?);
        }

        Ok(Arc::try_unwrap(all_symbols).unwrap().into_inner().unwrap())
    }

    /// Extract single file (for incremental updates)
    pub async fn extract_file(&self, file_path: &str) -> Result<Vec<Symbol>> {
        self.extractor_manager.extract_symbols(file_path, &std::fs::read_to_string(file_path)?).await
    }

    fn discover_files(&self, directory: &str) -> Result<Vec<PathBuf>> {
        // Walk directory, filter by supported extensions
        let mut files = Vec::new();
        for entry in walkdir::WalkDir::new(directory)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() {
                if let Some(ext) = entry.path().extension() {
                    if self.is_supported_extension(ext.to_str().unwrap_or("")) {
                        files.push(entry.path().to_path_buf());
                    }
                }
            }
        }
        Ok(files)
    }

    fn is_supported_extension(&self, ext: &str) -> bool {
        matches!(
            ext,
            "rs" | "ts" | "tsx" | "js" | "jsx" | "py" | "java" | "cs" | "go" | "cpp" | "c" |
            "rb" | "php" | "swift" | "kt" | "dart" | "gd" | "lua" | "vue" | "razor" | "sql" |
            "html" | "css" | "sh" | "bash" | "ps1" | "zig"
        )
    }
}

// Add sync version of extract_symbols for rayon compatibility
impl ExtractorManager {
    pub fn extract_symbols_sync(&self, file_path: &str) -> Result<Vec<Symbol>> {
        // Synchronous version for use in rayon parallel iterators
        let content = std::fs::read_to_string(file_path)?;
        // Use blocking version of async extraction
        tokio::runtime::Runtime::new()?.block_on(
            self.extract_symbols(file_path, &content)
        )
    }
}
```

**File**: `src/cli/progress.rs`

```rust
use std::time::Instant;

pub enum ProgressEvent {
    Started { total_files: usize },
    Progress { processed: usize, total: usize },
    Completed { total: usize, duration_ms: u64 },
}

pub struct ProgressReporter {
    start_time: Instant,
    total_files: usize,
}

impl ProgressReporter {
    pub fn new(total_files: usize) -> Self {
        eprintln!("🚀 Starting extraction: {} files", total_files);
        Self {
            start_time: Instant::now(),
            total_files,
        }
    }

    pub fn report(&self, processed: usize) {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        let rate = processed as f64 / elapsed;
        let pct = (processed as f64 / self.total_files as f64 * 100.0) as u32;

        eprintln!(
            "⚡ Progress: {}/{} ({}%) - {:.0} files/sec",
            processed, self.total_files, pct, rate
        );
    }

    pub fn complete(&self, total_symbols: usize) {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        eprintln!(
            "✅ Extraction complete: {} symbols in {:.2}s ({:.0} files/sec)",
            total_symbols,
            elapsed,
            self.total_files as f64 / elapsed
        );
    }
}
```

### 1.2 Update Cargo.toml (Add CLI Dependencies)

```toml
[dependencies]
# ... existing dependencies ...

# CLI-specific dependencies (only used by binaries)
clap = { version = "4.5", features = ["derive"] }
walkdir = "2.5"

[lib]
name = "julie"
path = "src/lib.rs"

[[bin]]
name = "julie-server"
path = "src/main.rs"
# ↑ UNCHANGED - MCP server

[[bin]]
name = "julie-extract"
path = "src/bin/extract.rs"
# ↑ NEW - Extraction CLI

[[bin]]
name = "julie-semantic"
path = "src/bin/semantic.rs"
# ↑ NEW - Semantic CLI
```

### 1.3 Update lib.rs (Expose CLI Module)

```rust
// src/lib.rs

pub mod database;
pub mod embeddings;
pub mod extractors;
pub mod handler;
pub mod health;
pub mod language;
pub mod search;
pub mod tools;
pub mod tracing;
pub mod utils;
pub mod watcher;
pub mod workspace;

// NEW: CLI utilities (only public for binary crates)
#[cfg(feature = "cli")]
pub mod cli;

// Re-export common types (unchanged)
pub use extractors::{Relationship, RelationshipKind, Symbol, SymbolKind};
// ... rest unchanged
```

---

## 📋 Phase 2: Build CLIs (New Binaries)

### 2.1 julie-extract CLI

**File**: `src/bin/extract.rs`

```rust
use clap::{Parser, Subcommand};
use julie::cli::{ExtractionConfig, OutputFormat, OutputWriter, ParallelExtractor, ProgressReporter};
use anyhow::Result;

#[derive(Parser)]
#[command(name = "julie-extract")]
#[command(about = "High-performance tree-sitter symbol extraction for 26 languages")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Extract symbols from a single file
    Single {
        /// Path to source file
        #[arg(short, long)]
        file: String,

        /// Output format
        #[arg(short, long, value_enum, default_value = "json")]
        output: OutputFormatArg,
    },

    /// Bulk extract entire directory (parallel, optimized)
    Bulk {
        /// Directory to scan
        #[arg(short, long)]
        directory: String,

        /// SQLite output database path
        #[arg(short, long)]
        output_db: String,

        /// Number of parallel threads
        #[arg(short, long, default_value_t = num_cpus::get())]
        threads: usize,

        /// Batch size for parallel processing
        #[arg(long, default_value_t = 100)]
        batch_size: usize,
    },

    /// Stream symbols from directory (NDJSON output)
    Stream {
        /// Directory to scan
        #[arg(short, long)]
        directory: String,

        /// Number of parallel threads
        #[arg(short, long, default_value_t = num_cpus::get())]
        threads: usize,
    },
}

#[derive(Debug, Clone, clap::ValueEnum)]
enum OutputFormatArg {
    Json,
    Ndjson,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Single { file, output } => {
            extract_single_file(&file, output).await?;
        }
        Commands::Bulk {
            directory,
            output_db,
            threads,
            batch_size,
        } => {
            extract_bulk(&directory, &output_db, threads, batch_size).await?;
        }
        Commands::Stream { directory, threads } => {
            extract_stream(&directory, threads).await?;
        }
    }

    Ok(())
}

async fn extract_single_file(file: &str, output_format: OutputFormatArg) -> Result<()> {
    let config = ExtractionConfig {
        num_threads: 1,
        batch_size: 1,
        output_db: None,
    };

    let extractor = ParallelExtractor::new(config);
    let symbols = extractor.extract_file(file).await?;

    let format = match output_format {
        OutputFormatArg::Json => OutputFormat::Json,
        OutputFormatArg::Ndjson => OutputFormat::Ndjson,
    };

    let mut writer = OutputWriter::new(format)?;
    writer.write_batch(&symbols)?;

    Ok(())
}

async fn extract_bulk(directory: &str, output_db: &str, threads: usize, batch_size: usize) -> Result<()> {
    let config = ExtractionConfig {
        num_threads: threads,
        batch_size,
        output_db: Some(output_db.to_string()),
    };

    let extractor = ParallelExtractor::new(config);
    let symbols = extractor.extract_directory(directory).await?;

    eprintln!("✅ Extracted {} symbols to {}", symbols.len(), output_db);

    // Output summary JSON to stdout
    println!(
        r#"{{"success": true, "symbol_count": {}, "output_db": "{}"}}"#,
        symbols.len(),
        output_db
    );

    Ok(())
}

async fn extract_stream(directory: &str, threads: usize) -> Result<()> {
    let config = ExtractionConfig {
        num_threads: threads,
        batch_size: 50,
        output_db: None,
    };

    let extractor = ParallelExtractor::new(config);
    let symbols = extractor.extract_directory(directory).await?;

    // Output NDJSON to stdout (streaming-friendly)
    let mut writer = OutputWriter::new(OutputFormat::Ndjson)?;
    writer.write_batch(&symbols)?;

    Ok(())
}
```

### 2.2 julie-semantic CLI

**File**: `src/bin/semantic.rs`

```rust
use clap::{Parser, Subcommand};
use julie::embeddings::{EmbeddingEngine, VectorStore};
use julie::database::SymbolDatabase;
use anyhow::Result;
use std::sync::{Arc, Mutex};

#[derive(Parser)]
#[command(name = "julie-semantic")]
#[command(about = "Semantic code intelligence with FastEmbed + HNSW")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate embeddings for symbols database
    Embed {
        /// SQLite symbols database path
        #[arg(long)]
        symbols_db: String,

        /// Output HNSW index path
        #[arg(short, long)]
        output: String,

        /// Embedding model name
        #[arg(long, default_value = "bge-small")]
        model: String,

        /// Batch size for embedding generation
        #[arg(long, default_value_t = 100)]
        batch_size: usize,
    },

    /// Semantic similarity search
    Search {
        /// Search query text
        #[arg(short, long)]
        query: String,

        /// HNSW index path
        #[arg(short, long)]
        index: String,

        /// Number of results
        #[arg(long, default_value_t = 10)]
        top_k: usize,
    },

    /// Find related symbols (for refactoring)
    Relate {
        /// Symbol ID to find relations for
        #[arg(long)]
        symbol_id: String,

        /// HNSW index path
        #[arg(short, long)]
        index: String,

        /// Number of results
        #[arg(long, default_value_t = 20)]
        top_k: usize,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Embed {
            symbols_db,
            output,
            model,
            batch_size,
        } => {
            embed_symbols(&symbols_db, &output, &model, batch_size).await?;
        }
        Commands::Search { query, index, top_k } => {
            semantic_search(&query, &index, top_k).await?;
        }
        Commands::Relate {
            symbol_id,
            index,
            top_k,
        } => {
            find_related(&symbol_id, &index, top_k).await?;
        }
    }

    Ok(())
}

async fn embed_symbols(db_path: &str, output: &str, model: &str, batch_size: usize) -> Result<()> {
    eprintln!("🧠 Loading symbols from {}...", db_path);

    // 1. Load symbols from SQLite
    let db = SymbolDatabase::new(db_path)?;
    let symbols = db.get_all_symbols()?;
    eprintln!("📊 Found {} symbols to embed", symbols.len());

    // 2. Initialize embedding engine
    let cache_dir = std::env::temp_dir().join("julie-embeddings");
    let db_arc = Arc::new(Mutex::new(db));
    let mut engine = EmbeddingEngine::new(model, cache_dir, db_arc.clone())?;

    eprintln!("🚀 Starting embedding generation (batch size: {})...", batch_size);

    // 3. Process in batches (dramatically reduces ML overhead)
    let mut vector_store = VectorStore::new(output)?;
    let mut total_embedded = 0;

    for (i, batch) in symbols.chunks(batch_size).enumerate() {
        // CRITICAL: Batch embedding is 10x faster than individual calls
        let embeddings = engine.embed_symbols_batch(batch)?;

        vector_store.add_batch(embeddings)?;
        total_embedded += batch.len();

        eprintln!(
            "⚡ Batch {}/{}: {} symbols embedded",
            i + 1,
            (symbols.len() + batch_size - 1) / batch_size,
            total_embedded
        );
    }

    // 4. Build HNSW index
    eprintln!("🔨 Building HNSW index...");
    vector_store.build_index()?;

    eprintln!("✅ Semantic index complete: {}", output);
    println!(
        r#"{{"success": true, "symbols_embedded": {}, "index_path": "{}"}}"#,
        total_embedded, output
    );

    Ok(())
}

async fn semantic_search(query: &str, index_path: &str, top_k: usize) -> Result<()> {
    // Load vector store
    let vector_store = VectorStore::load(index_path)?;

    // Generate query embedding
    let cache_dir = std::env::temp_dir().join("julie-embeddings");
    let dummy_db = Arc::new(Mutex::new(SymbolDatabase::in_memory()?));
    let mut engine = EmbeddingEngine::new("bge-small", cache_dir, dummy_db)?;

    let query_vector = engine.embed_text(query)?;

    // Search HNSW
    let results = vector_store.search(&query_vector, top_k)?;

    // Output JSON
    let json = serde_json::to_string_pretty(&results)?;
    println!("{}", json);

    Ok(())
}

async fn find_related(symbol_id: &str, index_path: &str, top_k: usize) -> Result<()> {
    let vector_store = VectorStore::load(index_path)?;

    // Get symbol's embedding and find nearest neighbors
    let related = vector_store.find_similar_to_symbol(symbol_id, top_k)?;

    let json = serde_json::to_string_pretty(&related)?;
    println!("{}", json);

    Ok(())
}
```

---

## 📋 Phase 3: Performance Optimization

### 3.1 Parallel Extraction Strategy

**Key Insight**: Let Rust/rayon handle parallelism, not C#

```rust
// OPTIMIZED: Parallel file discovery + parallel extraction
files.par_chunks(batch_size).for_each(|batch| {
    let symbols: Vec<Symbol> = batch
        .par_iter()  // Parallel iteration within batch
        .flat_map(|file| extract_file(file).ok().unwrap_or_default())
        .collect();

    // Batched DB write (reduces lock contention)
    db.bulk_store_symbols(&symbols).unwrap();
});
```

**Why this is faster**:
1. **Rayon work stealing**: Optimal CPU utilization
2. **Batched I/O**: Fewer DB transactions
3. **No IPC overhead**: Everything in-process
4. **Tree-sitter thread pool**: Reuses parsers

### 3.2 SQLite Bulk Insert Optimization

**File**: `src/database/mod.rs` (enhancement)

```rust
impl SymbolDatabase {
    /// Begin bulk insert mode (drops indexes for speed)
    pub fn begin_bulk_insert(&mut self) -> Result<()> {
        self.conn.execute("PRAGMA synchronous = OFF", [])?;
        self.conn.execute("PRAGMA journal_mode = MEMORY", [])?;
        self.conn.execute("BEGIN TRANSACTION", [])?;

        // Drop indexes (will rebuild later)
        self.conn.execute("DROP INDEX IF EXISTS idx_symbols_name", [])?;
        self.conn.execute("DROP INDEX IF EXISTS idx_symbols_file_path", [])?;

        tracing::info!("🚀 Bulk insert mode enabled (indexes dropped)");
        Ok(())
    }

    /// End bulk insert mode (rebuilds indexes)
    pub fn end_bulk_insert(&mut self) -> Result<()> {
        self.conn.execute("COMMIT", [])?;

        // Rebuild indexes
        self.conn.execute(
            "CREATE INDEX idx_symbols_name ON symbols(name)",
            []
        )?;
        self.conn.execute(
            "CREATE INDEX idx_symbols_file_path ON symbols(file_path)",
            []
        )?;

        // Restore normal mode
        self.conn.execute("PRAGMA synchronous = NORMAL", [])?;
        self.conn.execute("PRAGMA journal_mode = WAL", [])?;

        tracing::info!("✅ Bulk insert complete, indexes rebuilt");
        Ok(())
    }

    /// Bulk store symbols (batched insert)
    pub fn bulk_store_symbols(&mut self, symbols: &[Symbol]) -> Result<()> {
        if symbols.is_empty() {
            return Ok(());
        }

        // Prepare statement once, execute many times
        let mut stmt = self.conn.prepare(
            "INSERT OR REPLACE INTO symbols
             (id, name, kind, file_path, start_line, start_col, end_line, end_col, signature)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )?;

        for symbol in symbols {
            stmt.execute(params![
                symbol.id,
                symbol.name,
                symbol.kind.to_string(),
                symbol.file_path,
                symbol.start_line,
                symbol.start_col,
                symbol.end_line,
                symbol.end_col,
                symbol.signature,
            ])?;
        }

        Ok(())
    }
}
```

### 3.3 Streaming Output (Memory Efficiency)

**For large workspaces, stream results instead of buffering:**

```rust
// julie-extract stream mode (NDJSON)
async fn extract_stream(directory: &str) -> Result<()> {
    let files = discover_files(directory)?;

    // Stream symbols as we extract (no buffering)
    for file in files {
        let symbols = extract_file(&file).await?;

        for symbol in symbols {
            // Write immediately to stdout (NDJSON)
            println!("{}", serde_json::to_string(&symbol)?);
        }
    }

    Ok(())
}
```

**CodeSearch reads stream:**

```csharp
// CodeSearch consumes NDJSON stream
public async IAsyncEnumerable<Symbol> StreamExtract(string directory)
{
    var process = StartJulieExtract($"stream --directory {directory}");

    await foreach (var line in process.StandardOutput.Lines())
    {
        yield return JsonSerializer.Deserialize<Symbol>(line);
    }
}
```

---

## 📋 Phase 4: Testing & Validation

### 4.1 Unit Tests (Rust)

```rust
// tests/cli_extraction_tests.rs

#[tokio::test]
async fn test_single_file_extraction() {
    let result = extract_single_file("tests/samples/test.rs", OutputFormat::Json).await;
    assert!(result.is_ok());

    let symbols = result.unwrap();
    assert!(!symbols.is_empty());
}

#[tokio::test]
async fn test_bulk_extraction_sqlite() {
    let temp_db = "/tmp/test_symbols.db";

    extract_bulk("tests/samples", temp_db, 4, 10).await.unwrap();

    let db = SymbolDatabase::new(temp_db).unwrap();
    let symbols = db.get_all_symbols().unwrap();

    assert!(symbols.len() > 100); // Should have many symbols
}

#[tokio::test]
async fn test_parallel_performance() {
    let start = Instant::now();

    let config = ExtractionConfig {
        num_threads: 8,
        batch_size: 100,
        output_db: None,
    };

    let extractor = ParallelExtractor::new(config);
    let symbols = extractor.extract_directory("tests/samples").await.unwrap();

    let elapsed = start.elapsed();
    let rate = symbols.len() as f64 / elapsed.as_secs_f64();

    println!("Extraction rate: {:.0} symbols/sec", rate);
    assert!(rate > 100.0); // Should be fast
}
```

### 4.2 Integration Tests (with CodeSearch)

```bash
# Build Julie CLIs
cd ~/Source/julie
cargo build --release --bin julie-extract
cargo build --release --bin julie-semantic

# Copy to CodeSearch bin folder
cp target/release/julie-extract ~/Source/coa-codesearch-mcp/bin/
cp target/release/julie-semantic ~/Source/coa-codesearch-mcp/bin/

# Test from CodeSearch
cd ~/Source/coa-codesearch-mcp
dotnet test --filter "JulieIntegrationTests"
```

---

## 📋 Phase 5: Build & Deployment

### 5.1 Cross-Platform Builds

```bash
# Build for all platforms
cargo build --release --target x86_64-unknown-linux-gnu --bin julie-extract
cargo build --release --target x86_64-pc-windows-gnu --bin julie-extract
cargo build --release --target aarch64-apple-darwin --bin julie-extract

# Rename for bundling
mv target/x86_64-unknown-linux-gnu/release/julie-extract bin/julie-extract-linux
mv target/x86_64-pc-windows-gnu/release/julie-extract.exe bin/julie-extract-windows.exe
mv target/aarch64-apple-darwin/release/julie-extract bin/julie-extract-macos
```

### 5.2 CI/CD Pipeline

```yaml
# .github/workflows/build-cli-binaries.yml
name: Build Julie CLI Binaries

on:
  push:
    tags:
      - 'v*'

jobs:
  build:
    strategy:
      matrix:
        include:
          - os: ubuntu-latest
            target: x86_64-unknown-linux-gnu
            artifact: julie-extract-linux
          - os: windows-latest
            target: x86_64-pc-windows-gnu
            artifact: julie-extract-windows.exe
          - os: macos-latest
            target: aarch64-apple-darwin
            artifact: julie-extract-macos

    runs-on: ${{ matrix.os }}

    steps:
      - uses: actions/checkout@v3
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
          target: ${{ matrix.target }}

      - name: Build julie-extract
        run: cargo build --release --target ${{ matrix.target }} --bin julie-extract

      - name: Upload artifact
        uses: actions/upload-artifact@v3
        with:
          name: ${{ matrix.artifact }}
          path: target/${{ matrix.target }}/release/julie-extract*
```

---

## 📊 Performance Benchmarks

### Expected Performance

| Operation | Target | Current (Julie MCP) | CLI Mode |
|-----------|--------|---------------------|----------|
| **Single File Extract** | <50ms | ~30ms | ~20ms (less overhead) |
| **Bulk Extract (1000 files)** | <10s | ~15s (concurrency issues) | ~5s (optimized parallel) |
| **Embedding Generation (10K symbols)** | <30s | ~45s | ~20s (batched) |
| **Semantic Search** | <30ms | ~50ms | ~15ms (HNSW only) |

### Bottleneck Analysis

```
julie-extract bulk (1000 files, 8 cores):

File Discovery:           ~500ms  (walkdir)
Tree-sitter Parsing:      ~2000ms (parallel, 8 threads)
Symbol Extraction:        ~1000ms (in-memory, no DB)
SQLite Bulk Insert:       ~1500ms (batched, indexes dropped)
Index Rebuild:            ~500ms  (SQLite ANALYZE)
─────────────────────────────────
Total:                    ~5500ms (~180 files/sec)

Optimization potential:
- Memoize parser instances: -500ms
- Increase batch size: -300ms
- Use prepared statements: -200ms
─────────────────────────────────
Optimized:                ~4500ms (~220 files/sec)
```

---

## 🎯 Success Criteria

### Phase Completion Checklist

**Phase 1: Library Prep**
- [ ] CLI module added (`src/cli/`)
- [ ] Zero changes to Julie MCP server code
- [ ] All existing tests pass
- [ ] Julie server still runs

**Phase 2: CLI Implementation**
- [ ] `julie-extract` binary builds
- [ ] `julie-semantic` binary builds
- [ ] Single file mode works
- [ ] Bulk mode works (SQLite)
- [ ] Streaming mode works (NDJSON)

**Phase 3: Performance**
- [ ] Parallel extraction >100 files/sec
- [ ] Bulk insert <10s for 1000 files
- [ ] Embedding generation <30s for 10K symbols
- [ ] Memory usage <500MB for bulk operations

**Phase 4: Testing**
- [ ] All Rust unit tests pass
- [ ] Integration tests with CodeSearch pass
- [ ] Cross-platform builds successful

**Phase 5: Deployment**
- [ ] Binaries bundled with CodeSearch
- [ ] CI/CD pipeline builds all platforms
- [ ] Version compatibility checks

---

## 🔧 Development Workflow

### Daily Iteration

```bash
# 1. Work on Julie extractors (business as usual)
cd ~/Source/julie
# ... edit extractor code ...

# 2. Test MCP server (unchanged)
cargo run -- stdio

# 3. Build CLI (new)
cargo build --release --bin julie-extract

# 4. Test CLI directly
./target/release/julie-extract single --file test.cs --output json

# 5. Test bulk mode
./target/release/julie-extract bulk \
  --directory ~/Source/coa-codesearch-mcp \
  --output-db /tmp/symbols.db \
  --threads 8

# 6. Check performance
time ./target/release/julie-extract bulk --directory ~/Source/some-project --output-db /tmp/test.db

# 7. Deploy to CodeSearch (optional)
cp target/release/julie-{extract,semantic} ~/Source/coa-codesearch-mcp/bin/
```

---

## 📈 Future Enhancements

### Post-Launch Optimizations

1. **Incremental Extraction**
   - Cache file hashes, only re-extract changed files
   - Delta updates to SQLite

2. **gRPC Transport**
   - Replace JSON stdio with binary protocol
   - Streaming bidirectional communication

3. **Distributed Processing**
   - Shard large workspaces across multiple julie-extract instances
   - Aggregator service combines results

4. **Persistent Parser Pool**
   - Keep tree-sitter parsers in memory between runs
   - Daemon mode for sub-10ms extraction

---

## 🚀 Ready to Build?

**Next Steps**:
1. Review this plan
2. Approve architecture
3. Create feature branch: `feature/cli-extraction`
4. Begin Phase 1: Library preparation

**Estimated Timeline**: 2-3 weeks
**Risk Level**: Low (no breaking changes)
**Impact**: High (enables CodeSearch integration)

---

**Questions? Issues? Optimizations?**
Open a discussion or file an issue in the Julie repo.

Let's build this! 🔥
