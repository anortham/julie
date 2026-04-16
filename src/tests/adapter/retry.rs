//! Tests for adapter restart handoff and retry behavior.

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    use crate::adapter::{ForwardOutcome, run_adapter_with};

    #[tokio::test]
    async fn test_run_adapter_with_retries_connect_failure_without_fixed_sleep() {
        let ensure_calls = Arc::new(AtomicUsize::new(0));
        let connect_calls = Arc::new(AtomicUsize::new(0));
        let forward_calls = Arc::new(AtomicUsize::new(0));

        let started = Instant::now();
        run_adapter_with(
            {
                let ensure_calls = Arc::clone(&ensure_calls);
                move || {
                    ensure_calls.fetch_add(1, Ordering::Relaxed);
                    Ok(())
                }
            },
            {
                let connect_calls = Arc::clone(&connect_calls);
                move || {
                    let connect_calls = Arc::clone(&connect_calls);
                    async move {
                        let call = connect_calls.fetch_add(1, Ordering::Relaxed);
                        if call == 0 {
                            anyhow::bail!("daemon restart handoff")
                        }
                        let (client, _server) = tokio::io::duplex(64);
                        Ok(client)
                    }
                }
            },
            {
                let forward_calls = Arc::clone(&forward_calls);
                move |_| {
                    let forward_calls = Arc::clone(&forward_calls);
                    async move {
                        forward_calls.fetch_add(1, Ordering::Relaxed);
                        Ok(ForwardOutcome::SessionEnded)
                    }
                }
            },
        )
        .await
        .expect("adapter loop should retry and complete");

        assert_eq!(ensure_calls.load(Ordering::Relaxed), 2);
        assert_eq!(connect_calls.load(Ordering::Relaxed), 2);
        assert_eq!(forward_calls.load(Ordering::Relaxed), 1);
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "retry path should not wait on a fixed sleep"
        );
    }

    #[tokio::test]
    async fn test_run_adapter_with_retries_immediate_disconnect_without_fixed_sleep() {
        let ensure_calls = Arc::new(AtomicUsize::new(0));
        let connect_calls = Arc::new(AtomicUsize::new(0));
        let forward_calls = Arc::new(AtomicUsize::new(0));

        let started = Instant::now();
        run_adapter_with(
            {
                let ensure_calls = Arc::clone(&ensure_calls);
                move || {
                    ensure_calls.fetch_add(1, Ordering::Relaxed);
                    Ok(())
                }
            },
            {
                let connect_calls = Arc::clone(&connect_calls);
                move || {
                    let connect_calls = Arc::clone(&connect_calls);
                    async move {
                        connect_calls.fetch_add(1, Ordering::Relaxed);
                        let (client, _server) = tokio::io::duplex(64);
                        Ok(client)
                    }
                }
            },
            {
                let forward_calls = Arc::clone(&forward_calls);
                move |_| {
                    let forward_calls = Arc::clone(&forward_calls);
                    async move {
                        let call = forward_calls.fetch_add(1, Ordering::Relaxed);
                        if call == 0 {
                            Ok(ForwardOutcome::ImmediateDaemonDisconnect)
                        } else {
                            Ok(ForwardOutcome::SessionEnded)
                        }
                    }
                }
            },
        )
        .await
        .expect("adapter loop should retry and complete");

        assert_eq!(ensure_calls.load(Ordering::Relaxed), 2);
        assert_eq!(connect_calls.load(Ordering::Relaxed), 2);
        assert_eq!(forward_calls.load(Ordering::Relaxed), 2);
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "immediate disconnect retry should not wait on a fixed sleep"
        );
    }

    #[tokio::test]
    async fn test_run_adapter_with_errors_after_exhausting_immediate_disconnect_retries() {
        let ensure_calls = Arc::new(AtomicUsize::new(0));
        let connect_calls = Arc::new(AtomicUsize::new(0));
        let forward_calls = Arc::new(AtomicUsize::new(0));

        let result = run_adapter_with(
            {
                let ensure_calls = Arc::clone(&ensure_calls);
                move || {
                    ensure_calls.fetch_add(1, Ordering::Relaxed);
                    Ok(())
                }
            },
            {
                let connect_calls = Arc::clone(&connect_calls);
                move || {
                    let connect_calls = Arc::clone(&connect_calls);
                    async move {
                        connect_calls.fetch_add(1, Ordering::Relaxed);
                        let (client, _server) = tokio::io::duplex(64);
                        Ok(client)
                    }
                }
            },
            {
                let forward_calls = Arc::clone(&forward_calls);
                move |_| {
                    let forward_calls = Arc::clone(&forward_calls);
                    async move {
                        forward_calls.fetch_add(1, Ordering::Relaxed);
                        Ok(ForwardOutcome::ImmediateDaemonDisconnect)
                    }
                }
            },
        )
        .await;

        let err = result.expect_err("retry budget exhaustion should error");
        assert!(err.to_string().contains("immediately after handshake"));
        assert_eq!(ensure_calls.load(Ordering::Relaxed), 3);
        assert_eq!(connect_calls.load(Ordering::Relaxed), 3);
        assert_eq!(forward_calls.load(Ordering::Relaxed), 3);
    }
}
