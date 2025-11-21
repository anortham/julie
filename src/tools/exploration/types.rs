use serde::Serialize;

/// Business logic symbol result (simpler than full Symbol)
///
/// CRITICAL: No #[serde(skip_serializing_if)] attributes!
/// TOON requires ALL objects to have IDENTICAL key sets for tabular encoding.
///
/// NOTE: confidence is Option<f32> not f32 because TOON library can't encode bare f32!
/// This matches ToonSymbol pattern from fast_search.
#[derive(Debug, Clone, Serialize)]
pub struct BusinessLogicSymbol {
    pub name: String,
    pub kind: String,
    pub language: String,
    pub file_path: String,
    pub start_line: u32,
    pub confidence: Option<f32>, // MUST be Option for TOON compatibility (always Some in practice)
    pub signature: Option<String>, // Always serialized (null if None) for TOON compatibility
}

/// Structured result from find_logic operation
#[derive(Debug, Clone, Serialize)]
pub struct FindLogicResult {
    pub tool: String,
    pub domain: String,
    pub found_count: usize,
    pub max_results: usize,
    pub min_business_score: f32,
    pub group_by_layer: bool,
    pub intelligence_layers: Vec<String>,
    pub business_symbols: Vec<BusinessLogicSymbol>,
    pub next_actions: Vec<String>,
}

impl FindLogicResult {
    /// Convert to flat structure for TOON encoding
    ///
    /// TOON can't handle the full result structure with multiple Vec fields.
    /// Extract just the business_symbols array for efficient tabular encoding.
    ///
    /// Returns owned Vec to match pattern of get_symbols and trace_call_path.
    pub fn to_toon_flat(&self) -> Vec<BusinessLogicSymbol> {
        self.business_symbols.clone()
    }
}
