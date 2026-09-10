mod apply;
mod document;
mod facts_text;
pub mod from_facts;

pub const TANTIVY_PROJECTION_NAME: &str = "tantivy";

pub struct SearchProjection {
    _workspace_id: String,
    _projection: &'static str,
}

impl SearchProjection {
    pub fn tantivy(workspace_id: impl Into<String>) -> Self {
        Self {
            _workspace_id: workspace_id.into(),
            _projection: TANTIVY_PROJECTION_NAME,
        }
    }
}
