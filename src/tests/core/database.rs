// Tests extracted from src/database/mod.rs
// These were previously inline tests that have been moved to follow project standards

use crate::database::*;
use crate::extractors::{IdentifierKind, RelationshipKind, Symbol, SymbolKind, Visibility};
use crate::tests::helpers::db::{
    file_info_builder, identifier_builder, relationship_builder, set_symbol_reference_scores,
    symbol_builder,
};
use crate::tests::test_helpers::open_test_connection;
use std::collections::HashMap;
use std::path::PathBuf;
use tempfile::TempDir;
use tree_sitter::Parser;

mod basic_storage;
mod concurrency_wal;
mod embeddings;
mod extractor_symbols;
mod file_queries;
mod identifier_centrality;
mod migrations;
mod reference_scores_basic;
mod reference_scores_propagation;
mod relationships;
mod symbol_lookup;
mod deweighting;
