use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceStartupHint {
    pub path: PathBuf,
    pub source: Option<WorkspaceStartupSource>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceStartupSource {
    Cli,
    Env,
    Cwd,
}

impl WorkspaceStartupSource {
    pub fn as_header_value(self) -> &'static str {
        match self {
            Self::Cli => "cli",
            Self::Env => "env",
            Self::Cwd => "cwd",
        }
    }

    pub fn from_header_value(value: &str) -> Option<Self> {
        match value {
            "cli" => Some(Self::Cli),
            "env" => Some(Self::Env),
            "cwd" => Some(Self::Cwd),
            _ => None,
        }
    }
}
