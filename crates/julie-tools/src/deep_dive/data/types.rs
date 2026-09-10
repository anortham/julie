use julie_core::Symbol;
use julie_extractors::RelationshipKind;
use julie_facts::rows::ComplexityRow;
pub use julie_index::search::similarity::SimilarEntry;

/// Aggregated context for a single symbol, ready for formatting
#[derive(Debug)]
pub struct SymbolContext {
    /// The primary symbol being investigated
    pub symbol: Symbol,
    /// Extractor-provided structural complexity metric
    pub complexity: Option<ComplexityRow>,
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

/// A reference entry with optional enriched symbol data
#[derive(Debug, Clone)]
pub struct RefEntry {
    pub kind: RelationshipKind,
    pub file_path: String,
    pub line_number: u32,
    /// The source/target symbol (enriched at all depth levels)
    pub symbol: Option<Symbol>,
}
