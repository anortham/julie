//! SQLite vector storage and generation readiness verification.

use serde_json::json;
use std::path::Path;

use crate::embeddings::EmbeddingProvider;
use crate::request_engine::semantic::{
    CURRENT_EMBEDDING_FORMAT_VERSION, SemanticMode, SemanticReadiness,
};
use crate::request_engine::types::RequestFailure;

/// Verifies SQLite vector storage, configuration, and generation readiness.
pub fn check_sqlite_vectors(
    db_path: &Path,
    provider: &dyn EmbeddingProvider,
    mode: SemanticMode,
) -> Result<SemanticReadiness, RequestFailure> {
    if !db_path.exists() {
        return match mode {
            SemanticMode::Required => Err(RequestFailure::semantics_not_ready(
                "Workspace symbols database does not exist",
                json!({ "coverage": "missing" }),
            )),
            SemanticMode::Auto => Ok(SemanticReadiness::Degraded {
                reason: "DATABASE_MISSING".to_string(),
                retryable: true,
            }),
            SemanticMode::Off => Ok(SemanticReadiness::Disabled),
        };
    }

    // Open read-only without acquiring write or exclusive locks
    let conn = rusqlite::Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| RequestFailure::internal(format!("Failed to open SQLite database: {e}")))?;

    // 1. Verify table presence
    let has_vectors: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='symbol_vectors'",
            [],
            |row| row.get::<_, i32>(0).map(|c| c > 0),
        )
        .unwrap_or(false);

    let has_config: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='embedding_config'",
            [],
            |row| row.get::<_, i32>(0).map(|c| c > 0),
        )
        .unwrap_or(false);

    let has_symbols: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='symbols'",
            [],
            |row| row.get::<_, i32>(0).map(|c| c > 0),
        )
        .unwrap_or(false);

    let has_generations: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='embedding_generations'",
            [],
            |row| row.get::<_, i32>(0).map(|c| c > 0),
        )
        .unwrap_or(false);

    if !has_vectors || !has_config {
        return match mode {
            SemanticMode::Required => Err(RequestFailure::semantics_not_ready(
                "Workspace vector storage tables missing",
                json!({ "coverage": "missing" }),
            )),
            SemanticMode::Auto => Ok(SemanticReadiness::Degraded {
                reason: "VECTORS_MISSING".to_string(),
                retryable: true,
            }),
            SemanticMode::Off => Ok(SemanticReadiness::Disabled),
        };
    }

    // 2. Validate model & dimensions
    let config: Result<(String, usize, u32), _> = conn.query_row(
        "SELECT model_name, dimensions, format_version FROM embedding_config WHERE id = 1",
        [],
        |row| {
            Ok((
                row.get(0)?,
                row.get::<_, i64>(1)? as usize,
                row.get::<_, i64>(2)? as u32,
            ))
        },
    );

    let (stored_model, stored_dims, stored_fmt) = match config {
        Ok(c) => c,
        Err(_) => {
            return match mode {
                SemanticMode::Required => Err(RequestFailure::semantics_not_ready(
                    "Embedding configuration row missing in SQLite",
                    json!({ "coverage": "missing" }),
                )),
                SemanticMode::Auto => Ok(SemanticReadiness::Degraded {
                    reason: "CONFIG_MISSING".to_string(),
                    retryable: true,
                }),
                SemanticMode::Off => Ok(SemanticReadiness::Disabled),
            };
        }
    };

    let dev_info = provider.device_info();
    let provider_dims = provider.dimensions();

    let encoder_identity = match provider.encoder_identity() {
        Ok(id) => id,
        Err(e) => {
            return match mode {
                SemanticMode::Required => Err(RequestFailure::semantics_not_ready(
                    format!("Encoder identity unavailable: {e}"),
                    json!({ "coverage": "missing_identity" }),
                )),
                SemanticMode::Auto => Ok(SemanticReadiness::Degraded {
                    reason: "ENCODER_IDENTITY_UNAVAILABLE".to_string(),
                    retryable: true,
                }),
                SemanticMode::Off => Ok(SemanticReadiness::Disabled),
            };
        }
    };

    let expected_key = match encoder_identity.storage_key() {
        Ok(key) => key,
        Err(e) => {
            return match mode {
                SemanticMode::Required => Err(RequestFailure::semantics_not_ready(
                    format!("Failed to compute canonical encoder storage key: {e}"),
                    json!({ "coverage": "invalid_identity" }),
                )),
                SemanticMode::Auto => Ok(SemanticReadiness::Degraded {
                    reason: "ENCODER_STORAGE_KEY_INVALID".to_string(),
                    retryable: true,
                }),
                SemanticMode::Off => Ok(SemanticReadiness::Disabled),
            };
        }
    };

    if stored_dims != provider_dims || stored_model != expected_key {
        return match mode {
            SemanticMode::Required => Err(RequestFailure::semantics_not_ready(
                format!(
                    "Stored vectors are incompatible: stored {} ({}d) vs expected {} ({}d)",
                    stored_model, stored_dims, expected_key, provider_dims
                ),
                json!({
                    "coverage": "incompatible",
                    "stored_model": stored_model,
                    "stored_dimensions": stored_dims,
                    "expected_encoder_key": expected_key,
                    "provider_model": dev_info.model_name,
                    "provider_dimensions": provider_dims,
                }),
            )),
            SemanticMode::Auto => Ok(SemanticReadiness::Degraded {
                reason: "VECTORS_INCOMPATIBLE".to_string(),
                retryable: true,
            }),
            SemanticMode::Off => Ok(SemanticReadiness::Disabled),
        };
    }

    // 3. Check vector count & staleness
    let vector_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM symbol_vectors", [], |row| row.get(0))
        .unwrap_or(0);

    let symbol_count: i64 = if has_symbols {
        conn.query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))
            .unwrap_or(0)
    } else {
        0
    };

    // Check if a ready generation exists with eligible_symbols == 0 for expected_key
    let zero_eligible_ready: bool = if has_generations {
        conn.query_row(
            "SELECT COUNT(*) FROM embedding_generations
             WHERE encoder_key = ?1 AND status = 'ready' AND eligible_symbols = 0",
            rusqlite::params![expected_key],
            |row| row.get::<_, i64>(0).map(|c| c > 0),
        )
        .unwrap_or(false)
    } else {
        false
    };

    if symbol_count > 0 && vector_count == 0 && !zero_eligible_ready {
        return match mode {
            SemanticMode::Required => Err(RequestFailure::semantics_not_ready(
                "Workspace has symbols but zero vector embeddings",
                json!({ "coverage": "missing" }),
            )),
            SemanticMode::Auto => Ok(SemanticReadiness::Degraded {
                reason: "VECTORS_MISSING".to_string(),
                retryable: true,
            }),
            SemanticMode::Off => Ok(SemanticReadiness::Disabled),
        };
    }

    if stored_fmt != CURRENT_EMBEDDING_FORMAT_VERSION && symbol_count > 0 {
        return match mode {
            SemanticMode::Required => Err(RequestFailure::semantics_not_ready(
                format!(
                    "Stored vectors are stale: format v{stored_fmt} != v{CURRENT_EMBEDDING_FORMAT_VERSION}"
                ),
                json!({
                    "coverage": "stale",
                    "format_version": stored_fmt,
                    "expected_version": CURRENT_EMBEDDING_FORMAT_VERSION,
                }),
            )),
            SemanticMode::Auto => Ok(SemanticReadiness::Degraded {
                reason: "VECTORS_STALE".to_string(),
                retryable: true,
            }),
            SemanticMode::Off => Ok(SemanticReadiness::Disabled),
        };
    }

    // 4. Check embedding_generations table (Milestone N3/N4)
    let has_canonical: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='canonical_revisions'",
            [],
            |row| row.get::<_, i32>(0).map(|c| c > 0),
        )
        .unwrap_or(false);

    let canonical_rev: i64 = if has_canonical {
        conn.query_row(
            "SELECT revision FROM canonical_revisions ORDER BY revision DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0)
    } else {
        0
    };

    let total_gen_count: i64 = if has_generations {
        conn.query_row("SELECT COUNT(*) FROM embedding_generations", [], |row| {
            row.get(0)
        })
        .unwrap_or(0)
    } else {
        0
    };

    if symbol_count > 0 && (!has_generations || total_gen_count == 0) {
        return match mode {
            SemanticMode::Required => Err(RequestFailure::semantics_not_ready(
                "Workspace has symbols but no embedding generations recorded",
                json!({ "coverage": "missing" }),
            )),
            SemanticMode::Auto => Ok(SemanticReadiness::Degraded {
                reason: "GENERATIONS_MISSING".to_string(),
                retryable: true,
            }),
            SemanticMode::Off => Ok(SemanticReadiness::Disabled),
        };
    }

    let encoder_identity_opt = Some(encoder_identity);
    let mut vector_generation = None;
    let mut eligible_symbols = symbol_count as usize;
    let mut embedded_symbols = vector_count as usize;

    if total_gen_count > 0 {
        let ready_gen: Result<(i64, usize, usize, i64), _> = conn.query_row(
            "SELECT id, eligible_symbols, embedded_symbols, source_revision
             FROM embedding_generations
             WHERE encoder_key = ?1 AND status = 'ready'
             ORDER BY id DESC LIMIT 1",
            rusqlite::params![expected_key],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get::<_, i64>(1)? as usize,
                    row.get::<_, i64>(2)? as usize,
                    row.get::<_, i64>(3)?,
                ))
            },
        );

        match ready_gen {
            Ok((gen_id, eligible, embedded, gen_source_rev)) => {
                if gen_source_rev < canonical_rev {
                    return match mode {
                        SemanticMode::Required => Err(RequestFailure::semantics_not_ready(
                            format!(
                                "Embedding generation is stale: generation rev {gen_source_rev} < canonical rev {canonical_rev}"
                            ),
                            json!({
                                "coverage": "stale",
                                "generation_revision": gen_source_rev,
                                "canonical_revision": canonical_rev,
                            }),
                        )),
                        SemanticMode::Auto => Ok(SemanticReadiness::Degraded {
                            reason: "GENERATION_STALE".to_string(),
                            retryable: true,
                        }),
                        SemanticMode::Off => Ok(SemanticReadiness::Disabled),
                    };
                }

                if embedded < eligible || (eligible > 0 && embedded == 0) {
                    return match mode {
                        SemanticMode::Required => Err(RequestFailure::semantics_not_ready(
                            format!(
                                "Embedding generation is incomplete: embedded {embedded}/{eligible} symbols"
                            ),
                            json!({
                                "coverage": "incomplete",
                                "eligible_symbols": eligible,
                                "embedded_symbols": embedded,
                            }),
                        )),
                        SemanticMode::Auto => Ok(SemanticReadiness::Degraded {
                            reason: "GENERATION_INCOMPLETE".to_string(),
                            retryable: true,
                        }),
                        SemanticMode::Off => Ok(SemanticReadiness::Disabled),
                    };
                }

                vector_generation = Some(gen_id);
                eligible_symbols = eligible;
                embedded_symbols = embedded;
            }
            Err(_) => {
                // Check if generation is currently building
                let building_count: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM embedding_generations WHERE status = 'building'",
                        [],
                        |row| row.get(0),
                    )
                    .unwrap_or(0);

                if building_count > 0 {
                    return match mode {
                        SemanticMode::Required => Err(RequestFailure::semantics_not_ready(
                            "Embedding generation is still building",
                            json!({ "coverage": "building" }),
                        )),
                        SemanticMode::Auto => Ok(SemanticReadiness::Degraded {
                            reason: "GENERATION_BUILDING".to_string(),
                            retryable: true,
                        }),
                        SemanticMode::Off => Ok(SemanticReadiness::Disabled),
                    };
                } else {
                    return match mode {
                        SemanticMode::Required => Err(RequestFailure::semantics_not_ready(
                            "Embedding generation not ready",
                            json!({ "coverage": "missing" }),
                        )),
                        SemanticMode::Auto => Ok(SemanticReadiness::Degraded {
                            reason: "GENERATION_NOT_READY".to_string(),
                            retryable: true,
                        }),
                        SemanticMode::Off => Ok(SemanticReadiness::Disabled),
                    };
                }
            }
        }
    }

    Ok(SemanticReadiness::Ready {
        model_id: dev_info.model_name,
        dimensions: provider_dims,
        device: dev_info.device,
        encoder_identity: encoder_identity_opt,
        vector_generation,
        eligible_symbols,
        embedded_symbols,
    })
}
