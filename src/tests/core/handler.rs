// Tests for src/handler.rs — JulieServerHandler construction and lifecycle.
//
// Shared helpers live in `common.rs`. They are re-exported here so submodules
// can keep using `use super::*;` without referencing `common` directly.

mod common;

#[allow(unused_imports)]
pub(crate) use common::*;

mod deadline;
mod editing_metrics;
mod fa_pin_hint;
mod inprocess_ctor;
mod inprocess_serve;
mod leader_watcher;
mod loser_refuses;
mod metrics_recording;
mod path_helpers;
mod public_surface;
mod startup_checkpoint;
mod t9_bounded_read;
mod workspace_binding_metrics;
