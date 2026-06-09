// VCS_ROOT_MARKERS and RegistryPaths relocated to julie-core so that
// julie-runtime (workspace-root discovery) can use them without depending
// upward on the full julie crate. Re-exported here so all existing
// `crate::paths::*` call sites in the main crate compile unchanged.
pub use julie_core::paths::{RegistryPaths, VCS_ROOT_MARKERS};
