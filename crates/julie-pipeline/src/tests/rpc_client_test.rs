//! Tests for `RpcEmbeddingProvider` (Phase 3b, Task 3).
//!
//! IMPLEMENTED IN TASK 3. Drive the client against an in-test tokio fake host
//! that speaks the envelope protocol; assert embed_query/embed_batch return
//! correct vectors, cached dimensions/device_info reflect the health response,
//! and a simulated broken pipe triggers exactly one reconnect.
