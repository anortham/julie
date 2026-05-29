//! File processing helpers for indexing stages.
//!
//! The shared implementations live in `indexing_core::extraction` so external
//! process-facing extraction can use them without a workspace tool instance.

#[cfg(test)]
use crate::extractors::ExtractionResults;
#[cfg(test)]
use crate::indexing_core::extraction::{ExtractedFileDisposition, ExtractedFileRecord};
#[cfg(test)]
use crate::tools::workspace::commands::ManageWorkspaceTool;
#[cfg(test)]
use crate::tools::workspace::indexing::state::{IndexedFileDisposition, IndexingBatchState};
#[cfg(test)]
use anyhow::Result;
#[cfg(test)]
use std::collections::HashMap;
#[cfg(test)]
use std::path::{Path, PathBuf};

#[cfg(test)]
type ParserFileProcessResult = (
    Vec<crate::extractors::Symbol>,
    Vec<crate::extractors::Relationship>,
    Vec<crate::extractors::PendingRelationship>,
    Vec<julie_extractors::base::StructuredPendingRelationship>,
    Vec<crate::extractors::Identifier>,
    std::collections::HashMap<String, crate::extractors::base::TypeInfo>,
    Vec<crate::extractors::base::TypeArgumentUsage>,
    Vec<crate::extractors::Literal>,
    Vec<crate::extractors::base::ParseDiagnostic>,
    crate::database::FileInfo,
);

#[cfg(test)]
impl ManageWorkspaceTool {
    pub(crate) async fn queue_failed_parser_file_for_cleanup(
        &self,
        file_path: &Path,
        language: &str,
        workspace_root: &Path,
        files_to_clean: &mut Vec<String>,
        all_file_infos: &mut Vec<crate::database::FileInfo>,
    ) {
        crate::indexing_core::extraction::queue_failed_parser_file_for_cleanup(
            file_path,
            language,
            workspace_root,
            files_to_clean,
            all_file_infos,
        )
        .await;
    }

    pub(crate) async fn process_file_with_parser(
        &self,
        file_path: &Path,
        language: &str,
        workspace_root: &Path,
    ) -> Result<ParserFileProcessResult> {
        crate::indexing_core::extraction::process_file_with_parser(
            file_path,
            language,
            workspace_root,
        )
        .await
    }

    pub(crate) async fn process_file_with_parser_for_test<F>(
        &self,
        file_path: &Path,
        language: &str,
        workspace_root: &Path,
        extract: F,
    ) -> Result<(
        Vec<crate::extractors::Symbol>,
        Vec<crate::extractors::Relationship>,
        Vec<crate::extractors::PendingRelationship>,
        Vec<julie_extractors::base::StructuredPendingRelationship>,
        Vec<crate::extractors::Identifier>,
        std::collections::HashMap<String, crate::extractors::base::TypeInfo>,
        Vec<crate::extractors::base::TypeArgumentUsage>,
        Vec<crate::extractors::Literal>,
        Vec<crate::extractors::base::ParseDiagnostic>,
        crate::database::FileInfo,
    )>
    where
        F: FnOnce(String, String, PathBuf) -> Result<ExtractionResults> + Send + 'static,
    {
        crate::indexing_core::extraction::process_file_with_parser_for_test(
            file_path,
            language,
            workspace_root,
            extract,
        )
        .await
    }

    pub(crate) async fn extract_index_batch(
        &self,
        files_by_language: HashMap<String, Vec<PathBuf>>,
        workspace_root: &Path,
        state: &mut IndexingBatchState,
    ) -> Result<crate::indexing_core::batch::ExtractedBatch> {
        let (batch, records) =
            crate::indexing_core::extraction::extract_files_for_indexing_with_records(
                files_by_language,
                workspace_root,
            )
            .await?;
        record_extracted_file_records(state, records);
        Ok(batch)
    }
}

#[cfg(test)]
fn record_extracted_file_records(
    state: &mut IndexingBatchState,
    records: Vec<ExtractedFileRecord>,
) {
    for record in records {
        match record.disposition {
            ExtractedFileDisposition::Parsed => {
                state.record_file(
                    record.relative_path,
                    record.language,
                    IndexedFileDisposition::Parsed,
                    None,
                );
            }
            ExtractedFileDisposition::TextOnly => {
                state.record_file(
                    record.relative_path,
                    record.language,
                    IndexedFileDisposition::TextOnly,
                    None,
                );
            }
            ExtractedFileDisposition::RepairNeeded { detail } => {
                state.record_file(
                    record.relative_path,
                    record.language,
                    IndexedFileDisposition::RepairNeeded,
                    Some(detail),
                );
            }
        }
    }
}
