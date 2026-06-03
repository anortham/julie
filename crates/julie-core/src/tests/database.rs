// Tests for the database module, relocated from src/tests/core/database.rs.
// Imports from julie-core types use LOCAL helpers (see tests::helpers) to avoid
// the dev-dep cycle rlib mismatch.  External-crate types (Symbol, Relationship,
// etc.) come from julie_extractors and are imported directly.

use crate::database::*;
use julie_extractors::{IdentifierKind, RelationshipKind, Symbol, SymbolKind, Visibility};
// Local helpers for julie-core types (FileInfo, SymbolDatabase operations)
use super::helpers::{file_info_builder, set_symbol_reference_scores};
// open_test_connection returns rusqlite::Connection — external type — safe to import from julie-test-support
use julie_test_support::open_test_connection;
// External-type builders from julie-test-support (Symbol/Relationship/Identifier)
use julie_test_support::{identifier_builder, relationship_builder, symbol_builder};
use std::collections::HashMap;
use std::path::PathBuf;
use tempfile::TempDir;
use tree_sitter::Parser;

mod basic_storage;
mod concurrency_wal;
mod deweighting;
mod embeddings;
mod extractor_symbols;
mod file_queries;
mod identifier_centrality;
mod migrations;
mod reference_scores_basic;
mod reference_scores_propagation;
mod relationships;
mod symbol_lookup;
