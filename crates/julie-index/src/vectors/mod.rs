//! Symbol vectors served with a snapshot: one row-normalized matrix shared
//! across publishes, bound to the current graph's ids by key. Queries are a
//! brute-force cosine scan; there is no index to maintain.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use julie_core::embeddings_identity::EncoderIdentity;
use julie_facts::rows::{EncoderRow, VectorRow};

use crate::graph::SymbolId;

#[derive(Debug, Default)]
pub struct VectorSet {
    encoder: Option<EncoderRow>,
    keys: Arc<[String]>,
    matrix: Arc<[f32]>,
    dims: usize,
    ids: Vec<Option<SymbolId>>,
    by_id: HashMap<SymbolId, usize>,
    last_scan_micros: AtomicU64,
}

/// The `encoder` row for an identity: its storage key plus the fields that
/// decide vector compatibility.
pub fn encoder_row(identity: &EncoderIdentity) -> anyhow::Result<EncoderRow> {
    Ok(EncoderRow {
        id: identity.storage_key()?,
        model_checksum: identity.weights_sha256.to_ascii_lowercase(),
        dimensions: identity.dimensions as u32,
        pooling: identity.pooling.clone(),
        normalization: identity.normalization.to_ascii_lowercase(),
        instruction_policy: identity.instruction_policy.clone(),
    })
}

fn normalized(vector: &[f32]) -> Option<Vec<f32>> {
    let norm = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
    (norm > 0.0 && norm.is_finite()).then(|| vector.iter().map(|v| v / norm).collect())
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    let mut sum = 0.0;
    for i in 0..a.len() {
        sum += a[i] * b[i];
    }
    sum
}

impl VectorSet {
    pub fn empty() -> Self {
        Self::default()
    }

    /// Normalize every row of `rows` whose length matches the encoder's
    /// dimensions (or the first row's when there is no encoder), then bind.
    pub fn from_rows(
        encoder: Option<EncoderRow>,
        rows: &[VectorRow],
        resolve: impl Fn(&str) -> Option<SymbolId>,
    ) -> Self {
        let dims = encoder
            .as_ref()
            .map(|e| e.dimensions as usize)
            .or_else(|| rows.first().map(|r| r.vector.len()))
            .unwrap_or(0);
        let mut keys = Vec::with_capacity(rows.len());
        let mut matrix = Vec::with_capacity(rows.len() * dims);
        for row in rows {
            if row.vector.len() != dims {
                continue;
            }
            let Some(unit) = normalized(&row.vector) else {
                continue;
            };
            keys.push(row.symbol_id());
            matrix.extend(unit);
        }
        Self {
            encoder,
            keys: keys.into(),
            matrix: matrix.into(),
            dims,
            ids: Vec::new(),
            by_id: HashMap::new(),
            last_scan_micros: AtomicU64::new(0),
        }
        .bind(resolve)
    }

    /// The same rows bound to new ids; rows `resolve` does not know stay
    /// loaded but are skipped by every query. O(rows), no vector copies.
    pub fn bind(&self, resolve: impl Fn(&str) -> Option<SymbolId>) -> Self {
        let ids: Vec<Option<SymbolId>> = self.keys.iter().map(|key| resolve(key)).collect();
        let by_id = ids
            .iter()
            .enumerate()
            .filter_map(|(index, id)| id.map(|id| (id, index)))
            .collect();
        Self {
            encoder: self.encoder.clone(),
            keys: Arc::clone(&self.keys),
            matrix: Arc::clone(&self.matrix),
            dims: self.dims,
            ids,
            by_id,
            last_scan_micros: AtomicU64::new(self.last_scan_micros.load(Ordering::Relaxed)),
        }
    }

    pub fn encoder(&self) -> Option<&EncoderRow> {
        self.encoder.as_ref()
    }

    pub fn dims(&self) -> usize {
        self.dims
    }

    /// Rows bound to a symbol of the current graph.
    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    pub fn contains(&self, id: SymbolId) -> bool {
        self.by_id.contains_key(&id)
    }

    /// The unit-length vector stored for `id`.
    pub fn vector_of(&self, id: SymbolId) -> Option<&[f32]> {
        self.by_id
            .get(&id)
            .map(|index| &self.matrix[index * self.dims..(index + 1) * self.dims])
    }

    /// Most recent scan duration in microseconds, or `None` if `scan` has never run.
    pub fn last_scan_micros(&self) -> Option<u64> {
        match self.last_scan_micros.load(Ordering::Relaxed) {
            0 => None,
            n => Some(n - 1),
        }
    }

    /// The `limit` bound rows nearest to `query` by cosine similarity, best
    /// first. Scores are in `-1.0..=1.0`.
    pub fn scan(&self, query: &[f32], limit: usize) -> Vec<(SymbolId, f32)> {
        let start = std::time::Instant::now();
        let result = self.scan_inner(query, limit);
        let elapsed = start.elapsed().as_micros() as u64;
        self.last_scan_micros.store(elapsed + 1, Ordering::Relaxed);
        result
    }

    fn scan_inner(&self, query: &[f32], limit: usize) -> Vec<(SymbolId, f32)> {
        if self.by_id.is_empty() || limit == 0 || query.len() != self.dims {
            return Vec::new();
        }
        let Some(query) = normalized(query) else {
            return Vec::new();
        };
        let mut scored: Vec<(SymbolId, f32)> = self
            .ids
            .iter()
            .enumerate()
            .filter_map(|(index, id)| {
                let row = &self.matrix[index * self.dims..(index + 1) * self.dims];
                id.map(|id| (id, dot(&query, row)))
            })
            .collect();
        let keep = limit.min(scored.len());
        if keep < scored.len() {
            scored.select_nth_unstable_by(keep, |a, b| b.1.total_cmp(&a.1));
            scored.truncate(keep);
        }
        scored.sort_by(|a, b| b.1.total_cmp(&a.1));
        scored
    }
}
