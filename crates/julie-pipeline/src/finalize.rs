//! Relationship resolution now happens in the in-memory graph at load time.
//! This module is kept so existing `use julie_pipeline::finalize` sites compile.

pub fn resolve_pending_relationships() {}
