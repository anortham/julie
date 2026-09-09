mod process;

#[cfg(test)]
#[cfg(feature = "embeddings-sidecar")]
mod tests;

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};

use super::sidecar_protocol::{
    EmbedBatchRequest, EmbedBatchResult, EmbedQueryRequest, EmbedQueryResult, HealthResult,
    validate_batch_response, validate_query_response,
};
use super::sidecar_supervisor::{SidecarLaunchConfig, build_sidecar_launch_config};
use super::{DeviceInfo, EmbeddingProvider, EmbeddingRequestBudget, EncoderIdentity};
use process::{SidecarProcess, read_response_timeout, spawn_process};

pub struct SidecarEmbeddingProvider {
    process: Mutex<SidecarProcess>,
    health: Mutex<HealthResult>,
    launch_config: SidecarLaunchConfig,
    response_timeout: Duration,
    device: String,
    sidecar_runtime: String,
    model_id: String,
    expected_dims: usize,
    accelerated: bool,
    degraded_reason: Option<String>,
    /// Count of consecutive fatal failures across all respawn attempts.
    /// Resets to 0 on the first successful request. Once it reaches
    /// FATAL_THRESHOLD the provider is permanently disabled and stops
    /// attempting to respawn.
    consecutive_fatal_failures: AtomicU32,
}

impl SidecarEmbeddingProvider {
    pub fn try_new() -> Result<Self> {
        let launch = build_sidecar_launch_config()?;
        Self::spawn_from_launch_config(launch, read_response_timeout())
    }

    pub fn try_new_for_command(program: String, args: Vec<String>) -> Result<Self> {
        Self::try_new_for_command_with_timeout(program, args, read_response_timeout())
    }

    pub fn try_new_for_command_with_timeout(
        program: String,
        args: Vec<String>,
        response_timeout: Duration,
    ) -> Result<Self> {
        let launch = SidecarLaunchConfig {
            program: PathBuf::from(program),
            args,
            env: Vec::new(),
        };
        Self::spawn_from_launch_config(launch, response_timeout)
    }

    fn spawn_from_launch_config(
        launch_config: SidecarLaunchConfig,
        response_timeout: Duration,
    ) -> Result<Self> {
        let (process, health) = spawn_process(&launch_config, response_timeout)?;

        let expected_dims = health.dims.unwrap_or(384);

        Ok(Self {
            process: Mutex::new(process),
            health: Mutex::new(health.clone()),
            launch_config,
            response_timeout,
            device: health.device.unwrap_or_else(|| "unknown".to_string()),
            sidecar_runtime: health
                .runtime
                .unwrap_or_else(|| "python-sidecar".to_string()),
            model_id: health
                .model_id
                .unwrap_or_else(|| "BAAI/bge-small-en-v1.5".to_string()),
            expected_dims,
            accelerated: health.accelerated.unwrap_or(false),
            degraded_reason: health.degraded_reason,
            consecutive_fatal_failures: AtomicU32::new(0),
        })
    }

    fn reset_process_if_fatal(&self, process: &mut SidecarProcess) -> Result<()> {
        if !process.take_connection_fatal() {
            return Ok(());
        }

        const FATAL_THRESHOLD: u32 = 3;
        let failures = self
            .consecutive_fatal_failures
            .fetch_add(1, Ordering::Relaxed)
            + 1;
        if failures >= FATAL_THRESHOLD {
            bail!(
                "embedding sidecar permanently disabled after {} consecutive fatal failures",
                failures
            );
        }

        process.terminate();
        let (replacement, new_health) =
            spawn_process(&self.launch_config, self.response_timeout)
                .context("failed to respawn sidecar process after connection-fatal error")?;
        *process = replacement;
        if let Ok(mut h) = self.health.lock() {
            *h = new_health;
        }
        Ok(())
    }
}

fn is_valid_sha256_digest(digest: &str) -> bool {
    digest.len() == 64 && digest.chars().all(|c| c.is_ascii_hexdigit())
}

impl EmbeddingProvider for SidecarEmbeddingProvider {
    fn embed_query(&self, text: &str, budget: &EmbeddingRequestBudget) -> Result<Vec<f32>> {
        budget.check_budget()?;
        let remaining = budget.remaining_time();
        if remaining.is_zero() {
            bail!("embedding request budget exceeded before process lock acquisition");
        }

        let mut process = self
            .process
            .lock()
            .map_err(|_| anyhow!("sidecar process lock poisoned"))?;

        budget.check_budget()?;
        let remaining = budget.remaining_time();
        let timeout = self.response_timeout.min(remaining);
        if timeout.is_zero() {
            bail!("embedding request budget exceeded");
        }

        let result: EmbedQueryResult = match process.send_request_with_timeout(
            "embed_query",
            EmbedQueryRequest {
                text: text.to_string(),
                remaining_budget_ms: Some(timeout.as_millis() as u64),
            },
            timeout,
        ) {
            Ok(result) => result,
            Err(err) => {
                self.reset_process_if_fatal(&mut process)?;
                return Err(err);
            }
        };
        validate_query_response(&result, self.expected_dims)?;
        // Successful request: reset the consecutive failure counter.
        self.consecutive_fatal_failures.store(0, Ordering::Relaxed);
        Ok(result.vector)
    }

    fn embed_batch(
        &self,
        texts: &[String],
        budget: &EmbeddingRequestBudget,
    ) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        budget.check_budget()?;
        let remaining = budget.remaining_time();
        if remaining.is_zero() {
            bail!("embedding request budget exceeded before process lock acquisition");
        }

        let mut process = self
            .process
            .lock()
            .map_err(|_| anyhow!("sidecar process lock poisoned"))?;

        budget.check_budget()?;
        let remaining = budget.remaining_time();
        let timeout = self.response_timeout.min(remaining);
        if timeout.is_zero() {
            bail!("embedding request budget exceeded");
        }

        let result: EmbedBatchResult = match process.send_request_with_timeout(
            "embed_batch",
            EmbedBatchRequest {
                texts: texts.to_vec(),
                remaining_budget_ms: Some(timeout.as_millis() as u64),
            },
            timeout,
        ) {
            Ok(result) => result,
            Err(err) => {
                self.reset_process_if_fatal(&mut process)?;
                return Err(err);
            }
        };
        validate_batch_response(&result, texts.len(), self.expected_dims)?;
        // Successful request: reset the consecutive failure counter.
        self.consecutive_fatal_failures.store(0, Ordering::Relaxed);
        Ok(result.vectors)
    }

    fn encoder_identity(&self) -> Result<EncoderIdentity> {
        let health = self
            .health
            .lock()
            .map_err(|_| anyhow!("sidecar health lock poisoned"))?
            .clone();

        let weights_sha256 = match health.model_sha256.as_deref() {
            Some(sha) if is_valid_sha256_digest(sha) => sha.to_ascii_lowercase(),
            _ => bail!("IdentityUnavailable"),
        };

        let dims = health.dims.unwrap_or(self.expected_dims);
        if dims == 0 {
            bail!("IdentityUnavailable: invalid dimensions");
        }

        let model_id = health
            .model_id
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| self.model_id.clone());

        let pooling = match health.pooling.as_deref() {
            Some(p) if !p.trim().is_empty() => p.to_string(),
            _ => bail!("IdentityUnavailable: missing pooling"),
        };

        let normalization = match health.normalization.as_deref() {
            Some(n) if !n.trim().is_empty() => n.to_string(),
            _ => bail!("IdentityUnavailable: missing normalization"),
        };

        let instruction_policy = match health.instruction_policy_version {
            Some(v) => format!("v{v}"),
            None => bail!("IdentityUnavailable: missing instruction_policy_version"),
        };

        let runtime_build = health
            .llama_cpp_build
            .filter(|s| !s.trim().is_empty())
            .or_else(|| health.runtime.filter(|s| !s.trim().is_empty()))
            .unwrap_or_else(|| self.sidecar_runtime.clone());

        let identity = EncoderIdentity {
            schema: 1,
            model_id,
            weights_sha256,
            dimensions: dims,
            pooling,
            normalization,
            instruction_policy,
            text_format: 1,
            runtime_build,
        };

        identity.validate()?;
        Ok(identity)
    }

    fn dimensions(&self) -> usize {
        self.expected_dims
    }

    fn device_info(&self) -> DeviceInfo {
        DeviceInfo {
            runtime: format!("python-sidecar ({})", self.sidecar_runtime),
            device: self.device.clone(),
            model_name: self.model_id.clone(),
            dimensions: self.expected_dims,
        }
    }

    fn accelerated(&self) -> Option<bool> {
        Some(self.accelerated)
    }

    fn degraded_reason(&self) -> Option<String> {
        self.degraded_reason.clone()
    }

    fn shutdown(&self) {
        match self.process.lock() {
            Ok(mut process) => process.shutdown_and_terminate(),
            Err(poisoned) => {
                let mut process = poisoned.into_inner();
                process.terminate();
            }
        }
    }

    fn wait_for_exit(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let poll_interval = Duration::from_millis(10);

        loop {
            let exited = match self.process.lock() {
                Ok(mut process) => process.child.try_wait().ok().flatten().is_some(),
                Err(poisoned) => {
                    drop(poisoned.into_inner());
                    true
                }
            };

            if exited {
                return true;
            }

            if Instant::now() >= deadline {
                return false;
            }

            thread::sleep(poll_interval);
        }
    }
}

impl Drop for SidecarEmbeddingProvider {
    fn drop(&mut self) {
        match self.process.lock() {
            Ok(mut process) => process.terminate(),
            Err(poisoned) => poisoned.into_inner().terminate(),
        }
    }
}
