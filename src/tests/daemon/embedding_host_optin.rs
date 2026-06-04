//! Tests for the opt-in embedding-host coexistence wiring (Phase 3b, Task 7).
//!
//! IMPLEMENTED IN TASK 7. The lead owns `src/tests/daemon/mod.rs` registration;
//! fill this file only. Verify that `spawn_embedding_init` only takes the host
//! path when `JULIE_EMBEDDING_USE_HOST` is truthy, and is byte-for-byte the
//! existing `create_embedding_provider` path when the env is unset.
