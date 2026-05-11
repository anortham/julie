//! Example: extract symbols from a file path argument.
//!
//! Run: `cargo run -p julie-extractors --example extract_file -- path/to/file.rs`

use julie_extractors::{capability_snapshot, extract_canonical};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() -> anyhow::Result<()> {
    let path = env::args().nth(1).expect("usage: extract_file <path>");
    let path = PathBuf::from(path);
    let file_path_str = path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("file path is not valid UTF-8: {:?}", path))?;
    let source = fs::read_to_string(&path)?;
    let workspace_root = path.parent().unwrap_or(Path::new("."));
    let result = extract_canonical(file_path_str, &source, workspace_root)?;
    println!("# {}", path.display());
    println!("Symbols: {}", result.symbols.len());
    for s in &result.symbols {
        println!("  - {} ({:?}) at line {}", s.name, s.kind, s.start_line);
    }
    println!("Relationships: {}", result.relationships.len());
    println!(
        "Structured pending: {}",
        result.structured_pending_relationships.len()
    );

    let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let snap = capability_snapshot();
    if let Some(cap) = snap
        .languages()
        .find(|row| row.extensions.iter().any(|e| e == extension))
    {
        println!("\nLanguage: {}", cap.language);
        println!(
            "Capabilities: symbols={} relationships={} pending={} identifiers={} types={}",
            cap.capabilities.symbols,
            cap.capabilities.relationships,
            cap.capabilities.pending_relationships,
            cap.capabilities.identifiers,
            cap.capabilities.types
        );
    } else {
        println!(
            "\n(no julie-extractors language matches extension `.{}`; extraction still proceeded via filename heuristics)",
            extension
        );
    }
    Ok(())
}
