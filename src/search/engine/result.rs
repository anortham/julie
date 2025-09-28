#[derive(Debug, Clone)]
pub struct SearchResult {
    pub symbol: crate::extractors::base::Symbol,
    pub score: f32,
    pub snippet: String,
}
