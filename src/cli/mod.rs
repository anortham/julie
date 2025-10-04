/// CLI utilities for julie-extract and julie-semantic binaries
///
/// This module provides shared functionality for the standalone CLI tools,
/// completely separate from the Julie MCP server. The MCP server does NOT
/// use these modules - they exist solely for the CLI binaries.
///
/// Modules:
/// - output: Handles different output formats (JSON, NDJSON, SQLite)
/// - parallel: Parallel extraction with Rayon for optimal performance
/// - progress: Progress reporting for long-running operations
pub mod output;
pub mod parallel;
pub mod progress;

pub use output::{OutputFormat, OutputWriter};
pub use parallel::{ExtractionConfig, ParallelExtractor};
pub use progress::{ProgressEvent, ProgressReporter};
