// Julie Runtime — file-watcher and workspace lifecycle layer.
//
// Sits above julie-pipeline in the crate graph; the main julie crate re-exports
// both modules via `pub use julie_runtime::{watcher, workspace}`.

pub mod watcher;
pub mod workspace;

#[cfg(test)]
mod tests;
