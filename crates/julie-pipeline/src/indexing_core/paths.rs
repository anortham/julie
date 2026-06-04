use std::path::Path;

pub(crate) fn relative_path_for_storage(file_path: &Path, workspace_root: &Path) -> String {
    if file_path.is_absolute() {
        julie_core::paths::to_relative_unix_style(file_path, workspace_root)
            .unwrap_or_else(|_| file_path.to_string_lossy().replace('\\', "/"))
    } else {
        file_path.to_string_lossy().replace('\\', "/")
    }
}
