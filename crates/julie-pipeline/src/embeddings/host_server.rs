//! Resident embedding-host server (Phase 3b).
//!
//! IMPLEMENTED IN TASK 4. The lead owns module wiring in `embeddings/mod.rs`;
//! fill this file only. Make `run_embedding_host` / `run_embedding_host_default`
//! (and any config struct) `pub`.
//!
//! Contract (from the plan's Fixed Contract):
//! - `pub async fn run_embedding_host(addr: &super::host_transport::HostAddress,
//!   lock_path: &Path, cancel: CancellationToken, provider: Arc<dyn EmbeddingProvider>)`
//!   plus `run_embedding_host_default(...)` which resolves the provider via
//!   `super::init::create_embedding_provider`.
//! - Acquire an `fs2` singleton lock on `lock_path` (yield/exit if held).
//! - Bind via `super::host_transport::HostListener::bind`; accept loop spawns a
//!   task per connection. Each connection: read a line → parse
//!   `RequestEnvelope<serde_json::Value>` → dispatch `health` / `embed_query` /
//!   `embed_batch` / `shutdown` to the provider via `tokio::task::spawn_blocking`
//!   (the provider is sync + `Mutex`-guarded) → write `ResponseEnvelope`.
//!   Application errors → `ResponseEnvelope.error`; never panic the accept loop.
//! - Graceful shutdown on `cancel.cancelled()` (model on
//!   `src/daemon/http_transport.rs`), then `provider.shutdown()` +
//!   `spawn_blocking(provider.wait_for_exit(Duration::from_secs(3)))`.
