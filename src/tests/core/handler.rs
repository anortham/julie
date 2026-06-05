// Tests for src/handler.rs — JulieServerHandler construction and lifecycle.
//
// Shared helpers live in `common.rs`. They are re-exported here so submodules
// can keep using `use super::*;` without referencing `common` directly.

mod common;

#[allow(unused_imports)]
pub(crate) use common::*;

mod deadline;
mod editing_metrics;
mod inprocess_ctor;
mod metrics_recording;
mod path_helpers;
mod public_surface;
mod startup_checkpoint;
mod workspace_binding_metrics;
