/// Output formatting for CLI tools
///
/// Supports multiple output formats optimized for different use cases:
/// - JSON: Single array, pretty-printed (for single file extraction)
/// - NDJSON: Newline-delimited JSON, streaming-friendly (for large directories)
/// - SQLite: Direct database writes (for bulk operations, fastest)
use crate::extractors::base::Symbol;
use anyhow::Result;
use std::io::{self, Write};

#[derive(Debug, Clone)]
pub enum OutputFormat {
    /// Standard JSON array (pretty-printed)
    Json,

    /// Newline-delimited JSON (streaming)
    Ndjson,

    /// SQLite database path (bulk mode)
    Sqlite(String),
}

pub struct OutputWriter {
    format: OutputFormat,
    writer: Box<dyn Write>,
    buffer: Vec<Symbol>,
}

impl OutputWriter {
    /// Create a new output writer
    pub fn new(format: OutputFormat) -> Result<Self> {
        let writer: Box<dyn Write> = match &format {
            OutputFormat::Sqlite(_) => {
                // For SQLite, we don't write to stdout
                Box::new(io::sink())
            }
            _ => {
                // For JSON/NDJSON, write to stdout
                Box::new(io::stdout())
            }
        };

        Ok(Self {
            format,
            writer,
            buffer: Vec::new(),
        })
    }

    /// Write a single symbol (for streaming mode)
    pub fn write_symbol(&mut self, symbol: &Symbol) -> Result<()> {
        match &self.format {
            OutputFormat::Ndjson => {
                // Write immediately as NDJSON line
                writeln!(self.writer, "{}", serde_json::to_string(symbol)?)?;
                self.writer.flush()?;
            }
            _ => {
                // Buffer for batch write
                self.buffer.push(symbol.clone());
            }
        }
        Ok(())
    }

    /// Write a batch of symbols
    pub fn write_batch(&mut self, symbols: &[Symbol]) -> Result<()> {
        match &self.format {
            OutputFormat::Json => {
                // Pretty-printed JSON array
                writeln!(self.writer, "{}", serde_json::to_string_pretty(symbols)?)?;
                self.writer.flush()?;
            }
            OutputFormat::Ndjson => {
                // Write each symbol as a line
                for symbol in symbols {
                    self.write_symbol(symbol)?;
                }
            }
            OutputFormat::Sqlite(_) => {
                // SQLite writes are handled by ParallelExtractor
                // This is a no-op for the writer
            }
        }
        Ok(())
    }

    /// Flush any buffered symbols (for JSON mode)
    pub fn flush(mut self) -> Result<()> {
        if !self.buffer.is_empty() {
            // Clone buffer to avoid borrow conflict
            let buffer = self.buffer.clone();
            self.write_batch(&buffer)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extractors::base::SymbolKind;

    #[test]
    fn test_json_output() {
        let symbols = vec![Symbol {
            id: "test1".to_string(),
            name: "TestSymbol".to_string(),
            kind: SymbolKind::Function,
            file_path: "test.rs".to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 5,
            end_column: 0,
            start_byte: 0,
            end_byte: 100,
            signature: Some("fn test()".to_string()),
            doc_comment: None,
            parent_id: None,
            visibility: None,
            language: "rust".to_string(),
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
        }];

        let mut writer = OutputWriter::new(OutputFormat::Json).unwrap();
        writer.write_batch(&symbols).unwrap();
    }

    #[test]
    fn test_ndjson_streaming() {
        let symbol = Symbol {
            id: "test1".to_string(),
            name: "TestSymbol".to_string(),
            kind: SymbolKind::Function,
            file_path: "test.rs".to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 5,
            end_column: 0,
            start_byte: 0,
            end_byte: 100,
            signature: Some("fn test()".to_string()),
            doc_comment: None,
            parent_id: None,
            visibility: None,
            language: "rust".to_string(),
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
        };

        let mut writer = OutputWriter::new(OutputFormat::Ndjson).unwrap();
        writer.write_symbol(&symbol).unwrap();
    }
}
