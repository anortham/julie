//! Native stdio child embedding provider using `julie-semantic-sidecar`.

pub mod child;
pub mod decoders;
pub mod health;
pub mod launch;

use std::sync::Mutex;
use std::time::Duration;

use anyhow::{Result, bail};

use julie_core::embeddings_contract::{
    DeviceInfo, EmbeddingProvider, EmbeddingRequestBudget, EncoderIdentity,
};

use crate::embeddings::factory::EmbeddingConfig;
pub use child::{MAX_PAYLOAD_BYTES, SidecarChild};
pub use health::{query_and_validate_health, validate_native_health};
pub use launch::{
    DEFAULT_NATIVE_MODEL, NativeLaunchConfig, find_and_hash_sidecar_binary, run_prepare,
};

#[derive(Clone)]
struct NativeRuntimeFacts {
    device_info: DeviceInfo,
    accelerated: Option<bool>,
    degraded_reason: Option<String>,
}

/// High-performance native embedding provider communicating with `julie-semantic-sidecar` child over stdio.
pub struct NativeEmbeddingProvider {
    config: NativeLaunchConfig,
    child: Mutex<Option<SidecarChild>>,
    identity: EncoderIdentity,
    runtime_facts: Mutex<NativeRuntimeFacts>,
}

impl NativeEmbeddingProvider {
    /// Attempts to spawn a native sidecar child and validates its initial health.
    pub fn try_new(embedding_config: &EmbeddingConfig) -> Result<Self> {
        let config = NativeLaunchConfig::try_new(
            embedding_config.native_program.as_deref(),
            embedding_config.native_model.as_deref(),
            embedding_config.cache_dir.as_deref(),
        )?;
        let budget = EmbeddingRequestBudget::with_timeout(Duration::from_secs(10));
        let (child, identity, facts) = Self::spawn_and_probe(&config, &budget, None)?;
        Ok(Self {
            config,
            child: Mutex::new(Some(child)),
            identity,
            runtime_facts: Mutex::new(facts),
        })
    }

    fn spawn_and_probe(
        config: &NativeLaunchConfig,
        budget: &EmbeddingRequestBudget,
        expected: Option<&EncoderIdentity>,
    ) -> Result<(SidecarChild, EncoderIdentity, NativeRuntimeFacts)> {
        let mut child = SidecarChild::spawn(&config.executable_path, &config.model_id)?;
        let (health, identity, device_info) =
            query_and_validate_health(&mut child, budget, Some(&config.model_id))?;
        if let Some(expected) = expected {
            if identity != *expected {
                bail!("sidecar restarted with a different encoder identity");
            }
        }
        Ok((
            child,
            identity,
            NativeRuntimeFacts {
                device_info,
                accelerated: health.accelerated,
                degraded_reason: health.degraded_reason,
            },
        ))
    }

    fn with_child<R>(
        &self,
        budget: &EmbeddingRequestBudget,
        call: impl FnOnce(&mut SidecarChild) -> Result<R>,
    ) -> Result<R> {
        let mut guard = self
            .child
            .lock()
            .map_err(|_| anyhow::anyhow!("sidecar mutex poisoned"))?;
        if guard.as_mut().is_none_or(|child| !child.is_alive()) {
            let (child, _, facts) =
                Self::spawn_and_probe(&self.config, budget, Some(&self.identity))?;
            *guard = Some(child);
            if let Ok(mut f) = self.runtime_facts.lock() {
                *f = facts;
            }
        }
        let child = guard.as_mut().expect("spawned above");
        match call(child) {
            Ok(value) => Ok(value),
            Err(err) => {
                *guard = None;
                Err(err)
            }
        }
    }

    /// Returns the OS process ID of the active sidecar child, if running.
    pub fn child_pid(&self) -> Option<u32> {
        let mut guard = self.child.lock().ok()?;
        let child = guard.as_mut()?;
        if child.is_alive() {
            Some(child.pid())
        } else {
            None
        }
    }

    /// Kills or drops the active sidecar child to test recovery (test only).
    pub fn kill_child_for_test(&self) {
        if let Ok(mut guard) = self.child.lock() {
            *guard = None;
        }
    }
}

impl EmbeddingProvider for NativeEmbeddingProvider {
    fn embed_query(&self, text: &str, budget: &EmbeddingRequestBudget) -> Result<Vec<f32>> {
        self.with_child(budget, |c| c.embed_query(text, budget))
    }

    fn embed_batch(
        &self,
        texts: &[String],
        budget: &EmbeddingRequestBudget,
    ) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        self.with_child(budget, |c| c.embed_batch(texts, budget))
    }

    fn encoder_identity(&self) -> Result<EncoderIdentity> {
        Ok(self.identity.clone())
    }

    fn dimensions(&self) -> usize {
        self.identity.dimensions
    }

    fn device_info(&self) -> DeviceInfo {
        self.runtime_facts
            .lock()
            .map(|f| f.device_info.clone())
            .unwrap_or_else(|_| DeviceInfo {
                runtime: "native".to_string(),
                device: "unknown".to_string(),
                model_name: self.identity.model_id.clone(),
                dimensions: self.identity.dimensions,
            })
    }

    fn accelerated(&self) -> Option<bool> {
        self.runtime_facts.lock().ok().and_then(|f| f.accelerated)
    }

    fn degraded_reason(&self) -> Option<String> {
        self.runtime_facts
            .lock()
            .ok()
            .and_then(|f| f.degraded_reason.clone())
    }

    fn running_executable_sha(&self) -> Option<String> {
        Some(self.config.executable_sha256.clone())
    }

    fn health_check(&self, budget: &EmbeddingRequestBudget) -> Result<()> {
        self.with_child(budget, |c| {
            let _ = c.health(budget)?;
            Ok(())
        })
    }

    fn shutdown(&self) {
        let child = self.child.lock().ok().and_then(|mut g| g.take());
        if let Some(c) = child {
            c.shutdown();
        }
    }

    fn child_pid(&self) -> Option<u32> {
        self.child_pid()
    }
}

