//! Navigation tools - Symbol reference finding
//!
//! This module provides high-performance tools for finding references:
//! - **fast_refs**: Find all references to a symbol (<20ms)
//!
//! Architecture:
//! - Uses multi-strategy symbol resolution (Tantivy → naming variants)
//! - Per-workspace database isolation
//! - Cross-language support through naming convention variants

mod fast_refs;
mod formatting;
mod reference_workspace;
pub mod resolution; // Public for use by other tools

// Re-export public APIs
pub use fast_refs::FastRefsTool;

use std::sync::{Arc, Mutex};

/// Lock the database mutex, recovering from poisoning if necessary.
/// Centralizes the lock+recover pattern used throughout navigation tools.
fn lock_db<'a>(
    db: &'a Arc<Mutex<crate::database::SymbolDatabase>>,
    context: &str,
) -> std::sync::MutexGuard<'a, crate::database::SymbolDatabase> {
    match db.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            tracing::warn!(
                "Database mutex poisoned in {}, recovering: {}",
                context,
                poisoned
            );
            poisoned.into_inner()
        }
    }
}
