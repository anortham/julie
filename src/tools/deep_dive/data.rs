//! Data gathering for deep_dive tool
//!
//! Collects symbol context from SQLite: relationships, children, types.
//! All queries use existing indexed data — no new indexing required.

use anyhow::Result;
use std::collections::{HashMap, HashSet};
use tracing::debug;

use crate::database::SymbolDatabase;
use crate::extractors::base::{RelationshipKind, Symbol, SymbolKind};
use crate::search::scoring::is_test_path;
use crate::tools::navigation::resolution::parse_qualified_name;
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
    /// Outgoing references: what this symbol calls/uses
    pub outgoing: Vec<RefEntry>,
    /// Total outgoing before capping
    pub outgoing_total: usize,
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
        full_name_results.retain(|s| s.kind != SymbolKind::Import && s.kind != SymbolKind::Export);
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
        candidates.retain(|s| s.kind != SymbolKind::Import);

        // Find parent symbols by name to collect their IDs
        let parents = db.find_symbols_by_name(parent_name)?;
        let parent_ids: std::collections::HashSet<&str> =
            parents.iter().map(|p| p.id.as_str()).collect();

        let qualified: Vec<Symbol> = candidates
            .iter()
            .filter(|s| {
                s.parent_id
                    .as_deref()
                    .map_or(false, |pid| parent_ids.contains(pid))
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
    symbols.retain(|s| s.kind != SymbolKind::Import);
    apply_context_file_filter(symbols, context_file)
}

/// Filter symbols by context_file if provided, falling back to full list.
fn apply_context_file_filter(
    symbols: Vec<Symbol>,
    context_file: Option<&str>,
) -> Result<Vec<Symbol>> {
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
            .map(|r| {
                (
                    format!("{}:{}", r.file_path, r.line_number),
                    r.from_symbol_id.clone(),
                )
            })
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
            .map(|r| {
                (
                    format!("{}:{}", r.file_path, r.line_number),
                    r.to_symbol_id.clone(),
                )
            })
            .collect();
        enrich_refs(db, &mut outgoing, &id_map)?;
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
    let (incoming, incoming_total) =
        merge_identifier_refs(db, symbol, incoming, incoming_total, incoming_cap)?;

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
        outgoing,
        outgoing_total,
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

/// Build test location refs by querying identifiers in test files.
fn build_test_refs(db: &SymbolDatabase, symbol: &Symbol) -> Result<Vec<RefEntry>> {
    let names = vec![symbol.name.clone()];
    let ident_refs = db.get_identifiers_by_names(&names)?;

    // Filter to test-file identifiers first, then batch-fetch containing symbols
    // in a single query instead of one per identifier.
    let test_idents: Vec<_> = ident_refs
        .into_iter()
        .filter(|ident| {
            is_test_path(&ident.file_path)
                && !(ident.file_path == symbol.file_path && ident.start_line == symbol.start_line)
        })
        .collect();

    // Batch-fetch all containing symbols in one query.
    let containing_ids: Vec<String> = test_idents
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
    incoming_cap: usize,
) -> Result<(Vec<RefEntry>, usize)> {
    let names = vec![symbol.name.clone()];
    let ident_refs = db.get_identifiers_by_names(&names)?;

    if ident_refs.is_empty() {
        return Ok((incoming, incoming_total));
    }

    // Build dedup set from existing relationship refs; kept mutable so new entries
    // added below are tracked and don't get added twice from the identifier list.
    let mut existing: HashSet<(String, u32)> = incoming
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
        existing.insert(key);
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

/// Find semantically similar symbols via KNN on stored embeddings.
/// Delegates to the shared similarity module.
fn build_similar(db: &SymbolDatabase, symbol: &Symbol) -> Result<Vec<SimilarEntry>> {
    use crate::search::similarity::{self, MIN_SIMILARITY_SCORE};
    const SIMILAR_LIMIT: usize = 5;
    similarity::find_similar_symbols(db, symbol, SIMILAR_LIMIT, MIN_SIMILARITY_SCORE)
}
