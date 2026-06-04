use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SearchBackend {
    Lexical,
    Semantic,
    Hybrid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedSearchBackend {
    pub value: SearchBackend,
    pub explicit: bool,
}

impl SearchBackend {
    pub fn resolve(requested: Option<Self>) -> ResolvedSearchBackend {
        ResolvedSearchBackend {
            value: requested.unwrap_or(Self::Lexical),
            explicit: requested.is_some(),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lexical => "lexical",
            Self::Semantic => "semantic",
            Self::Hybrid => "hybrid",
        }
    }
}
