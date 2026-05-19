pub mod cli;
mod data_loss_guard;
pub mod info;
pub mod lock;
pub mod metadata;
pub mod operations;
pub mod paths;
pub mod report;

pub use cli::{ExternalExtractArgs, ExternalExtractCommand, ExternalExtractRawArgs};
pub use info::{
    ExternalExtractCounts, ExternalExtractInfo, ExternalInfoSchemaState, read_external_extract_info,
};
pub use lock::{
    DEFAULT_EXTERNAL_EXTRACT_OPERATION_LOCK_TIMEOUT, ExternalExtractOperationLock,
    ExternalExtractOperationLockError, external_extract_operation_lock_path,
};
pub use metadata::{
    EXTRACT_CONTRACT_VERSION, ExternalExtractDatabaseOperation, ExternalExtractMetadata,
    ensure_external_extract_metadata, ensure_external_extract_metadata_with_root_policy,
    load_external_extract_metadata, mark_external_extract_analysis_current,
    mark_external_extract_analysis_stale, open_external_extract_database,
    open_external_extract_database_for_operation,
};
pub use operations::run_external_extract;
pub use paths::{
    ExternalFilePath, normalize_deleted_external_file, normalize_existing_external_file,
    normalize_external_root,
};
pub use report::{
    ExternalExtractError, ExternalExtractReport, ExternalExtractStatus,
    failed_external_extract_report, format_external_extract_report,
};
