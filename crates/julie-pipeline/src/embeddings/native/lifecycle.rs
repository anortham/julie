//! Lifecycle management, broker launch, attachment, race coordination, and provenance verification.

use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use tracing::debug;

use julie_core::embeddings_contract::{DeviceInfo, EmbeddingRequestBudget, EncoderIdentity};

use crate::embeddings::native::client::NativeClientConn;
use crate::embeddings::native::health::query_and_validate_health;
use crate::embeddings::native::launch::{
    BrokerPaths, NativeLaunchConfig, derive_broker_paths, find_and_hash_sidecar_binary,
    spawn_broker, verify_launched_child_sha,
};
use crate::embeddings::sidecar_protocol::HealthResult;

pub fn connect_endpoint(
    paths: &BrokerPaths,
    timeout: Option<Duration>,
) -> Result<NativeClientConn> {
    #[cfg(unix)]
    {
        NativeClientConn::connect(&paths.endpoint_path, timeout).map_err(Into::into)
    }
    #[cfg(windows)]
    {
        NativeClientConn::connect(&paths.endpoint_str, timeout).map_err(Into::into)
    }
}

/// Launches or connects to a native sidecar broker, coordinating across processes.
pub fn launch_and_attach(
    config: &NativeLaunchConfig,
    budget: &EmbeddingRequestBudget,
    expected_identity: Option<&EncoderIdentity>,
) -> Result<(
    NativeClientConn,
    Option<std::process::ChildStdin>,
    EncoderIdentity,
    DeviceInfo,
    HealthResult,
    String,
)> {
    budget.check_budget()?;
    let remaining = budget.remaining_time();
    let probe_timeout = Duration::from_millis(100).min(remaining);

    let is_derived = derive_broker_paths(
        &config.cache_root,
        &config.executable_sha256,
        &config.model_id,
    )
    .map(|d| d.endpoint_str == config.broker_paths.endpoint_str)
    .unwrap_or(false);

    let (active_paths, mut current_sha) = {
        let (_, disk_sha) =
            find_and_hash_sidecar_binary(Some(&config.executable_path)).map_err(|e| {
                anyhow::anyhow!(
                    "NATIVE_SIDECAR_HASH_FAILED: failed to hash sidecar binary at '{}': {e}",
                    config.executable_path.display()
                )
            })?;
        if is_derived {
            let paths = derive_broker_paths(&config.cache_root, &disk_sha, &config.model_id)
                .unwrap_or_else(|_| config.broker_paths.clone());
            (paths, disk_sha)
        } else {
            (config.broker_paths.clone(), disk_sha)
        }
    };

    if let Ok(mut client) = connect_endpoint(&active_paths, Some(probe_timeout)) {
        if let Ok((identity, device_info, health)) =
            query_and_validate_health(&mut client, budget, Some(&config.model_id))
        {
            // Verify attached broker process provenance
            let peer_pid = client.peer_pid().map_err(|e| {
                anyhow::anyhow!(
                    "ATTACHED_BROKER_PID_UNAVAILABLE: failed to get peer pid from attached broker: {e}"
                )
            })?;
            current_sha =
                verify_launched_child_sha(&config.executable_path, peer_pid, &current_sha)?;

            if let Some(expected) = expected_identity {
                if &identity == expected {
                    return Ok((client, None, identity, device_info, health, current_sha));
                } else {
                    bail!(
                        "REPLACEMENT_BROKER_INCOMPATIBLE: existing broker identity does not match expected provider identity"
                    );
                }
            } else {
                return Ok((client, None, identity, device_info, health, current_sha));
            }
        }
    }

    debug!(
        "spawning native sidecar broker for model {}",
        config.model_id
    );
    let mut child = spawn_broker(
        &config.executable_path,
        &config.cache_root,
        &config.model_id,
        &active_paths,
    )?;

    let mut child_stdin_holder = child.stdin.take();
    let mut child_was_clean_loser = false;

    #[cfg(any(test, debug_assertions))]
    if let Ok(sync_path) = std::env::var("JULIE_TEST_SYNC_BEFORE_SPAWN_VERIFY") {
        let p = std::path::Path::new(&sync_path);
        let mut waited = 0;
        while !p.exists() && waited < 200 {
            std::thread::sleep(Duration::from_millis(10));
            waited += 1;
        }
        if !p.exists() {
            panic!("JULIE_TEST_SYNC_BEFORE_SPAWN_VERIFY timed out waiting for sync marker");
        }
        // Require confirmed child termination before proceeding; explicitly fail synchronization on timeout or error
        let mut exit_waited = 0;
        let mut confirmed_exit = false;
        while exit_waited < 200 {
            match child.try_wait() {
                Ok(Some(_)) => {
                    confirmed_exit = true;
                    break;
                }
                Ok(None) => {}
                Err(err) => {
                    panic!("JULIE_TEST_SYNC_BEFORE_SPAWN_VERIFY try_wait error: {err}");
                }
            }
            std::thread::sleep(Duration::from_millis(5));
            exit_waited += 1;
        }
        if !confirmed_exit {
            panic!("JULIE_TEST_SYNC_BEFORE_SPAWN_VERIFY timed out waiting for child termination");
        }
    }

    // Verify spawned child's provenance immediately
    match verify_launched_child_sha(&config.executable_path, child.id(), &current_sha) {
        Ok(verified_sha) => {
            current_sha = verified_sha;
        }
        Err(err) => {
            // P2: child could have exited cleanly between spawn and hashing because a sibling won the race!
            if let Ok(Some(status)) = child.try_wait() {
                if status.success() {
                    debug!(
                        "spawned broker child exited 0 (sibling won race); connecting to winner"
                    );
                    #[cfg(any(test, debug_assertions))]
                    if let Ok(marker) = std::env::var("JULIE_TEST_RECORD_ERROR_BRANCH") {
                        let _ = std::fs::write(marker, "spawn_hash_error_branch_executed");
                    }
                    child_stdin_holder = None;
                    child_was_clean_loser = true;
                } else {
                    bail!("sidecar broker child process failed with status {status:?}");
                }
            } else {
                return Err(err);
            }
        }
    }

    let start = Instant::now();
    let max_startup = Duration::from_secs(8).min(budget.remaining_time());
    let mut poll_interval = Duration::from_millis(15);

    loop {
        budget.check_budget()?;
        if !child_was_clean_loser {
            if let Ok(Some(status)) = child.try_wait() {
                if status.success() {
                    // Sibling broker won the acquisition race! Clean exit code 0.
                    debug!(
                        "spawned broker child exited 0 (sibling won race); connecting to winner"
                    );
                    child_stdin_holder = None;
                    child_was_clean_loser = true;
                } else {
                    bail!("sidecar broker child process failed with status {status:?}");
                }
            }
        }

        if let Ok(mut client) = connect_endpoint(
            &active_paths,
            Some(Duration::from_millis(250).min(budget.remaining_time())),
        ) {
            if let Ok((identity, device_info, health)) =
                query_and_validate_health(&mut client, budget, Some(&config.model_id))
            {
                if let Some(expected) = expected_identity {
                    if &identity != expected {
                        bail!(
                            "REPLACEMENT_BROKER_INCOMPATIBLE: replacement broker identity does not match existing provider identity"
                        );
                    }
                }

                // Verify socket peer provenance on every successful attachment
                let peer_pid = client.peer_pid().map_err(|e| {
                    anyhow::anyhow!(
                        "ATTACHED_BROKER_PID_UNAVAILABLE: failed to get peer pid from broker: {e}"
                    )
                })?;
                current_sha =
                    verify_launched_child_sha(&config.executable_path, peer_pid, &current_sha)?;

                return Ok((
                    client,
                    child_stdin_holder,
                    identity,
                    device_info,
                    health,
                    current_sha,
                ));
            }
        }

        if start.elapsed() >= max_startup {
            bail!(
                "timed out waiting for native sidecar broker to become ready at '{}'",
                active_paths.endpoint_str
            );
        }

        let rem = budget.remaining_time();
        if rem.is_zero() {
            bail!("embedding request deadline exceeded while waiting for native broker");
        }
        std::thread::sleep(poll_interval.min(rem));
        poll_interval = (poll_interval * 2).min(Duration::from_millis(200));
    }
}
