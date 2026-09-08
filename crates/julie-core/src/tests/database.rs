// Tests for the database module, relocated from src/tests/core/database.rs.
// Imports from julie-core types use LOCAL helpers (see tests::helpers) to avoid
// the dev-dep cycle rlib mismatch.  External-crate types (Symbol, Relationship,
// etc.) come from julie_extractors and are imported directly.

use crate::database::*;
use crate::symbol::Symbol;
use crate::test_support::{
    file_info_builder, identifier_builder, open_test_connection, relationship_builder,
    set_symbol_reference_scores, symbol_builder,
};
use julie_extractors::{IdentifierKind, RelationshipKind, SymbolKind, Visibility};
use std::collections::HashMap;
use std::path::PathBuf;
use tempfile::TempDir;

mod basic_storage;
mod concurrency_wal;
mod deweighting;
mod embeddings;
mod extractor_symbols;
mod file_queries;
mod identifier_centrality;
mod identifier_queries;
mod migrations;
mod reference_scores_basic;
mod reference_scores_propagation;
mod relationships;
mod symbol_lookup;
