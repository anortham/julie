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
    /// True when an omitted `backend` should run semantic symbol search: the
    /// query reads as natural language and does not name a file or path.
    pub fn auto_prefers_semantic(query: &str) -> bool {
        julie_index::search::scoring::is_nl_like_query(query)
            && !crate::search::query::looks_like_file_or_path_query(query)
    }

    pub fn resolve(requested: Option<Self>, query: &str) -> ResolvedSearchBackend {
        let value = match requested {
            Some(value) => value,
            None if Self::auto_prefers_semantic(query) => Self::Semantic,
            None => Self::Lexical,
        };
        ResolvedSearchBackend {
            value,
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
