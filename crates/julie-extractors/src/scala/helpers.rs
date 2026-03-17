//! Helper functions for Scala symbol extraction
//!
//! Provides utility functions for extracting modifiers, visibility,
//! type parameters, and other metadata from Scala code.

use crate::base::Visibility;

/// Determine visibility from modifiers
pub(super) fn determine_visibility(modifiers: &[String]) -> Visibility {
    if modifiers.contains(&"private".to_string()) {
        Visibility::Private
    } else if modifiers.contains(&"protected".to_string()) {
        Visibility::Protected
    } else {
        Visibility::Public // Scala defaults to public
    }
}
