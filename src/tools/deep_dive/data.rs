//! Data gathering for deep_dive tool
//!
//! Collects symbol context from SQLite: relationships, children, types.
//! All queries use existing indexed data — no new indexing required.

use anyhow::Result;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use tracing::debug;

use crate::database::{IdentifierRef, SymbolDatabase};
use crate::extractors::base::{Relationship, RelationshipKind, Symbol, SymbolKind};
use crate::search::scoring::is_test_path;
use crate::tools::navigation::resolution::{file_path_matches_suffix, parse_qualified_name};
use crate::tools::shared::NOISE_CALLEE_NAMES;

/// Aggregated context for a single symbol, ready for formatting
#[derive(Debug)]
pub struct SymbolContext {
    /// The primary symbol being investigated
    pub symbol: Symbol,
    /// Incoming references: who calls/uses this symbol
    pub incoming: Vec<RefEntry>,
    /// Total incoming before capping
    pub incoming_total: usize,
    /// Total incoming call refs before capping
    pub incoming_calls_total: usize,
    /// Outgoing references: what this symbol calls/uses
    pub outgoing: Vec<RefEntry>,
    /// Total outgoing before capping
    pub outgoing_total: usize,
    /// Total outgoing call refs before capping
    pub outgoing_calls_total: usize,
    /// Child symbols (methods, fields) for struct/class/trait/enum
    pub children: Vec<Symbol>,
    /// Implementations of this trait/interface
    pub implementations: Vec<Symbol>,
    /// Test file references (populated at context and full depth)
    pub test_refs: Vec<RefEntry>,
    /// Semantically similar symbols (populated at "full" depth only)
    pub similar: Vec<SimilarEntry>,
}

// Re-export SimilarEntry from shared similarity module
pub use crate::search::similarity::SimilarEntry;

/// A reference entry with optional enriched symbol data
#[derive(Debug, Clone)]
pub struct RefEntry {
    pub kind: RelationshipKind,
    pub file_path: String,
    pub line_number: u32,
    /// The source/target symbol (enriched at all depth levels)
    pub symbol: Option<Symbol>,
}

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

/// Build full context for a symbol.
///
/// Refs are always enriched with symbol names. `depth` controls body display:
/// - "overview": no code bodies
/// - "context": primary symbol body (30 lines)
/// - "full": primary symbol body (100 lines) + ref bodies
pub fn build_symbol_context(
    db: &SymbolDatabase,
    symbol: &Symbol,
    depth: &str,
    incoming_cap: usize,
    outgoing_cap: usize,
) -> Result<SymbolContext> {
    let symbol_ids = vec![symbol.id.clone()];
    let needs_body_enrichment = depth != "overview";

    // === Incoming references (who references this symbol) ===
    let raw_incoming = db.get_relationships_to_symbols(&symbol_ids)?;
    let incoming_total = raw_incoming.len();
    let incoming_calls_total = raw_incoming
        .iter()
        .filter(|rel| matches!(rel.kind, RelationshipKind::Calls))
        .count();

    let incoming_rels: Vec<&Relationship> = raw_incoming.iter().take(incoming_cap).collect();
    let mut incoming: Vec<RefEntry> = incoming_rels
        .iter()
        .map(|rel| RefEntry {
            kind: rel.kind.clone(),
            file_path: rel.file_path.clone(),
            line_number: rel.line_number,
            symbol: None,
        })
        .collect();

    // Always enrich refs — symbol names are useful at every depth level
    {
        let symbol_ids: Vec<String> = incoming_rels
            .iter()
            .map(|rel| rel.from_symbol_id.clone())
            .collect();
        enrich_refs(db, &mut incoming, &symbol_ids)?;
    }

    // === Outgoing references (what this symbol calls/uses) ===
    let raw_outgoing = db.get_outgoing_relationships(&symbol.id)?;
    let outgoing_total = raw_outgoing.len();
    let outgoing_calls_total = raw_outgoing
        .iter()
        .filter(|rel| matches!(rel.kind, RelationshipKind::Calls))
        .count();

    let outgoing_rels: Vec<&Relationship> = raw_outgoing.iter().take(outgoing_cap).collect();
    let mut outgoing: Vec<RefEntry> = outgoing_rels
        .iter()
        .map(|rel| RefEntry {
            kind: rel.kind.clone(),
            file_path: rel.file_path.clone(),
            line_number: rel.line_number,
            symbol: None,
        })
        .collect();

    {
        let symbol_ids: Vec<String> = outgoing_rels
            .iter()
            .map(|rel| rel.to_symbol_id.clone())
            .collect();
        enrich_refs(db, &mut outgoing, &symbol_ids)?;
    }

    // Filter noise callees — common names like `new`, `len`, `from` that
    // resolve to wrong symbols because they're too ambiguous
    let pre_filter_len = outgoing.len();
    outgoing.retain(|r| {
        let name = r.symbol.as_ref().map(|s| s.name.as_str()).unwrap_or("");
        !NOISE_CALLEE_NAMES.contains(&name)
    });
    let outgoing_total = outgoing_total.saturating_sub(pre_filter_len - outgoing.len());

    // === Identifier fallback: catch refs that relationships miss ===
    let (incoming, incoming_total, incoming_calls_total) = merge_identifier_refs(
        db,
        symbol,
        incoming,
        incoming_total,
        incoming_calls_total,
        incoming_cap,
    )?;

    debug!(
        "deep_dive: {} incoming (of {}), {} outgoing (of {})",
        incoming.len(),
        incoming_total,
        outgoing.len(),
        outgoing_total
    );

    // === Children (methods, fields for struct/class/trait/enum/module) ===
    let children = if is_container_kind(&symbol.kind) {
        db.get_children_by_parent_id(&symbol.id)?
    } else {
        vec![]
    };

    // === Implementations (for trait/interface) ===
    let implementations = if matches!(symbol.kind, SymbolKind::Interface | SymbolKind::Trait) {
        db.find_type_implementations(&symbol.name, Some(&symbol.language))
            .unwrap_or_default()
    } else {
        vec![]
    };

    // === Primary symbol enrichment (code_context at context/full) ===
    let symbol = if needs_body_enrichment && symbol.code_context.is_none() {
        db.get_symbol_by_id(&symbol.id)?
            .unwrap_or_else(|| symbol.clone())
    } else {
        symbol.clone()
    };

    // === Test locations (context and full depth) ===
    let test_refs = if depth == "full" || depth == "context" {
        build_test_refs(db, &symbol)?
    } else {
        vec![]
    };

    // === Semantically similar symbols (context and full depth) ===
    let similar = if depth == "full" || depth == "context" {
        build_similar(db, &symbol)?
    } else {
        vec![]
    };

    Ok(SymbolContext {
        symbol,
        incoming,
        incoming_total,
        incoming_calls_total,
        outgoing,
        outgoing_total,
        outgoing_calls_total,
        children,
        implementations,
        test_refs,
        similar,
    })
}

fn is_container_kind(kind: &SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Class
            | SymbolKind::Interface
            | SymbolKind::Trait
            | SymbolKind::Enum
            | SymbolKind::Module
            | SymbolKind::Namespace
    )
}

fn symbol_is_test(symbol: &Symbol) -> bool {
    crate::analysis::test_roles::is_test_related(symbol) || is_test_path(&symbol.file_path)
}

/// Build test location refs by querying identifiers linked from test symbols.
fn build_test_refs(db: &SymbolDatabase, symbol: &Symbol) -> Result<Vec<RefEntry>> {
    let names = identifier_names_for_symbol(db, symbol)?;
    let bare_name_allowed = bare_identifier_name_allowed(symbol, &names);
    let ident_refs = db.get_identifiers_by_names(&names)?;

    // Batch-fetch all containing symbols in one query so metadata can drive test
    // detection before the path fallback kicks in.
    let containing_ids: Vec<String> = ident_refs
        .iter()
        .filter_map(|i| i.containing_symbol_id.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let symbol_map: HashMap<String, Symbol> = if containing_ids.is_empty() {
        HashMap::new()
    } else {
        db.get_symbols_by_ids(&containing_ids)
            .unwrap_or_default()
            .into_iter()
            .map(|s| (s.id.clone(), s))
            .collect()
    };

    let test_idents: Vec<_> = ident_refs
        .into_iter()
        .filter(|ident| {
            if ident.file_path == symbol.file_path && ident.start_line == symbol.start_line {
                return false;
            }
            if !identifier_matches_symbol(ident, symbol, &names, bare_name_allowed) {
                return false;
            }

            ident
                .containing_symbol_id
                .as_ref()
                .and_then(|id| symbol_map.get(id))
                .is_some_and(symbol_is_test)
                || is_test_path(&ident.file_path)
        })
        .collect();

    let mut test_refs = Vec::new();
    for ident in test_idents {
        let containing_symbol = ident
            .containing_symbol_id
            .as_ref()
            .and_then(|id| symbol_map.get(id).cloned());

        let rel_kind = match ident.kind.as_str() {
            "call" => RelationshipKind::Calls,
            "import" => RelationshipKind::Imports,
            _ => RelationshipKind::References,
        };

        test_refs.push(RefEntry {
            kind: rel_kind,
            file_path: ident.file_path,
            line_number: ident.start_line,
            symbol: containing_symbol,
        });
    }

    // Deduplicate by (file_path, containing symbol name) — keep first occurrence
    let mut seen = HashSet::new();
    test_refs.retain(|r| {
        let key = (
            r.file_path.clone(),
            r.symbol
                .as_ref()
                .map(|s| s.name.clone())
                .unwrap_or_default(),
        );
        seen.insert(key)
    });

    // Cap to prevent output bloat for common symbol names
    test_refs.truncate(10);

    Ok(test_refs)
}

/// Merge identifier-based references into the incoming list.
///
/// Identifiers catch usage sites that relationship extraction misses (struct type
/// annotations, function calls without extracted relationships, member accesses).
/// Deduplicates against existing relationship-based refs by (file_path, line_number)
/// and filters out the definition site itself.
fn merge_identifier_refs(
    db: &SymbolDatabase,
    symbol: &Symbol,
    mut incoming: Vec<RefEntry>,
    incoming_total: usize,
    incoming_calls_total: usize,
    incoming_cap: usize,
) -> Result<(Vec<RefEntry>, usize, usize)> {
    let names = identifier_names_for_symbol(db, symbol)?;
    let bare_name_allowed = bare_identifier_name_allowed(symbol, &names);
    let ident_refs = db.get_identifiers_by_names(&names)?;

    if ident_refs.is_empty() {
        return Ok((incoming, incoming_total, incoming_calls_total));
    }

    // Build dedup set from existing relationship refs; kept mutable so new entries
    // added below are tracked and don't get added twice from the identifier list.
    let mut existing: HashSet<(String, u32)> = incoming
        .iter()
        .map(|r| (r.file_path.clone(), r.line_number))
        .collect();

    let mut added = 0;
    let mut call_added = 0;
    for ident in ident_refs {
        if !identifier_matches_symbol(&ident, symbol, &names, bare_name_allowed) {
            continue;
        }

        // Skip identifier at the definition site itself
        if ident.file_path == symbol.file_path && ident.start_line == symbol.start_line {
            continue;
        }

        // Skip if already covered by a relationship
        let key = (ident.file_path.clone(), ident.start_line);
        if existing.contains(&key) {
            continue;
        }

        // Respect incoming cap
        if incoming.len() >= incoming_cap {
            added += 1; // Still count towards total
            if ident.kind == "call" {
                call_added += 1;
            }
            continue;
        }

        let rel_kind = match ident.kind.as_str() {
            "call" => RelationshipKind::Calls,
            "import" => RelationshipKind::Imports,
            _ => RelationshipKind::References,
        };
        let is_call = matches!(rel_kind, RelationshipKind::Calls);

        // Enrich with containing symbol if available
        let containing_symbol = ident
            .containing_symbol_id
            .as_ref()
            .and_then(|id| db.get_symbol_by_id(id).ok().flatten());

        incoming.push(RefEntry {
            kind: rel_kind,
            file_path: ident.file_path,
            line_number: ident.start_line,
            symbol: containing_symbol,
        });
        existing.insert(key);
        added += 1;
        if is_call {
            call_added += 1;
        }
    }

    let new_total = incoming_total + added;
    let new_call_total = incoming_calls_total + call_added;
    Ok((incoming, new_total, new_call_total))
}

fn identifier_names_for_symbol(db: &SymbolDatabase, symbol: &Symbol) -> Result<Vec<String>> {
    let mut names = Vec::new();
    push_unique(&mut names, symbol.name.clone());

    let mut qualifiers = Vec::new();
    if let Some(parent_id) = &symbol.parent_id {
        if let Some(parent) = db.get_symbol_by_id(parent_id)? {
            push_unique(&mut qualifiers, parent.name);
        }
    }
    if let Some(impl_type) = impl_type_name(symbol) {
        push_unique(&mut qualifiers, impl_type.to_string());
        push_unique(&mut qualifiers, qualified_name_leaf(impl_type).to_string());
    }

    for qualifier in qualifiers {
        push_unique(&mut names, format!("{}::{}", qualifier, symbol.name));
        push_unique(&mut names, format!("{}.{}", qualifier, symbol.name));
    }

    Ok(names)
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

fn bare_identifier_name_allowed(symbol: &Symbol, names: &[String]) -> bool {
    symbol.parent_id.is_none()
        && impl_type_name(symbol).is_none()
        && names.iter().all(|name| name == &symbol.name)
}

fn identifier_matches_symbol(
    ident: &IdentifierRef,
    symbol: &Symbol,
    names: &[String],
    bare_name_allowed: bool,
) -> bool {
    if ident.target_symbol_id.as_deref() == Some(symbol.id.as_str()) {
        return true;
    }
    if ident.target_symbol_id.is_some() {
        return false;
    }
    if bare_name_allowed && ident.name == symbol.name {
        return true;
    }
    names
        .iter()
        .any(|name| name != &symbol.name && ident.name == *name)
}

/// Enrich ref entries with symbol data by looking up relationship symbol IDs.
fn enrich_refs(db: &SymbolDatabase, refs: &mut [RefEntry], symbol_ids: &[String]) -> Result<()> {
    // Collect unique symbol IDs to fetch
    let unique_symbol_ids: Vec<String> = symbol_ids
        .iter()
        .filter(|id| !id.is_empty())
        .cloned()
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    if unique_symbol_ids.is_empty() {
        return Ok(());
    }

    let symbols = db.get_symbols_by_ids(&unique_symbol_ids)?;
    let symbol_map: HashMap<String, Symbol> =
        symbols.into_iter().map(|s| (s.id.clone(), s)).collect();

    for (r, sym_id) in refs.iter_mut().zip(symbol_ids.iter()) {
        if !sym_id.is_empty() {
            r.symbol = symbol_map.get(sym_id).cloned();
        }
    }

    Ok(())
}

/// Find semantically similar symbols via KNN on stored embeddings.
/// Delegates to the shared similarity module.
fn build_similar(db: &SymbolDatabase, symbol: &Symbol) -> Result<Vec<SimilarEntry>> {
    use crate::search::similarity::{self, MIN_SIMILARITY_SCORE};
    const SIMILAR_LIMIT: usize = 5;
    similarity::find_similar_symbols(db, symbol, SIMILAR_LIMIT, MIN_SIMILARITY_SCORE)
}
