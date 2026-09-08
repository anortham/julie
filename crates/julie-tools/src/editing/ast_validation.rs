//! AST post-edit syntax validation and diagnostic span translation.

use crate::editing::syntax::{SyntaxAdapter, SyntaxAdapterError};
use julie_extractors::{ParseDiagnostic, ParseDiagnosticKind};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Instant;

/// Represents a replaced byte span in the original file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextEditSpan {
    pub old_start: usize,
    pub old_end: usize,
    pub new_len: usize,
}

impl TextEditSpan {
    pub fn delta(&self) -> isize {
        self.new_len as isize - (self.old_end - self.old_start) as isize
    }
}

/// An unaffected pre-existing diagnostic translated into proposed byte coordinates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranslatedDiagnostic {
    pub kind: ParseDiagnosticKind,
    pub start_byte: u32,
    pub end_byte: u32,
}

#[derive(Debug)]
pub enum SyntaxValidationError {
    SyntaxRegression {
        path: PathBuf,
        kind: ParseDiagnosticKind,
        start_byte: u32,
        end_byte: u32,
        message: Option<String>,
    },
    Adapter(SyntaxAdapterError),
}

impl std::fmt::Display for SyntaxValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SyntaxRegression {
                path,
                kind,
                start_byte,
                end_byte,
                message,
            } => write!(
                f,
                "Syntax regression in {}: {:?} at bytes {}..{}: {}",
                path.display(),
                kind,
                start_byte,
                end_byte,
                message.as_deref().unwrap_or("syntax error")
            ),
            Self::Adapter(e) => write!(f, "Syntax adapter error: {e}"),
        }
    }
}

impl std::error::Error for SyntaxValidationError {}

impl From<SyntaxAdapterError> for SyntaxValidationError {
    fn from(e: SyntaxAdapterError) -> Self {
        Self::Adapter(e)
    }
}

/// Translates old diagnostics unaffected by edits into proposed coordinate space.
/// Diagnostics overlapping any edit span are intentionally excluded (not exempted).
pub fn translate_unaffected_diagnostics(
    old_diagnostics: &[ParseDiagnostic],
    edits: &[TextEditSpan],
) -> Vec<TranslatedDiagnostic> {
    let mut translated = Vec::new();

    'diag: for d in old_diagnostics {
        let d_start = d.start_byte as usize;
        let d_end = d.end_byte as usize;

        let mut start_shift: isize = 0;
        let mut end_shift: isize = 0;

        for edit in edits {
            if edit.old_end <= d_start {
                // Edit strictly precedes diagnostic: shifts both start and end
                start_shift += edit.delta();
                end_shift += edit.delta();
            } else if edit.old_start >= d_end {
                // Edit strictly follows diagnostic: shifts neither
            } else if edit.old_start >= d_start && edit.old_end <= d_end {
                // Edit is completely enclosed within diagnostic: shifts only the end
                end_shift += edit.delta();
            } else {
                // Diagnostic partially overlaps or is inside the edited code: not preserved
                continue 'diag;
            }
        }

        let new_start = (d_start as isize + start_shift) as u32;
        let new_end = (d_end as isize + end_shift) as u32;

        translated.push(TranslatedDiagnostic {
            kind: d.kind,
            start_byte: new_start,
            end_byte: new_end,
        });
    }

    translated
}

/// Validates that proposed source code does not introduce new syntax errors.
pub fn validate_no_syntax_regression(
    file_path: &Path,
    proposed_source: &str,
    old_diagnostics: &[ParseDiagnostic],
    edits: &[TextEditSpan],
    adapter: &SyntaxAdapter,
    deadline: Option<Instant>,
    cancelled: Option<&AtomicBool>,
) -> Result<(), SyntaxValidationError> {
    let parsed = match adapter.parse_source(file_path, proposed_source, deadline, cancelled) {
        Ok(p) => p,
        Err(SyntaxAdapterError::UnsupportedLanguage { .. }) => return Ok(()),
        Err(e) => return Err(SyntaxValidationError::Adapter(e)),
    };

    let mut baseline = translate_unaffected_diagnostics(old_diagnostics, edits);

    for new_diag in &parsed.diagnostics {
        if matches!(
            new_diag.kind,
            ParseDiagnosticKind::Error | ParseDiagnosticKind::Missing
        ) {
            // Find matching translated diagnostic
            if let Some(pos) = baseline.iter().position(|b| {
                b.kind == new_diag.kind
                    && b.start_byte == new_diag.start_byte
                    && b.end_byte == new_diag.end_byte
            }) {
                baseline.swap_remove(pos);
            } else {
                return Err(SyntaxValidationError::SyntaxRegression {
                    path: file_path.to_path_buf(),
                    kind: new_diag.kind,
                    start_byte: new_diag.start_byte,
                    end_byte: new_diag.end_byte,
                    message: new_diag.message.clone(),
                });
            }
        }
    }

    Ok(())
}
