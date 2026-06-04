//! External extract path helpers — relocated to `julie_core::external_extract_paths`.
//!
//! All items re-exported so existing `crate::external_extract::paths::*` import
//! sites compile unchanged.
pub use julie_core::external_extract_paths::{
    ExternalFilePath, normalize_deleted_external_file, normalize_existing_external_file,
    normalize_external_root,
};
