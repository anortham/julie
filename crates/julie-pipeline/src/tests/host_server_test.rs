//! Tests for the embedding-host server (Phase 3b, Task 4).
//!
//! IMPLEMENTED IN TASK 4. Run `run_embedding_host` with an injected fake
//! provider; assert health/embed_query/embed_batch round-trip, two concurrent
//! client connections both succeed, and cancellation stops the accept loop and
//! releases the singleton lock.
