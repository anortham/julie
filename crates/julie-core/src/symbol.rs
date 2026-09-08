use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::ops::{Deref, DerefMut};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Symbol {
    #[serde(flatten)]
    pub extracted: julie_extractors::Symbol,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code_context: Option<String>,
}

impl Symbol {
    pub fn from_extracted(extracted: julie_extractors::Symbol, source: &str) -> Result<Self> {
        let start = extracted.start_byte as usize;
        let end = extracted.end_byte as usize;
        let text = source.get(start..end).with_context(|| {
            format!(
                "invalid symbol span {}:{}..{}",
                extracted.file_path, start, end
            )
        })?;
        let code_context = (!text.is_empty()).then(|| text.to_owned());
        Ok(Self {
            extracted,
            code_context,
        })
    }
}

impl From<julie_extractors::Symbol> for Symbol {
    fn from(extracted: julie_extractors::Symbol) -> Self {
        Self {
            extracted,
            code_context: None,
        }
    }
}

impl Deref for Symbol {
    type Target = julie_extractors::Symbol;
    fn deref(&self) -> &Self::Target {
        &self.extracted
    }
}

impl DerefMut for Symbol {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.extracted
    }
}
