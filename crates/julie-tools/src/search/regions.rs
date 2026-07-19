use anyhow::{Result, anyhow};
use julie_extractors::base::SourceRegionKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRegionFilter(pub Vec<SourceRegionKind>);

impl SourceRegionFilter {
    pub fn parse(value: &str) -> Result<Self> {
        let mut kinds = Vec::new();
        for raw in value.split(',') {
            let kind = match raw.trim().to_ascii_lowercase().as_str() {
                "comment" => SourceRegionKind::Comment,
                "doc_comment" | "docstring" => SourceRegionKind::DocComment,
                "string_literal" => SourceRegionKind::StringLiteral,
                "embedded" => SourceRegionKind::Embedded,
                "" => continue,
                unknown => return Err(anyhow!("unknown source region: {unknown}")),
            };
            if !kinds.contains(&kind) {
                kinds.push(kind);
            }
        }
        if kinds.is_empty() {
            return Err(anyhow!(
                "regions must contain at least one source region"
            ));
        }
        Ok(Self(kinds))
    }
}
