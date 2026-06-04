//! Acceptance test (Phase 3b, Task 8, HARD GATE): one resident embedding-host
//! serves three concurrent sessions backed by exactly one sidecar.
//!
//! IMPLEMENTED IN TASK 8. The lead owns `src/tests/daemon/mod.rs` registration;
//! fill this file only. Prove: 3 concurrent `connect_or_spawn_host` sessions
//! against one $JULIE_HOME share a single host (singleton lock) and a single
//! sidecar (counted cross-process via a stub-sidecar counter file), and all
//! three embed successfully.
