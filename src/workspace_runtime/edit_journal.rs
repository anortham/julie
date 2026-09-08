//! Durable Atomic Edit Journal.
//!
//! Tracks multi-file source edit transactions on disk under
//! `<canonical-root>/.julie/edit-journals/<edit_id>.json`.
//! Survives process crashes and allows deterministic, idempotent recovery.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Error, ErrorKind};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Recovery action requested for an incomplete or rollback edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryAction {
    Resume,
    Rollback,
}

impl RecoveryAction {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "resume" => Ok(Self::Resume),
            "rollback" => Ok(Self::Rollback),
            other => Err(format!(
                "Invalid recovery action '{other}': expected 'resume' or 'rollback'"
            )),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Resume => "resume",
            Self::Rollback => "rollback",
        }
    }
}

/// Outcome of a source edit application or recovery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditDisposition {
    pub edit_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery_action: Option<RecoveryAction>,
    pub applied_paths: Vec<PathBuf>,
    pub pending_paths: Vec<PathBuf>,
    pub conflicted_paths: Vec<PathBuf>,
    pub index_refresh_pending: bool,
}

/// Lifecycle state of an individual file within an edit journal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JournalFileState {
    Pending,
    Applied,
    RolledBack,
    Conflicted,
}

/// Recorded metadata and payloads for an individual file edit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalFileEntry {
    pub path: PathBuf,
    pub before_hash: String,
    pub after_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_bytes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_bytes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_bytes_hex: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_bytes_hex: Option<String>,
    pub state: JournalFileState,
}

impl JournalFileEntry {
    pub fn get_before_bytes(&self) -> Option<Vec<u8>> {
        if let Some(ref hex_str) = self.before_bytes_hex {
            if let Ok(bytes) = hex::decode(hex_str) {
                return Some(bytes);
            }
        }
        self.before_bytes.as_ref().map(|s| s.as_bytes().to_vec())
    }

    pub fn get_after_bytes(&self) -> Option<Vec<u8>> {
        if let Some(ref hex_str) = self.after_bytes_hex {
            if let Ok(bytes) = hex::decode(hex_str) {
                return Some(bytes);
            }
        }
        self.after_bytes.as_ref().map(|s| s.as_bytes().to_vec())
    }
}

/// Overall lifecycle state of an edit journal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JournalState {
    Pending,
    Applying,
    Applied,
    RollingBack,
    RolledBack,
    Conflicted,
}

/// Complete on-disk journal representation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditJournal {
    pub edit_id: String,
    #[serde(default)]
    pub created_at_unix_ms: u64,
    #[serde(default)]
    pub updated_at_unix_ms: u64,
    pub state: JournalState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_recovery_action: Option<RecoveryAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_receipt: Option<EditDisposition>,
    pub files: Vec<JournalFileEntry>,
}

impl EditJournal {
    pub fn new_empty(edit_id: String, files: Vec<JournalFileEntry>) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        Self {
            edit_id,
            created_at_unix_ms: now,
            updated_at_unix_ms: now,
            state: JournalState::Pending,
            active_recovery_action: None,
            terminal_receipt: None,
            files,
        }
    }

    pub fn load(journal_path: &Path) -> io::Result<Self> {
        let content = fs::read_to_string(journal_path)?;
        serde_json::from_str(&content)
            .map_err(|e| Error::new(ErrorKind::InvalidData, format!("Malformed journal: {e}")))
    }

    pub fn save(&self, journal_path: &Path) -> io::Result<()> {
        let parent = journal_path.parent().ok_or_else(|| {
            Error::new(ErrorKind::NotFound, "Journal path has no parent directory")
        })?;
        fs::create_dir_all(parent)?;

        let temp_filename = format!(
            ".tmp_{}_{}_{}.json",
            self.edit_id,
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        let temp_path = parent.join(temp_filename);

        let json = serde_json::to_string_pretty(self).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Failed to serialize journal: {e}"),
            )
        })?;

        fs::write(&temp_path, json.as_bytes())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&temp_path, fs::Permissions::from_mode(0o600));
        }
        fs::rename(&temp_path, journal_path)?;
        Ok(())
    }

    pub fn touch(&mut self) {
        self.updated_at_unix_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
    }

    pub fn record_file_state(
        &mut self,
        rel_path: &Path,
        state: JournalFileState,
        journal_path: &Path,
    ) -> io::Result<()> {
        for f in &mut self.files {
            if f.path == rel_path {
                f.state = state;
            }
        }
        self.touch();
        self.save(journal_path)
    }

    pub fn record_terminal_receipt(
        &mut self,
        disposition: EditDisposition,
        journal_path: &Path,
    ) -> io::Result<()> {
        match disposition.recovery_action {
            Some(RecoveryAction::Rollback) => self.state = JournalState::RolledBack,
            _ => self.state = JournalState::Applied,
        }
        self.terminal_receipt = Some(disposition);
        self.touch();
        self.save(journal_path)
    }

    pub fn set_active_recovery_action(
        &mut self,
        action: RecoveryAction,
        journal_path: &Path,
    ) -> io::Result<()> {
        self.active_recovery_action = Some(action);
        match action {
            RecoveryAction::Resume => self.state = JournalState::Applying,
            RecoveryAction::Rollback => self.state = JournalState::RollingBack,
        }
        self.touch();
        self.save(journal_path)
    }
}
