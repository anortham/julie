use crate::tools::workspace::commands::ManageWorkspaceTool;
use crate::tools::workspace::indexing::file_policy;
use std::path::Path;

impl ManageWorkspaceTool {
    /// Detect programming language from file path.
    ///
    /// Delegates to the shared indexing policy so batch and watcher paths classify files
    /// the same way.
    pub(crate) fn detect_language(&self, file_path: &Path) -> String {
        file_policy::detect_language_for_indexing(file_path)
    }
}
