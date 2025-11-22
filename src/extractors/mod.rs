//! Julie's Language Extractors Module (re-exports from julie-extractors crate)
//!
//! This module re-exports the julie-extractors crate for backward compatibility.
//! All extractor functionality is now in the separate `julie-extractors` crate.

// Re-export everything from the julie-extractors crate
pub use julie_extractors::*;

// Re-export language modules for code that uses `crate::extractors::rust::RustExtractor` etc.
pub use julie_extractors::{
    bash, c, cpp, csharp, css, dart, gdscript, go, html, java, javascript, json, kotlin, lua,
    markdown, php, powershell, python, qml, r, razor, regex, ruby, rust, sql, swift, toml,
    typescript, vue, yaml, zig,
};

// Re-export base module for code that uses `crate::extractors::base::*`
pub use julie_extractors::base;

// Re-export factory and manager
pub use julie_extractors::{factory, manager, routing_identifiers, routing_relationships, routing_symbols};
