//! Thin RPC-client `EmbeddingProvider` for the resident embedding-host (Phase 3b).
//!
//! IMPLEMENTED IN TASK 3. The lead owns module wiring in `embeddings/mod.rs`;
//! fill this file only. Make `RpcEmbeddingProvider` (and any helper) `pub`.
//!
//! Contract (from the plan's Fixed Contract):
//! - Implement `julie_core::embeddings_contract::EmbeddingProvider` over
//!   `super::host_transport::HostClientConn`.
//! - Hold `Mutex<Option<HostClientConn>>` (lazy connect + reconnect-once on
//!   broken pipe), a per-connection `request_id` counter, and cached
//!   `dimensions`/`DeviceInfo` from a `health` round-trip at first connect.
//! - Marshal via `super::sidecar_protocol::{RequestEnvelope, ResponseEnvelope,
//!   EmbedQueryRequest, EmbedQueryResult, EmbedBatchRequest, EmbedBatchResult,
//!   HealthResult, SIDECAR_PROTOCOL_SCHEMA, SIDECAR_PROTOCOL_VERSION}` and reuse
//!   `validate_query_response` / `validate_batch_response`.
//! - `shutdown()` drops the connection; `wait_for_exit()` returns `true`.
