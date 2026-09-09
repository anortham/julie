use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Complete cryptographic identity of an embedding encoder.
///
/// Encapsulates all factors affecting embedding vector compatibility and meaning.
/// Used to derive persistent storage keys and gate vector generation reads.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct EncoderIdentity {
    /// Identity schema version (currently 1).
    pub schema: u32,
    /// Canonical model identifier (e.g. "bge-small-en-v1.5-f32", "qwen3-0.6b-f16").
    pub model_id: String,
    /// 64-character lowercase hex SHA-256 hash of the resolved model weights.
    pub weights_sha256: String,
    /// Dimensionality of output vectors produced by this configuration.
    pub dimensions: usize,
    /// Pooling strategy applied to token representations (e.g. "cls", "mean", "last").
    pub pooling: String,
    /// Vector normalization strategy applied to outputs (e.g. "l2", "none").
    pub normalization: String,
    /// Instruction/prompt policy version applied to input texts (e.g. "v1").
    pub instruction_policy: String,
    /// Text preprocessing and formatting format version (currently 1).
    pub text_format: u32,
    /// Runtime build identifier (e.g. "llama.cpp-b3560", "fixture-runtime").
    pub runtime_build: String,
}

impl EncoderIdentity {
    /// Validates that all fields satisfy semantic constraints.
    pub fn validate(&self) -> Result<()> {
        if self.schema == 0 {
            bail!("encoder identity schema must be positive, got 0");
        }
        if self.model_id.trim().is_empty() {
            bail!("encoder identity model_id must not be empty");
        }
        if self.weights_sha256.len() != 64
            || !self.weights_sha256.chars().all(|c| c.is_ascii_hexdigit())
        {
            bail!(
                "encoder identity weights_sha256 must be a 64-character hex string, got '{}'",
                self.weights_sha256
            );
        }
        if self.dimensions == 0 {
            bail!("encoder identity dimensions must be positive, got 0");
        }
        if self.pooling.trim().is_empty() {
            bail!("encoder identity pooling must not be empty");
        }
        let norm = self.normalization.to_ascii_lowercase();
        if !matches!(norm.as_str(), "l2" | "none" | "l1" | "cosine") {
            bail!(
                "unsupported encoder normalization '{}'; expected one of: l2, none, l1, cosine",
                self.normalization
            );
        }
        if self.instruction_policy.trim().is_empty() {
            bail!("encoder identity instruction_policy must not be empty");
        }
        if self.text_format == 0 {
            bail!("encoder identity text_format must be positive, got 0");
        }
        if self.runtime_build.trim().is_empty() {
            bail!("encoder identity runtime_build must not be empty");
        }
        Ok(())
    }

    /// Derives the canonical cryptographic storage key for vector generation binding.
    ///
    /// The key is a 64-character lowercase hex SHA-256 digest of the canonical
    /// field-order serialized identity. Any change to encoder parameters produces
    /// a distinct storage key.
    pub fn storage_key(&self) -> Result<String> {
        self.validate()?;

        #[derive(Serialize)]
        struct CanonicalPayload<'a> {
            schema: u32,
            model_id: &'a str,
            weights_sha256: String,
            dimensions: usize,
            pooling: &'a str,
            normalization: String,
            instruction_policy: &'a str,
            text_format: u32,
            runtime_build: &'a str,
        }

        let payload = CanonicalPayload {
            schema: self.schema,
            model_id: &self.model_id,
            weights_sha256: self.weights_sha256.to_ascii_lowercase(),
            dimensions: self.dimensions,
            pooling: &self.pooling,
            normalization: self.normalization.to_ascii_lowercase(),
            instruction_policy: &self.instruction_policy,
            text_format: self.text_format,
            runtime_build: &self.runtime_build,
        };

        let canonical_bytes = serde_json::to_vec(&payload)?;
        let mut hasher = Sha256::new();
        hasher.update(&canonical_bytes);
        let digest = hasher.finalize();
        Ok(format!("{:064x}", digest))
    }

    /// Convenience constructor for test doubles and mocks.
    pub fn mock(model_id: &str, dimensions: usize) -> Self {
        Self {
            schema: 1,
            model_id: model_id.to_string(),
            weights_sha256: "0".repeat(64),
            dimensions: if dimensions == 0 { 1 } else { dimensions },
            pooling: "cls".to_string(),
            normalization: "l2".to_string(),
            instruction_policy: "v1".to_string(),
            text_format: 1,
            runtime_build: "mock".to_string(),
        }
    }
}
