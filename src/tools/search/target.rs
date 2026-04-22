use anyhow::{Result, bail};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SearchTarget {
    Content,
    Definitions,
    Files,
}

impl SearchTarget {
    pub(crate) fn parse(raw: &str) -> Result<Self> {
        match raw {
            "content" => Ok(Self::Content),
            "definitions" => Ok(Self::Definitions),
            "files" | "paths" => Ok(Self::Files),
            other => bail!(
                "Invalid search_target: '{}'. Expected one of: content, definitions, files",
                other
            ),
        }
    }

    pub(crate) fn canonical_name(self) -> &'static str {
        match self {
            Self::Content => "content",
            Self::Definitions => "definitions",
            Self::Files => "files",
        }
    }
}
