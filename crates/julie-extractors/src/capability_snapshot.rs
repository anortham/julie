//! Stable, downstream-readable capability declaration.
//!
//! Loads from `fixtures/extraction/capabilities.json` (single source of truth)
//! via `include_str!` at compile time. No build script. Path-dep and git-dep
//! consumers work because cargo serves the whole repo at its on-disk location.
//! This crate is intentionally NOT `cargo publish`-able to crates.io — see
//! `docs/plans/2026-05-10-best-in-class-tree-sitter-plan.md` Architecture
//! Quality section for the Pillar-3 scope decision.

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

const CAPABILITIES_JSON: &str = include_str!("../../../fixtures/extraction/capabilities.json");

#[derive(Debug, Deserialize)]
pub struct CapabilitySnapshot {
    languages: Vec<CapabilityRow>,
    #[serde(skip)]
    by_name: HashMap<String, usize>,
}

#[derive(Debug, Deserialize)]
pub struct CapabilityRow {
    pub language: String,
    pub parser_crate: String,
    pub extensions: Vec<String>,
    pub dependency_status: String,
    pub target_capabilities: CapabilityFlags,
    pub capabilities: CapabilityFlags,
    #[serde(default)]
    pub kind_coverage: CapabilityKindCoverage,
    pub fixtures: Vec<FixtureRef>,
    #[serde(default)]
    pub capability_gaps: Vec<CapabilityGap>,
}

#[derive(Debug, Deserialize, Clone, Copy)]
pub struct CapabilityFlags {
    pub symbols: bool,
    pub relationships: bool,
    pub pending_relationships: bool,
    pub identifiers: bool,
    pub types: bool,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct CapabilityKindCoverage {
    #[serde(default)]
    pub symbols: KindCoverage,
    #[serde(default)]
    pub relationships: KindCoverage,
    #[serde(default)]
    pub identifiers: KindCoverage,
    #[serde(default)]
    pub body_spans: KindCoverage,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct KindCoverage {
    #[serde(default)]
    pub supported: Vec<String>,
    #[serde(default)]
    pub not_applicable: Vec<String>,
    #[serde(default)]
    pub open_gaps: Vec<KindCoverageGap>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct KindCoverageGap {
    pub kind: String,
    pub reason: String,
    pub required_closure: String,
    pub planned_closure_task: String,
}

#[derive(Debug, Deserialize)]
pub struct FixtureRef {
    pub name: String,
    pub source: String,
    pub expected: String,
}

#[derive(Debug, Deserialize)]
pub struct CapabilityGap {
    pub capability: String,
    pub status: String,
    pub reason: String,
    pub required_closure: String,
    pub evidence: serde_json::Value,
}

impl CapabilitySnapshot {
    pub fn from_json_str(json: &str) -> Result<Self, serde_json::Error> {
        let mut snap: CapabilitySnapshot = serde_json::from_str(json)?;
        snap.rebuild_index();
        Ok(snap)
    }

    pub fn languages(&self) -> impl Iterator<Item = &CapabilityRow> {
        self.languages.iter()
    }

    pub fn get(&self, language: &str) -> Option<&CapabilityRow> {
        self.by_name.get(language).map(|&i| &self.languages[i])
    }

    fn rebuild_index(&mut self) {
        self.by_name = self
            .languages
            .iter()
            .enumerate()
            .map(|(i, row)| (row.language.clone(), i))
            .collect();
    }
}

pub fn capability_snapshot() -> &'static CapabilitySnapshot {
    static SNAPSHOT: OnceLock<CapabilitySnapshot> = OnceLock::new();
    SNAPSHOT.get_or_init(|| {
        CapabilitySnapshot::from_json_str(CAPABILITIES_JSON)
            .expect("capabilities.json must be valid JSON matching the snapshot schema")
    })
}
