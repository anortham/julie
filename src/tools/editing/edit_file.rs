//! edit_file tool: DMP-powered fuzzy find-and-replace.
//!
//! Lets agents edit files without reading them first. The agent provides
//! old_text (what to find) and new_text (what to replace with). DMP's
//! fuzzy matching tolerates minor differences.

use anyhow::{Result, anyhow};
use diff_match_patch_rs::{Compat, DiffMatchPatch, Ops};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tracing::debug;

use crate::handler::JulieServerHandler;
use crate::mcp_compat::CallToolResultExt;
use crate::utils::file_utils::secure_path_resolution;
use rmcp::model::{CallToolResult, Content};

use super::EditingTransaction;
use super::validation::{
    check_bracket_balance, format_dry_run_diff, format_unified_diff, should_check_balance,
};

/// A match location: character indices [start, end) in the file content.
/// For exact matches, end - start == old_text.chars().count().
/// For trimmed-line matches, end - start is the actual file content length (may differ).
#[derive(Debug, Clone, Copy)]
struct MatchSpan {
    start: usize,
    end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditMatchMode {
    Exact,
    TrimmedLines,
    Fuzzy,
}

impl EditMatchMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::TrimmedLines => "trimmed_lines",
            Self::Fuzzy => "fuzzy",
        }
    }
}

#[derive(Debug)]
struct EditApplication {
    modified_content: String,
    match_mode: EditMatchMode,
}

#[derive(Debug)]
pub(crate) struct EditFileFailure {
    pub kind: &'static str,
    pub message: String,
}

impl std::fmt::Display for EditFileFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for EditFileFailure {}

fn edit_file_error(kind: &'static str, message: impl Into<String>) -> anyhow::Error {
    anyhow!(EditFileFailure {
        kind,
        message: message.into(),
    })
}

pub(crate) fn failure_kind(error: &anyhow::Error) -> &'static str {
    error
        .downcast_ref::<EditFileFailure>()
        .map(|error| error.kind)
        .unwrap_or("execution_error")
}

fn default_dry_run() -> bool {
    true
}

fn default_occurrence() -> EditOccurrence {
    EditOccurrence::First
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EditOccurrence {
    First,
    Last,
    All,
}

impl EditOccurrence {
    pub fn as_str(self) -> &'static str {
        match self {
            EditOccurrence::First => "first",
            EditOccurrence::Last => "last",
            EditOccurrence::All => "all",
        }
    }
}

#[cfg(test)]
type BeforeCommitHook = Box<dyn Fn(&std::path::Path) + Send + Sync + 'static>;

#[cfg(test)]
static BEFORE_COMMIT_HOOK: std::sync::Mutex<Option<BeforeCommitHook>> = std::sync::Mutex::new(None);

#[cfg(test)]
pub(crate) fn set_before_commit_hook_for_test(
    hook: impl Fn(&std::path::Path) + Send + Sync + 'static,
) {
    *BEFORE_COMMIT_HOOK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Box::new(hook));
}

#[cfg(test)]
pub(crate) fn clear_before_commit_hook_for_test() {
    *BEFORE_COMMIT_HOOK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
}

#[cfg(test)]
fn run_before_commit_hook_for_test(path: &std::path::Path) {
    let hook = BEFORE_COMMIT_HOOK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take();
    if let Some(hook) = hook {
        hook(path);
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditFileTool {
    /// File path relative to workspace root
    pub file_path: String,

    /// Text to find in the file (fuzzy-matched via diff-match-patch)
    pub old_text: String,

    /// Replacement text
    pub new_text: String,

    /// Preview diff without applying (default: true). Always preview first.
    #[serde(
        default = "default_dry_run",
        deserialize_with = "crate::utils::serde_lenient::deserialize_bool_lenient"
    )]
    pub dry_run: bool,

    /// Which occurrence to replace: "first" (default), "last", or "all"
    #[serde(default = "default_occurrence")]
    pub occurrence: EditOccurrence,
}

/// Pure function: apply an edit to content string. Returns modified content.
/// Separated from tool struct for testability.
pub fn apply_edit(
    content: &str,
    old_text: &str,
    new_text: &str,
    occurrence: &str,
) -> Result<String> {
    Ok(apply_edit_with_metrics(content, old_text, new_text, occurrence)?.modified_content)
}

fn apply_edit_with_metrics(
    content: &str,
    old_text: &str,
    new_text: &str,
    occurrence: &str,
) -> Result<EditApplication> {
    if old_text.is_empty() {
        return Err(edit_file_error("validation", "old_text cannot be empty"));
    }

    let (spans, match_mode) = find_all_matches_with_mode(content, old_text)?;

    if spans.is_empty() {
        return Err(edit_file_error(
            "match_not_found",
            format!(
                "No match found for the provided old_text ({} chars). \
             Verify the text exists in the file.",
                old_text.len()
            ),
        ));
    }

    let selected: Vec<MatchSpan> = match occurrence {
        "first" => vec![spans[0]],
        "last" => vec![*spans.last().unwrap()],
        "all" => spans,
        _ => {
            return Err(edit_file_error(
                "validation",
                format!(
                    "Invalid occurrence '{}': must be 'first', 'last', or 'all'",
                    occurrence
                ),
            ));
        }
    };

    // Apply replacements in reverse order so character positions don't shift
    let new_chars: Vec<char> = new_text.chars().collect();
    let mut result_chars: Vec<char> = content.chars().collect();

    for span in selected.iter().rev() {
        result_chars.splice(span.start..span.end, new_chars.iter().copied());
    }

    Ok(EditApplication {
        modified_content: result_chars.into_iter().collect(),
        match_mode,
    })
}

/// Find all match spans for old_text in content.
///
/// Strategy (first match wins):
/// 1. Exact substring search (any pattern length)
/// 2. Trimmed-line matching (whitespace/indentation tolerance, any length)
/// 3. DMP bitap fuzzy matching (character-level tolerance, ≤32 chars only)
///
/// Returns MatchSpans sorted by start position ascending.
fn find_all_matches_with_mode(
    content: &str,
    old_text: &str,
) -> Result<(Vec<MatchSpan>, EditMatchMode)> {
    let old_char_len = old_text.chars().count();

    // Phase 1: exact substring (any length, always tried first)
    let exact_positions = find_exact_matches(content, old_text);
    if !exact_positions.is_empty() {
        return Ok((
            exact_positions
                .into_iter()
                .map(|pos| MatchSpan {
                    start: pos,
                    end: pos + old_char_len,
                })
                .collect(),
            EditMatchMode::Exact,
        ));
    }

    // Phase 2: trimmed-line matching (handles indentation and trailing whitespace)
    let trimmed = find_matches_by_trimmed_lines(content, old_text);
    if !trimmed.is_empty() {
        return Ok((trimmed, EditMatchMode::TrimmedLines));
    }

    // Phase 3: DMP bitap fuzzy (≤32 chars, handles minor typos)
    const DMP_BITAP_LIMIT: usize = 32;
    if old_char_len > DMP_BITAP_LIMIT {
        return Ok((Vec::new(), EditMatchMode::Fuzzy));
    }

    let dmp = DiffMatchPatch::new();
    let mut spans = Vec::new();
    let mut search_from: usize = 0;
    let content_chars: Vec<char> = content.chars().collect();

    loop {
        match dmp.match_main::<Compat>(content, old_text, search_from) {
            Some(pos) if pos >= search_from => {
                let end = compute_fuzzy_end(&dmp, &content_chars, pos, old_text, old_char_len);
                spans.push(MatchSpan { start: pos, end });
                // Guard: if compute_fuzzy_end returns pos (possible when the content
                // window is empty near the tail of the file), we must still advance
                // search_from to prevent an infinite loop.
                search_from = end.max(pos + 1);
            }
            _ => break,
        }
    }

    Ok((spans, EditMatchMode::Fuzzy))
}

/// After DMP bitap finds a fuzzy match at `pos`, compute the actual end position
/// by diffing old_text against a content window and walking the diff operations.
/// Falls back to `pos + old_char_len` if the diff fails for any reason.
fn compute_fuzzy_end(
    dmp: &DiffMatchPatch,
    content_chars: &[char],
    pos: usize,
    old_text: &str,
    old_char_len: usize,
) -> usize {
    let fallback = pos + old_char_len;
    let window_end = (pos + old_char_len * 2).min(content_chars.len());
    let window: String = content_chars[pos..window_end].iter().collect();

    let diffs = match dmp.diff_main::<Compat>(old_text, &window) {
        Ok(d) => d,
        Err(_) => return fallback,
    };

    let mut old_pos = 0usize;
    let mut content_pos = 0usize;

    for diff in &diffs {
        let len = diff.data().len();
        match diff.op() {
            Ops::Equal => {
                old_pos += len;
                content_pos += len;
            }
            Ops::Delete => {
                old_pos += len;
            }
            Ops::Insert => {
                content_pos += len;
            }
        }
        if old_pos >= old_char_len {
            break;
        }
    }

    pos + content_pos
}

/// Exact substring search returning all character-index positions ascending.
fn find_exact_matches(content: &str, pattern: &str) -> Vec<usize> {
    let mut positions = Vec::new();
    let mut byte_start = 0;

    while let Some(byte_pos) = content[byte_start..].find(pattern) {
        let abs_byte_pos = byte_start + byte_pos;
        // Convert byte position to char position
        let char_pos = content[..abs_byte_pos].chars().count();
        positions.push(char_pos);
        // Advance past this match (at least one char to avoid infinite loops on empty patterns)
        byte_start = abs_byte_pos + pattern.len().max(1);
    }

    positions
}

/// Trimmed-line matching: finds blocks where each line matches after trim().
/// Handles indentation differences (tabs vs spaces, 2 vs 4 spaces) and trailing whitespace.
/// Returns spans covering the actual file content with original whitespace.
fn find_matches_by_trimmed_lines(content: &str, old_text: &str) -> Vec<MatchSpan> {
    let content_lines: Vec<&str> = content.lines().collect();
    let old_lines: Vec<&str> = old_text.lines().collect();

    if old_lines.is_empty() {
        return vec![];
    }

    let old_trimmed: Vec<&str> = old_lines.iter().map(|l| l.trim()).collect();
    let n = old_lines.len();

    // Reject if ALL trimmed lines are empty (matching on only blank lines is too ambiguous)
    if old_trimmed.iter().all(|l| l.is_empty()) {
        return vec![];
    }

    if n > content_lines.len() {
        return vec![];
    }

    // Collect chars for CRLF-aware end position calculation
    let content_chars: Vec<char> = content.chars().collect();
    let total_chars = content_chars.len();

    // Precompute char offset of each line start
    let mut line_starts = vec![0usize];
    for (i, &c) in content_chars.iter().enumerate() {
        if c == '\n' {
            line_starts.push(i + 1);
        }
    }

    let mut matches = vec![];

    for i in 0..=content_lines.len() - n {
        let all_match = (0..n).all(|j| content_lines[i + j].trim() == old_trimmed[j]);
        if all_match {
            let start = line_starts[i];
            // End boundary: include trailing line ending only if old_text ends with \n
            let end = if old_text.ends_with('\n') || old_text.ends_with("\r\n") {
                if i + n < line_starts.len() {
                    line_starts[i + n]
                } else {
                    total_chars
                }
            } else {
                // Exclude trailing line ending (\n or \r\n) of the last matched line
                let raw = if i + n < line_starts.len() {
                    line_starts[i + n]
                } else {
                    total_chars
                };
                let mut e = raw;
                if e > start && e > 0 && content_chars.get(e - 1) == Some(&'\n') {
                    e -= 1;
                }
                if e > start && e > 0 && content_chars.get(e - 1) == Some(&'\r') {
                    e -= 1;
                }
                e
            };

            matches.push(MatchSpan { start, end });
        }
    }

    // Filter overlapping spans: keep non-overlapping matches in order
    let mut filtered = Vec::with_capacity(matches.len());
    let mut last_end = 0usize;
    for span in &matches {
        if span.start >= last_end {
            filtered.push(*span);
            last_end = span.end;
        }
    }

    filtered
}

impl EditFileTool {
    pub(crate) fn request_input_bytes(&self) -> u64 {
        serde_json::to_vec(self)
            .map(|bytes| bytes.len() as u64)
            .unwrap_or(0)
    }

    pub(crate) fn base_metrics_metadata(&self) -> Value {
        let input_bytes = self.request_input_bytes();
        json!({
            "kind": "edit_file",
            "dry_run": self.dry_run,
            "applied": false,
            "input_bytes": input_bytes,
            "old_text_bytes": self.old_text.len(),
            "new_text_bytes": self.new_text.len(),
            "occurrence": self.occurrence.as_str(),
        })
    }

    pub(crate) fn success_metrics_metadata(&self, handler: &JulieServerHandler) -> Result<Value> {
        let workspace_root = handler.require_primary_workspace_root()?;
        let resolved_path = secure_path_resolution(&self.file_path, &workspace_root)?;
        let original_content = std::fs::read_to_string(&resolved_path)
            .map_err(|error| anyhow!("Cannot read file '{}': {}", self.file_path, error))?;
        let application = apply_edit_with_metrics(
            &original_content,
            &self.old_text,
            &self.new_text,
            self.occurrence.as_str(),
        )?;
        let diff = format_unified_diff(
            &original_content,
            &application.modified_content,
            &self.file_path,
        );
        let changed_bytes = changed_region_bytes(&original_content, &application.modified_content);
        let mut metadata = self.base_metrics_metadata();
        if let Some(object) = metadata.as_object_mut() {
            object.insert("file_size_bytes".to_string(), json!(original_content.len()));
            object.insert("diff_bytes".to_string(), json!(diff.len()));
            object.insert("changed_bytes".to_string(), json!(changed_bytes));
            object.insert(
                "match_mode".to_string(),
                json!(application.match_mode.as_str()),
            );
            object.insert(
                "applied".to_string(),
                json!(!self.dry_run && application.modified_content != original_content),
            );
        }
        Ok(metadata)
    }

    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        if self.old_text.is_empty() {
            return Err(edit_file_error(
                "validation",
                "old_text is required and cannot be empty",
            ));
        }

        // Resolve and validate file path (security check)
        let workspace_root = handler.require_primary_workspace_root()?;
        let resolved_path = secure_path_resolution(&self.file_path, &workspace_root)?;
        let resolved_str = resolved_path.to_string_lossy().to_string();

        // Read file content internally (not costing agent context tokens)
        let original_content = std::fs::read_to_string(&resolved_path)
            .map_err(|e| anyhow!("Cannot read file '{}': {}", self.file_path, e))?;

        // Apply the edit
        let modified_content = match apply_edit(
            &original_content,
            &self.old_text,
            &self.new_text,
            self.occurrence.as_str(),
        ) {
            Ok(content) => content,
            Err(e) => {
                return Err(anyhow!("{e}"));
            }
        };

        // Balance check: advisory warning only (cannot distinguish code from strings/comments)
        let balance_warning = if should_check_balance(&self.file_path) {
            check_bracket_balance(&original_content, &modified_content)
        } else {
            None
        };

        // Generate diff preview
        let diff = format_unified_diff(&original_content, &modified_content, &self.file_path);

        if self.dry_run {
            debug!("edit_file dry_run for {}", self.file_path);
            let preview_diff = format_dry_run_diff(&diff);
            let mut msg = format!(
                "Dry run preview (set dry_run=false to apply):\n\n{}",
                preview_diff
            );
            if let Some(ref warning) = balance_warning {
                msg.push_str(&format!("\n\n{}", warning));
            }
            return Ok(CallToolResult::text_content(vec![Content::text(msg)]));
        }

        // Commit the edit atomically.
        // NOTE: do NOT call update_file_hash after writing. The watcher must see the
        // hash mismatch to trigger symbol re-extraction. Updating the hash here would
        // poison watcher change-detection and leave the index permanently stale.
        #[cfg(test)]
        run_before_commit_hook_for_test(&resolved_path);
        let txn = EditingTransaction::begin(&resolved_str)?;
        txn.commit_if_unchanged(&modified_content, &original_content)?;

        debug!("edit_file applied to {}", self.file_path);
        let mut msg = format!("Applied edit to {}:\n\n{}", self.file_path, diff);
        if let Some(warning) = balance_warning {
            msg.push_str(&format!("\n\n{}", warning));
        }
        Ok(CallToolResult::text_content(vec![Content::text(msg)]))
    }
}

pub(crate) fn changed_region_bytes(original: &str, modified: &str) -> usize {
    if original == modified {
        return 0;
    }
    let original_bytes = original.as_bytes();
    let modified_bytes = modified.as_bytes();
    let prefix = original_bytes
        .iter()
        .zip(modified_bytes.iter())
        .take_while(|(left, right)| left == right)
        .count();
    let mut original_suffix = original_bytes.len();
    let mut modified_suffix = modified_bytes.len();
    while original_suffix > prefix
        && modified_suffix > prefix
        && original_bytes[original_suffix - 1] == modified_bytes[modified_suffix - 1]
    {
        original_suffix -= 1;
        modified_suffix -= 1;
    }
    (original_suffix - prefix).max(modified_suffix - prefix)
}
