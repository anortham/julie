use anyhow::Result;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct FileInfo {
    pub path: String,
    pub language: String,
    pub hash: String,
    pub size: i64,
    pub last_modified: i64,
    pub last_indexed: i64,
    pub symbol_count: i32,
    pub line_count: i32,
    pub content: Option<String>,
}

pub fn create_file_info(path: &Path, language: &str, workspace_root: &Path) -> Result<FileInfo> {
    let relative = julie_core::paths::to_relative_unix_style(path, workspace_root)?;
    let metadata = std::fs::metadata(path)?;
    let content = std::fs::read_to_string(path).ok();
    let hash = content
        .as_ref()
        .map(|text| blake3::hash(text.as_bytes()).to_hex().to_string())
        .unwrap_or_default();
    let last_modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    let line_count = content
        .as_ref()
        .map(|text| text.lines().count() as i32)
        .unwrap_or(0);
    Ok(FileInfo {
        path: relative,
        language: language.to_string(),
        hash,
        size: metadata.len() as i64,
        last_modified,
        last_indexed: 0,
        symbol_count: 0,
        line_count,
        content,
    })
}
