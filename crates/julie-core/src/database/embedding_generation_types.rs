use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EmbeddingGenerationStatus {
    Building,
    Ready,
    Failed,
    Stale,
    Superseded,
}

impl EmbeddingGenerationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Building => "building",
            Self::Ready => "ready",
            Self::Failed => "failed",
            Self::Stale => "stale",
            Self::Superseded => "superseded",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "building" => Some(Self::Building),
            "ready" => Some(Self::Ready),
            "failed" => Some(Self::Failed),
            "stale" => Some(Self::Stale),
            "superseded" => Some(Self::Superseded),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmbeddingGeneration {
    pub id: i64,
    pub encoder_key: String,
    pub source_revision: i64,
    pub dimensions: usize,
    pub status: EmbeddingGenerationStatus,
    pub eligible_symbols: usize,
    pub embedded_symbols: usize,
    pub created_at: i64,
    pub updated_at: i64,
}

impl EmbeddingGeneration {
    pub fn is_ready(&self) -> bool {
        self.status == EmbeddingGenerationStatus::Ready
    }

    pub fn is_complete(&self) -> bool {
        self.is_ready() && self.embedded_symbols >= self.eligible_symbols
    }

    pub fn coverage_ratio(&self) -> f64 {
        if self.eligible_symbols == 0 {
            1.0
        } else {
            (self.embedded_symbols as f64 / self.eligible_symbols as f64).min(1.0)
        }
    }
}
