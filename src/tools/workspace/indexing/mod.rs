//! Workspace indexing module
//!
//! Responsible for discovering, parsing, and indexing files within a workspace.
//! Coordinates symbol extraction, database storage, and Tantivy search indexing.
//!
//! ## Module Structure
//!
//! - **index**: Main entry point - coordinates the full indexing pipeline
//! - **processor**: File processing logic - handles parsing and symbol extraction
//! - **extractor**: Symbol extraction from ASTs - all 26 language extractors
//! - **incremental**: Incremental updates - detects changed files and orphan cleanup

pub(crate) mod extractor;
pub(crate) mod incremental;
pub(crate) mod index;
pub(crate) mod processor;
