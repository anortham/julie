// Julie's Language Extractors Module
//
// This module contains all the tree-sitter based extractors for various programming languages.
// Each extractor is responsible for parsing source code and extracting symbols, relationships,
// and type information using tree-sitter parsers.

pub mod base;

// TODO: Implement language extractors (Phase 1 & 2)
// Phase 1 - Core Languages:
pub mod typescript;
pub mod javascript;          // JavaScript extractor - FIXING API errors - MY ASSIGNED TASK
pub mod python;
// pub mod dart;                // Dart extractor (TDD GREEN phase) - Temporarily disabled for Vue focus
pub mod rust;                // Rust extractor - RUST AGENT WORKING - DO NOT DISABLE
pub mod go;                  // Go extractor - API FIXED, READY FOR TESTING

// Phase 2 - Extended Languages:
// pub mod c;                   // C extractor (TDD GREEN phase) - Temporarily disabled for C++ focus
// pub mod cpp;                 // C++ extractor (TDD GREEN phase) - Temporarily disabled for Vue focus
pub mod java;                // Java extractor - FIXING API compatibility errors - MY ASSIGNED TASK
pub mod csharp;              // C# extractor - Testing if it works
// pub mod ruby;                // Ruby extractor (TDD RED phase) - Temporarily disabled for C++ focus
// pub mod php;                 // Temporarily disabled - compilation errors
// pub mod swift;               // Swift extractor (GREEN phase) - Temporarily disabled for Vue focus
// pub mod kotlin;              // Kotlin extractor (GREEN phase) - Temporarily disabled for C++ focus

// Phase 2 - Specialized Languages:
pub mod gdscript;              // GDScript extractor (Phase 1 SUCCESS - FIXED)
// pub mod lua;                    // Lua extractor - 100+ API errors
pub mod vue;
// pub mod razor;                 // Razor extractor - Minor bracket syntax errors to fix
pub mod sql;                 // SQL extractor - Fixed syntax errors
pub mod html;                // HTML extractor - REPAIRING API errors - MY ASSIGNED TASK
pub mod css;                    // CSS extractor (Phase 1 SUCCESS)
pub mod regex;                  // Regex extractor (Phase 1 SUCCESS)
pub mod bash;                   // Bash extractor (TDD GREEN phase)
pub mod powershell;             // PowerShell extractor (Phase 1 SUCCESS)
pub mod zig;                    // Zig extractor (Phase 1 SUCCESS - FIXED)

// Re-export the base extractor types
pub use base::{BaseExtractor, Symbol, SymbolKind, Relationship, RelationshipKind, TypeInfo};

/// Manager for all language extractors
pub struct ExtractorManager {
    // TODO: Store language parsers and extractors
}

impl ExtractorManager {
    pub fn new() -> Self {
        Self {
            // TODO: Initialize
        }
    }

    /// Get supported languages
    pub fn supported_languages(&self) -> Vec<&'static str> {
        vec![
            // TODO: Return actual supported languages as they are implemented
            "placeholder"
        ]
    }
}