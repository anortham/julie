use serde::Serialize;

/// Business logic symbol result (simpler than full Symbol)
#[derive(Debug, Clone, Serialize)]
pub struct BusinessLogicSymbol {
    pub name: String,
    pub kind: String,
    pub language: String,
    pub file_path: String,
    pub start_line: u32,
    pub confidence: f32, // Business relevance score
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
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
