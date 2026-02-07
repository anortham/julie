//! Data gathering for deep_dive tool
//!
//! Collects symbol context from SQLite: relationships, children, types.
//! All queries use existing indexed data — no new indexing required.

use anyhow::Result;
use std::collections::{HashMap, HashSet};
use tracing::debug;

use crate::database::SymbolDatabase;
use crate::extractors::base::{RelationshipKind, Symbol, SymbolKind};

/// Aggregated context for a single symbol, ready for formatting
#[derive(Debug)]
pub struct SymbolContext {
    /// The primary symbol being investigated
    pub symbol: Symbol,
    /// Incoming references: who calls/uses this symbol
    pub incoming: Vec<RefEntry>,
    /// Total incoming before capping
    pub incoming_total: usize,
    /// Outgoing references: what this symbol calls/uses
    pub outgoing: Vec<RefEntry>,
    /// Total outgoing before capping
    pub outgoing_total: usize,
    /// Child symbols (methods, fields) for struct/class/trait/enum
    pub children: Vec<Symbol>,
    /// Implementations of this trait/interface
    pub implementations: Vec<Symbol>,
    /// Test file references (populated at full depth only)
    pub test_refs: Vec<RefEntry>,
}

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
pub fn find_symbol(
    db: &SymbolDatabase,
    name: &str,
    context_file: Option<&str>,
) -> Result<Vec<Symbol>> {
    let mut symbols = db.find_symbols_by_name(name)?;

    // Filter out imports — we want actual definitions
    symbols.retain(|s| s.kind != SymbolKind::Import);

    // Disambiguate by file if specified
    if let Some(file) = context_file {
        let file_matches: Vec<Symbol> = symbols
            .iter()
            .filter(|s| s.file_path.contains(file))
            .cloned()
            .collect();
        if !file_matches.is_empty() {
            return Ok(file_matches);
        }
    }

    Ok(symbols)
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

    let mut incoming: Vec<RefEntry> = raw_incoming
        .iter()
        .take(incoming_cap)
        .map(|rel| RefEntry {
            kind: rel.kind.clone(),
            file_path: rel.file_path.clone(),
            line_number: rel.line_number,
            symbol: None,
        })
        .collect();

    // Always enrich refs — symbol names are useful at every depth level
    {
        let id_map: HashMap<String, String> = raw_incoming
            .iter()
            .map(|r| (format!("{}:{}", r.file_path, r.line_number), r.from_symbol_id.clone()))
            .collect();
        enrich_refs(db, &mut incoming, &id_map)?;
    }

    // === Outgoing references (what this symbol calls/uses) ===
    let raw_outgoing = db.get_outgoing_relationships(&symbol.id)?;
    let outgoing_total = raw_outgoing.len();

    let mut outgoing: Vec<RefEntry> = raw_outgoing
        .iter()
        .take(outgoing_cap)
        .map(|rel| RefEntry {
            kind: rel.kind.clone(),
            file_path: rel.file_path.clone(),
            line_number: rel.line_number,
            symbol: None,
        })
        .collect();

    {
        let id_map: HashMap<String, String> = raw_outgoing
            .iter()
            .map(|r| (format!("{}:{}", r.file_path, r.line_number), r.to_symbol_id.clone()))
            .collect();
        enrich_refs(db, &mut outgoing, &id_map)?;
    }

    // === Identifier fallback: catch refs that relationships miss ===
    let (incoming, incoming_total) = merge_identifier_refs(
        db, symbol, incoming, incoming_total, incoming_cap,
    )?;

    debug!(
        "deep_dive: {} incoming (of {}), {} outgoing (of {})",
        incoming.len(), incoming_total, outgoing.len(), outgoing_total
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

    // === Test locations (full depth only) ===
    let test_refs = if depth == "full" {
        build_test_refs(db, &symbol)?
    } else {
        vec![]
    };

    Ok(SymbolContext {
        symbol,
        incoming,
        incoming_total,
        outgoing,
        outgoing_total,
        children,
        implementations,
        test_refs,
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

/// Check if a file path looks like a test file.
pub(crate) fn is_test_file(path: &str) -> bool {
    // Directory patterns
    path.contains("/tests/")
        || path.contains("/test/")
        || path.contains("/__tests__/")
        // File suffix patterns
        || path.ends_with("_test.rs")
        || path.ends_with("_tests.rs")
        || path.ends_with("_test.go")
        || path.ends_with("_test.py")
        || path.ends_with(".test.ts")
        || path.ends_with(".test.js")
        || path.ends_with(".test.tsx")
        || path.ends_with(".test.jsx")
        || path.ends_with(".spec.ts")
        || path.ends_with(".spec.js")
        // File prefix patterns
        || path.split('/').last().map_or(false, |f| f.starts_with("test_"))
}

/// Build test location refs by querying identifiers in test files.
fn build_test_refs(db: &SymbolDatabase, symbol: &Symbol) -> Result<Vec<RefEntry>> {
    let names = vec![symbol.name.clone()];
    let ident_refs = db.get_identifiers_by_names(&names)?;

    let mut test_refs = Vec::new();
    for ident in ident_refs {
        if !is_test_file(&ident.file_path) {
            continue;
        }
        // Skip definition site
        if ident.file_path == symbol.file_path && ident.start_line == symbol.start_line {
            continue;
        }

        let containing_symbol = ident
            .containing_symbol_id
            .as_ref()
            .and_then(|id| db.get_symbol_by_id(id).ok().flatten());

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
    incoming_cap: usize,
) -> Result<(Vec<RefEntry>, usize)> {
    let names = vec![symbol.name.clone()];
    let ident_refs = db.get_identifiers_by_names(&names)?;

    if ident_refs.is_empty() {
        return Ok((incoming, incoming_total));
    }

    // Build dedup set from existing relationship refs
    let existing: HashSet<(String, u32)> = incoming
        .iter()
        .map(|r| (r.file_path.clone(), r.line_number))
        .collect();

    let mut added = 0;
    for ident in ident_refs {
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
            continue;
        }

        let rel_kind = match ident.kind.as_str() {
            "call" => RelationshipKind::Calls,
            "import" => RelationshipKind::Imports,
            _ => RelationshipKind::References,
        };

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
        added += 1;
    }

    let new_total = incoming_total + added;
    Ok((incoming, new_total))
}

/// Enrich ref entries with symbol data by looking up IDs from the relationship map
fn enrich_refs(
    db: &SymbolDatabase,
    refs: &mut [RefEntry],
    id_map: &HashMap<String, String>,
) -> Result<()> {
    // Collect unique symbol IDs to fetch
    let symbol_ids: Vec<String> = refs
        .iter()
        .filter_map(|r| {
            let key = format!("{}:{}", r.file_path, r.line_number);
            id_map.get(&key).filter(|id| !id.is_empty()).cloned()
        })
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    if symbol_ids.is_empty() {
        return Ok(());
    }

    let symbols = db.get_symbols_by_ids(&symbol_ids)?;
    let symbol_map: HashMap<String, Symbol> =
        symbols.into_iter().map(|s| (s.id.clone(), s)).collect();

    for r in refs.iter_mut() {
        let key = format!("{}:{}", r.file_path, r.line_number);
        if let Some(sym_id) = id_map.get(&key) {
            r.symbol = symbol_map.get(sym_id).cloned();
        }
    }

    Ok(())
}
