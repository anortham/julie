use serde::Serialize;

/// Business logic symbol result (simpler than full Symbol)
#[derive(Debug, Clone, Serialize)]
pub struct BusinessLogicSymbol {
    pub name: String,
    pub kind: String,
    pub language: String,
    pub file_path: String,
    pub start_line: u32,
    pub confidence: Option<f32>,
    pub signature: Option<String>,
}
