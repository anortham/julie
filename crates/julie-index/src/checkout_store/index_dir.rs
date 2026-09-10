//! The Tantivy directory beside `facts.sqlite`. `julie.meta.json` records the
//! schema signature and engine version; any difference discards the directory
//! and the store rebuilds it from facts.

use std::path::Path;

use anyhow::{Context, Result};
use julie_facts::version::SEMANTIC_INDEX_ENGINE_VERSION;
use serde::{Deserialize, Serialize};
use tantivy::Index;
use tantivy::tokenizer::TextAnalyzer;
use tracing::warn;

use super::TantivyState;
use crate::search::LanguageConfigs;
use crate::search::schema::{SchemaCompatibilitySignature, compatibility_signature, create_schema};
use crate::search::tokenizer::{CodeTokenizer, SimpleCodeTokenizer};

pub const META_FILE: &str = "julie.meta.json";

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Meta {
    schema_signature: SchemaCompatibilitySignature,
    engine_version: String,
}

fn expected_meta() -> Meta {
    Meta {
        schema_signature: compatibility_signature(&create_schema()),
        engine_version: SEMANTIC_INDEX_ENGINE_VERSION.to_string(),
    }
}

fn register_tokenizers(index: &Index) {
    let code = CodeTokenizer::from_language_configs(&LanguageConfigs::load_embedded());
    index
        .tokenizers()
        .register("code", TextAnalyzer::builder(code).build());
    index.tokenizers().register(
        "simple_code",
        TextAnalyzer::builder(SimpleCodeTokenizer::new()).build(),
    );
}

pub fn in_ram() -> Index {
    let index = Index::create_in_ram(create_schema());
    register_tokenizers(&index);
    index
}

fn open_current(dir: &Path, expected: &Meta) -> Option<Index> {
    let raw = std::fs::read_to_string(dir.join(META_FILE)).ok()?;
    let found: Meta = serde_json::from_str(&raw).ok()?;
    if found != *expected {
        return None;
    }
    let index = Index::open_in_dir(dir).ok()?;
    (compatibility_signature(&index.schema()) == expected.schema_signature).then_some(index)
}

fn create(dir: &Path, expected: &Meta) -> Result<Index> {
    std::fs::create_dir_all(dir)
        .with_context(|| format!("create tantivy dir {}", dir.display()))?;
    let index = Index::create_in_dir(dir, create_schema())?;
    std::fs::write(dir.join(META_FILE), serde_json::to_string_pretty(expected)?)?;
    Ok(index)
}

/// Open `dir` when its meta matches, else discard and create it empty.
pub fn open_or_create(dir: &Path) -> Result<(Index, TantivyState)> {
    let expected = expected_meta();
    let (index, state) = match open_current(dir, &expected) {
        Some(index) => (index, TantivyState::Present),
        None if dir.exists() => {
            warn!(dir = %dir.display(), "tantivy directory is stale; rebuilding from facts");
            std::fs::remove_dir_all(dir)
                .with_context(|| format!("remove stale tantivy dir {}", dir.display()))?;
            (create(dir, &expected)?, TantivyState::Stale)
        }
        None => (create(dir, &expected)?, TantivyState::Absent),
    };
    register_tokenizers(&index);
    Ok((index, state))
}
