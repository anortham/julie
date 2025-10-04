/// julie-extract: High-performance symbol extraction CLI
///
/// Extracts symbols from source files using tree-sitter parsers for 26+ languages.
/// Supports multiple output formats optimized for different use cases.
///
/// Modes:
/// - single: Extract one file, output JSON (for incremental updates)
/// - bulk: Extract directory, write to SQLite (for initial indexing, fastest)
/// - stream: Extract directory, stream NDJSON (for large workspaces, memory-efficient)
use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use julie::cli::{ExtractionConfig, OutputFormat, OutputWriter, ParallelExtractor};

#[derive(Parser)]
#[command(name = "julie-extract")]
#[command(about = "High-performance tree-sitter symbol extraction for 26 languages", long_about = None)]
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

    /// Bulk extract entire directory (parallel, optimized for performance)
    Bulk {
        /// Directory to scan recursively
        #[arg(short, long)]
        directory: String,

        /// SQLite output database path
        #[arg(short, long)]
        output_db: String,

        /// Number of parallel threads (defaults to CPU count)
        #[arg(short, long)]
        threads: Option<usize>,

        /// Batch size for parallel processing
        #[arg(long, default_value_t = 100)]
        batch_size: usize,
    },

    /// Stream symbols from directory (NDJSON output, memory-efficient)
    Stream {
        /// Directory to scan recursively
        #[arg(short, long)]
        directory: String,

        /// Number of parallel threads (defaults to CPU count)
        #[arg(short, long)]
        threads: Option<usize>,
    },
}

#[derive(Debug, Clone, ValueEnum)]
enum OutputFormatArg {
    /// Standard JSON array (pretty-printed)
    Json,
    /// Newline-delimited JSON (streaming)
    Ndjson,
}

fn main() -> Result<()> {
    // Parse command line arguments
    let cli = Cli::parse();

    match cli.command {
        Commands::Single { file, output } => {
            extract_single_file(&file, output)?;
        }
        Commands::Bulk {
            directory,
            output_db,
            threads,
            batch_size,
        } => {
            extract_bulk(&directory, &output_db, threads, batch_size)?;
        }
        Commands::Stream { directory, threads } => {
            extract_stream(&directory, threads)?;
        }
    }

    Ok(())
}

/// Extract a single file and output JSON
fn extract_single_file(file: &str, output_format: OutputFormatArg) -> Result<()> {
    let config = ExtractionConfig {
        num_threads: 1,
        batch_size: 1,
        output_db: None,
    };

    let extractor = ParallelExtractor::new(config);
    let symbols = extractor.extract_file(file)?;

    // Write to stdout
    let format = match output_format {
        OutputFormatArg::Json => OutputFormat::Json,
        OutputFormatArg::Ndjson => OutputFormat::Ndjson,
    };

    let mut writer = OutputWriter::new(format)?;
    writer.write_batch(&symbols)?;
    writer.flush()?;

    Ok(())
}

/// Extract entire directory in bulk mode (fastest, SQLite output)
fn extract_bulk(
    directory: &str,
    output_db: &str,
    threads: Option<usize>,
    batch_size: usize,
) -> Result<()> {
    let config = ExtractionConfig {
        num_threads: threads.unwrap_or_else(num_cpus::get),
        batch_size,
        output_db: Some(output_db.to_string()),
    };

    eprintln!(
        "🚀 Starting bulk extraction: {} threads, batch size {}",
        config.num_threads, config.batch_size
    );

    let extractor = ParallelExtractor::new(config);
    let symbols = extractor.extract_directory(directory)?;

    // Output success summary to stdout (JSON for easy parsing)
    println!(
        r#"{{"success": true, "symbol_count": {}, "output_db": "{}"}}"#,
        symbols.len(),
        output_db
    );

    Ok(())
}

/// Extract directory in streaming mode (NDJSON, memory-efficient)
fn extract_stream(directory: &str, threads: Option<usize>) -> Result<()> {
    let config = ExtractionConfig {
        num_threads: threads.unwrap_or_else(num_cpus::get),
        batch_size: 50, // Smaller batches for streaming
        output_db: None,
    };

    let extractor = ParallelExtractor::new(config);
    let symbols = extractor.extract_directory(directory)?;

    // Output NDJSON to stdout (streaming-friendly)
    let mut writer = OutputWriter::new(OutputFormat::Ndjson)?;
    writer.write_batch(&symbols)?;
    writer.flush()?;

    Ok(())
}
