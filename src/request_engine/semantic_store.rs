//! Vector readiness verification against `facts.sqlite`: the encoder row must
//! match the running provider and the current paths must hold vectors.

use serde_json::json;
use std::path::Path;

use julie_facts::FactsStore;

use crate::embeddings::EmbeddingProvider;
use crate::request_engine::semantic::{SemanticMode, SemanticReadiness};
use crate::request_engine::types::RequestFailure;

fn not_ready(
    mode: SemanticMode,
    message: impl Into<String>,
    details: serde_json::Value,
    reason: &str,
) -> Result<SemanticReadiness, RequestFailure> {
    match mode {
        SemanticMode::Required => Err(RequestFailure::semantics_not_ready(message, details)),
        SemanticMode::Auto => Ok(SemanticReadiness::Degraded {
            reason: reason.to_string(),
            retryable: true,
        }),
        SemanticMode::Off => Ok(SemanticReadiness::Disabled),
    }
}

/// Verifies that `facts_path` holds vectors the running provider can query.
pub fn check_facts_vectors(
    facts_path: &Path,
    provider: &dyn EmbeddingProvider,
    mode: SemanticMode,
) -> Result<SemanticReadiness, RequestFailure> {
    if !facts_path.exists() {
        return not_ready(
            mode,
            "Workspace facts database does not exist",
            json!({ "coverage": "missing" }),
            "DATABASE_MISSING",
        );
    }
    let store = FactsStore::open_read_only(facts_path)
        .map_err(|e| RequestFailure::internal(format!("Failed to open facts database: {e}")))?;
    let reader = store.reader();

    let dev_info = provider.device_info();
    let provider_dims = provider.dimensions();
    let encoder_identity = match provider.encoder_identity() {
        Ok(id) => id,
        Err(e) => {
            return not_ready(
                mode,
                format!("Encoder identity unavailable: {e}"),
                json!({ "coverage": "missing_identity" }),
                "ENCODER_IDENTITY_UNAVAILABLE",
            );
        }
    };
    let expected_key = match encoder_identity.storage_key() {
        Ok(key) => key,
        Err(e) => {
            return not_ready(
                mode,
                format!("Failed to compute canonical encoder storage key: {e}"),
                json!({ "coverage": "invalid_identity" }),
                "ENCODER_STORAGE_KEY_INVALID",
            );
        }
    };

    let symbol_count = reader.symbol_count().unwrap_or(0) as usize;
    let Some(stored) = reader.encoder().ok().flatten() else {
        if symbol_count == 0 {
            return ready(dev_info, provider_dims, encoder_identity, 0, 0);
        }
        return not_ready(
            mode,
            "Workspace has symbols but no encoder row",
            json!({ "coverage": "missing" }),
            "VECTORS_MISSING",
        );
    };
    if stored.dimensions as usize != provider_dims || stored.id != expected_key {
        return not_ready(
            mode,
            format!(
                "Stored vectors are incompatible: stored {} ({}d) vs expected {} ({}d)",
                stored.id, stored.dimensions, expected_key, provider_dims
            ),
            json!({
                "coverage": "incompatible",
                "stored_model": stored.id,
                "stored_dimensions": stored.dimensions,
                "expected_encoder_key": expected_key,
                "provider_model": dev_info.model_name,
                "provider_dimensions": provider_dims,
            }),
            "VECTORS_INCOMPATIBLE",
        );
    }

    let vector_count = reader.vector_count().unwrap_or(0) as usize;
    if symbol_count > 0 && vector_count == 0 {
        return not_ready(
            mode,
            "Workspace has symbols but zero vector embeddings",
            json!({ "coverage": "missing" }),
            "VECTORS_MISSING",
        );
    }
    ready(
        dev_info,
        provider_dims,
        encoder_identity,
        symbol_count,
        vector_count,
    )
}

fn ready(
    dev_info: crate::embeddings::DeviceInfo,
    dimensions: usize,
    encoder_identity: julie_core::EncoderIdentity,
    eligible_symbols: usize,
    embedded_symbols: usize,
) -> Result<SemanticReadiness, RequestFailure> {
    Ok(SemanticReadiness::Ready {
        model_id: dev_info.model_name,
        dimensions,
        device: dev_info.device,
        encoder_identity: Some(encoder_identity),
        vector_generation: None,
        eligible_symbols,
        embedded_symbols,
    })
}
