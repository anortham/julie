use std::path::{Path, PathBuf};
use tracing::warn;
use tree_sitter::Node;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NormalizedSpan {
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
    pub start_byte: u32,
    pub end_byte: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RecordOffset {
    pub line_delta: u32,
    pub byte_delta: u32,
}

impl NormalizedSpan {
    pub fn from_node(node: &Node) -> Self {
        let start_pos = node.start_position();
        let end_pos = node.end_position();

        Self {
            start_line: start_pos.row as u32 + 1,
            start_column: start_pos.column as u32,
            end_line: end_pos.row as u32 + 1,
            end_column: end_pos.column as u32,
            start_byte: node.start_byte() as u32,
            end_byte: node.end_byte() as u32,
        }
    }

    pub fn with_offset(self, offset: RecordOffset) -> Self {
        Self {
            start_line: self.start_line + offset.line_delta,
            start_column: self.start_column,
            end_line: self.end_line + offset.line_delta,
            end_column: self.end_column,
            start_byte: self.start_byte + offset.byte_delta,
            end_byte: self.end_byte + offset.byte_delta,
        }
    }
}

pub fn normalize_file_path(file_path: &str, workspace_root: &Path) -> String {
    let path_to_canonicalize = if Path::new(file_path).is_absolute() {
        PathBuf::from(file_path)
    } else {
        workspace_root.join(file_path)
    };

    let canonical_path = path_to_canonicalize.canonicalize().unwrap_or_else(|e| {
        warn!(
            "⚠️  Failed to canonicalize path '{}': {} - using joined path",
            path_to_canonicalize.display(),
            e
        );
        path_to_canonicalize.clone()
    });

    if canonical_path.is_absolute() {
        crate::utils::paths::to_relative_unix_style(&canonical_path, workspace_root).unwrap_or_else(
            |e| {
                warn!(
                    "⚠️  Failed to convert to relative path '{}': {} - using absolute as fallback",
                    canonical_path.display(),
                    e
                );
                canonical_path.to_string_lossy().replace('\\', "/")
            },
        )
    } else {
        canonical_path.to_string_lossy().replace('\\', "/")
    }
}
