//! Name lookup over `SymbolDatabase` for the editing tools, until they move onto the snapshot.

use anyhow::Result;
use serde_json::Value;

use crate::navigation::resolution::{file_path_matches_suffix, parse_qualified_name};
use julie_core::Symbol;
use julie_core::database::SymbolDatabase;
use julie_extractors::SymbolKind;

/// Look up a symbol by name, optionally disambiguated by file path.
///
/// Resolution order:
/// 1. Try full name as-is (handles Elixir's "Phoenix.Router", Scala's "cats.Monad", etc.)
/// 2. Try qualified parent/child parsing (handles Rust's "Struct::method", Python's "Class.method")
/// 3. Fall back to full name without definition-kind filter
pub fn find_symbol(
    db: &SymbolDatabase,
    name: &str,
    context_file: Option<&str>,
) -> Result<Vec<Symbol>> {
    // Step 1: Try full name first — handles flat namespace languages (Elixir, Scala, PHP, C#)
    // where "Phoenix.Channel" is a single symbol name, not a parent/child relationship.
    if name.contains('.') || name.contains("::") {
        let mut full_name_results = db.find_symbols_by_name(name)?;
        full_name_results.retain(|s| !is_lookup_stub(&s.kind));
        // Only use these if we found actual definitions (Module, Class, Trait, Function, etc.)
        let definitions: Vec<Symbol> = full_name_results
            .iter()
            .filter(|s| is_definition_kind(&s.kind))
            .cloned()
            .collect();
        if !definitions.is_empty() {
            return apply_context_file_filter(definitions, context_file);
        }
    }

    // Step 2: Try qualified name resolution (e.g. "SearchIndex::search_symbols" or "MyClass.method")
    if let Some((parent_name, child_name)) = parse_qualified_name(name) {
        let mut candidates = db.find_symbols_by_name(child_name)?;
        candidates.retain(|s| !is_lookup_stub(&s.kind));

        // Find parent symbols by name to collect their IDs
        let parents = db.find_symbols_by_name(parent_name)?;
        let parent_ids: std::collections::HashSet<&str> =
            parents.iter().map(|p| p.id.as_str()).collect();
        let parent_leaf_name = qualified_name_leaf(parent_name);

        let qualified: Vec<Symbol> = candidates
            .iter()
            .filter(|s| {
                s.parent_id
                    .as_deref()
                    .map_or(false, |pid| parent_ids.contains(pid))
                    || impl_type_name(s)
                        .is_some_and(|name| name == parent_name || name == parent_leaf_name)
            })
            .cloned()
            .collect();

        if !qualified.is_empty() {
            return apply_context_file_filter(qualified, context_file);
        }
        // Fall through if no parent match found (e.g. parent not yet indexed)
    }

    // Step 3: Fall back to full name without definition-kind filter
    let mut symbols = db.find_symbols_by_name(name)?;
    symbols.retain(|s| !is_lookup_stub(&s.kind));
    apply_context_file_filter(symbols, context_file)
}

fn qualified_name_leaf(name: &str) -> &str {
    name.rsplit_once("::")
        .map(|(_, tail)| tail)
        .or_else(|| name.rsplit_once('.').map(|(_, tail)| tail))
        .unwrap_or(name)
}

fn impl_type_name(symbol: &Symbol) -> Option<&str> {
    symbol
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("impl_type_name"))
        .and_then(Value::as_str)
}

/// Filter symbols by context_file if provided, falling back to full list.
fn apply_context_file_filter(
    symbols: Vec<Symbol>,
    context_file: Option<&str>,
) -> Result<Vec<Symbol>> {
    if let Some(file) = context_file {
        let file_matches: Vec<Symbol> = symbols
            .iter()
            .filter(|s| context_file_matches(&s.file_path, file))
            .cloned()
            .collect();
        return Ok(file_matches);
    }
    Ok(symbols)
}

fn context_file_matches(symbol_path: &str, context_file: &str) -> bool {
    file_path_matches_suffix(symbol_path, context_file)
        || file_path_matches_suffix(context_file, symbol_path)
        || context_file_matches_basename_stem(symbol_path, context_file)
}

fn context_file_matches_basename_stem(symbol_path: &str, context_file: &str) -> bool {
    if context_file.contains(['/', '\\', '.']) {
        return false;
    }

    let basename = symbol_path
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(symbol_path);
    let stem = basename.rsplit_once('.').map_or(basename, |(stem, _)| stem);
    stem == context_file
}

fn is_lookup_stub(kind: &SymbolKind) -> bool {
    matches!(kind, SymbolKind::Import | SymbolKind::Export)
}

/// Definition kinds that should be preferred in full-name lookups.
fn is_definition_kind(kind: &SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Module
            | SymbolKind::Class
            | SymbolKind::Struct
            | SymbolKind::Trait
            | SymbolKind::Interface
            | SymbolKind::Enum
            | SymbolKind::Function
            | SymbolKind::Method
            | SymbolKind::Namespace
            | SymbolKind::Type
            | SymbolKind::Constant
    )
}
