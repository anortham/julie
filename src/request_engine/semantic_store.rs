use serde_json::json;

use julie_index::search::language_config::LanguageConfigs;
use julie_index::snapshot::Snapshot;
use julie_pipeline::embeddings::pipeline::eligible_vector_coverage;

use crate::embeddings::EmbeddingProvider;
use crate::request_engine::semantic::{SemanticMode, SemanticReadiness};
use crate::request_engine::types::RequestFailure;

fn not_ready(
    mode: SemanticMode,
    message: impl Into<String>,
    details: serde_json::Value,
    reason: &str,
    coverage: Option<(usize, usize)>,
) -> Result<SemanticReadiness, RequestFailure> {
    match mode {
        SemanticMode::Required => Err(RequestFailure::semantics_not_ready(message, details)),
        SemanticMode::Auto => Ok(SemanticReadiness::Degraded {
            reason: reason.to_string(),
            retryable: true,
            eligible_symbols: coverage.map(|value| value.0),
            embedded_symbols: coverage.map(|value| value.1),
        }),
        SemanticMode::Off => Ok(SemanticReadiness::Disabled),
    }
}

pub fn check_facts_vectors(
    snapshot: &Snapshot,
    lang_configs: &LanguageConfigs,
    provider: &dyn EmbeddingProvider,
    mode: SemanticMode,
) -> Result<SemanticReadiness, RequestFailure> {
    let dev_info = provider.device_info();
    let provider_dims = provider.dimensions();
    let encoder_identity = match provider.encoder_identity() {
        Ok(identity) => identity,
        Err(error) => {
            return not_ready(
                mode,
                format!("Encoder identity unavailable: {error}"),
                json!({ "coverage": "missing_identity" }),
                "ENCODER_IDENTITY_UNAVAILABLE",
                None,
            );
        }
    };
    let expected_key = match encoder_identity.storage_key() {
        Ok(key) => key,
        Err(error) => {
            return not_ready(
                mode,
                format!("Failed to compute canonical encoder storage key: {error}"),
                json!({ "coverage": "invalid_identity" }),
                "ENCODER_STORAGE_KEY_INVALID",
                None,
            );
        }
    };

    let (eligible, embedded) = eligible_vector_coverage(snapshot, Some(lang_configs));
    if eligible == 0 {
        return ready(dev_info, provider_dims, encoder_identity, 0, 0);
    }

    let vectors = snapshot.vectors();
    let Some(stored) = vectors.encoder() else {
        return not_ready(
            mode,
            "Workspace has eligible symbols but no encoder row",
            json!({ "coverage": "missing", "eligible_symbols": eligible, "embedded_symbols": 0 }),
            "VECTORS_MISSING",
            Some((eligible, 0)),
        );
    };
    if stored.dimensions as usize != provider_dims || stored.id != expected_key {
        return not_ready(
            mode,
            format!(
                "Stored vectors are incompatible: stored {} ({}d) vs expected {} ({}d)",
                stored.id, stored.dimensions, expected_key, provider_dims
            ),
            json!({ "coverage": "incompatible", "stored_model": stored.id, "stored_dimensions": stored.dimensions, "expected_encoder_key": expected_key, "provider_model": dev_info.model_name, "provider_dimensions": provider_dims, "eligible_symbols": eligible, "embedded_symbols": 0 }),
            "VECTORS_INCOMPATIBLE",
            Some((eligible, 0)),
        );
    }

    if embedded == 0 {
        return not_ready(
            mode,
            "Workspace has eligible symbols but zero vector embeddings",
            json!({ "coverage": "missing", "eligible_symbols": eligible, "embedded_symbols": 0 }),
            "VECTORS_MISSING",
            Some((eligible, 0)),
        );
    }
    if embedded < eligible {
        return not_ready(
            mode,
            format!(
                "Workspace vector coverage is incomplete: {embedded}/{eligible} eligible symbols"
            ),
            json!({ "coverage": "partial", "eligible_symbols": eligible, "embedded_symbols": embedded }),
            "VECTORS_PARTIAL",
            Some((eligible, embedded)),
        );
    }
    ready(
        dev_info,
        provider_dims,
        encoder_identity,
        eligible,
        embedded,
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
        vector_generation: None,
        encoder_identity: Some(encoder_identity),
        eligible_symbols,
        embedded_symbols,
    })
}
