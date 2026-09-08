//! Replay engine executing streams of requests serially from JSONL.
//!
//! Enforces:
//! - 16 MiB input size ceiling
//! - Strict schema_version: 1 and unknown field rejection on envelopes
//! - Auto-assignment of 1-based line number for missing request_id
//! - Rejection of duplicate request_id within a replay session
//! - Immediate halt on framing errors (to avoid ambiguous mutation state)
//! - Continuation across ordinary request/tool failures
//! - Reporting the first non-zero exit code encountered in input order

use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::cli_tools::output::{
    format_failure_envelope, format_success_envelope, write_stdout_safe,
};
use crate::cli_tools::subcommands::ReplayArgs;
use crate::request_engine::types::RequestContext;
use crate::request_engine::{
    RequestEngine, RequestFailure, RequestOrigin, SemanticMode, ToolRequest,
};

pub const MAX_REPLAY_INPUT_BYTES: usize = 16 * 1024 * 1024;

/// Single replay record line from JSONL.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplayRecord {
    pub schema_version: u32,
    #[serde(default)]
    pub request_id: Option<String>,
    pub request: ReplayRequest,
}

/// Request payload inside a replay record.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplayRequest {
    pub name: String,
    pub arguments: Value,
    #[serde(default)]
    pub workspace: Option<PathBuf>,
    #[serde(default)]
    pub semantics: Option<SemanticMode>,
}

/// Run replay from args against a shared RequestEngine.
pub async fn run_replay(
    engine: Arc<RequestEngine>,
    args: &ReplayArgs,
    params_stdin: bool,
) -> Result<i32, RequestFailure> {
    if args.input == Path::new("-") && params_stdin {
        return Err(RequestFailure::invalid_arguments(
            "Cannot read replay input from stdin when --params-stdin is set",
        ));
    }

    let input_bytes = if args.input == Path::new("-") {
        let mut reader = std::io::stdin()
            .lock()
            .take((MAX_REPLAY_INPUT_BYTES + 1) as u64);
        let mut buf = Vec::new();
        reader
            .read_to_end(&mut buf)
            .map_err(|e| RequestFailure::invalid_arguments(format!("stdin read error: {e}")))?;
        buf
    } else {
        let file = File::open(&args.input).map_err(|e| {
            RequestFailure::invalid_arguments(format!("{}: {e}", args.input.display()))
        })?;
        let mut reader = file.take((MAX_REPLAY_INPUT_BYTES + 1) as u64);
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).map_err(|e| {
            RequestFailure::invalid_arguments(format!("{}: {e}", args.input.display()))
        })?;
        buf
    };

    if input_bytes.len() > MAX_REPLAY_INPUT_BYTES {
        return Err(RequestFailure::invalid_arguments(
            "Replay input exceeds 16 MiB",
        ));
    }

    let cursor = std::io::Cursor::new(input_bytes);
    let reader = BufReader::new(cursor);

    let mut seen_ids = HashSet::new();
    let mut first_nonzero_exit_code: Option<i32> = None;
    let mut line_number = 0;

    for line_res in reader.lines() {
        line_number += 1;
        let line = line_res.map_err(|e| {
            RequestFailure::invalid_arguments(format!("Line {line_number}: read error: {e}"))
        })?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Parse framing
        let record: ReplayRecord = match serde_json::from_str(trimmed) {
            Ok(rec) => rec,
            Err(e) => {
                let failure = RequestFailure::invalid_arguments(format!(
                    "Line {line_number}: malformed framing: {e}"
                ));
                write_stdout_safe(&format_failure_envelope(
                    Some(line_number.to_string()),
                    &failure,
                ));
                if first_nonzero_exit_code.is_none() {
                    first_nonzero_exit_code = Some(failure.exit_code());
                }
                // Halt on malformed framing
                break;
            }
        };

        if record.schema_version != 1 {
            let failure = RequestFailure::invalid_arguments(format!(
                "Line {line_number}: unsupported schema_version {}",
                record.schema_version
            ));
            write_stdout_safe(&format_failure_envelope(
                Some(line_number.to_string()),
                &failure,
            ));
            if first_nonzero_exit_code.is_none() {
                first_nonzero_exit_code = Some(failure.exit_code());
            }
            break;
        }

        let req_id = record.request_id.unwrap_or_else(|| line_number.to_string());

        if !seen_ids.insert(req_id.clone()) {
            let failure = RequestFailure::invalid_arguments(format!(
                "Line {line_number}: duplicate request_id: {req_id}"
            ));
            write_stdout_safe(&format_failure_envelope(Some(req_id), &failure));
            if first_nonzero_exit_code.is_none() {
                first_nonzero_exit_code = Some(failure.exit_code());
            }
            break;
        }

        let arguments = match record.request.arguments {
            Value::Object(map) => map,
            _ => {
                let failure = RequestFailure::invalid_arguments(format!(
                    "Line {line_number}: arguments must be a JSON object"
                ));
                write_stdout_safe(&format_failure_envelope(Some(req_id), &failure));
                if first_nonzero_exit_code.is_none() {
                    first_nonzero_exit_code = Some(failure.exit_code());
                }
                break;
            }
        };

        let tool_request = ToolRequest {
            name: record.request.name,
            arguments,
            workspace: record.request.workspace,
            semantics: record.request.semantics.unwrap_or(SemanticMode::Auto),
        };

        let timeout = Duration::from_millis(args.timeout_ms);
        let context =
            RequestContext::new(RequestOrigin::Cli, Some(timeout), CancellationToken::new());

        match engine.execute(tool_request, context).await {
            Ok(reply) => {
                let exit_code = if reply.is_error() { 3 } else { 0 };
                if exit_code != 0 && first_nonzero_exit_code.is_none() {
                    first_nonzero_exit_code = Some(exit_code);
                }
                write_stdout_safe(&format_success_envelope(Some(req_id), &reply));
            }
            Err(failure) => {
                let exit_code = failure.exit_code();
                if exit_code != 0 && first_nonzero_exit_code.is_none() {
                    first_nonzero_exit_code = Some(exit_code);
                }
                write_stdout_safe(&format_failure_envelope(Some(req_id), &failure));
            }
        }
    }

    Ok(first_nonzero_exit_code.unwrap_or(0))
}

/// Helper for testing replay record deserialization and validation.
pub fn parse_replay_line(
    line: &str,
    line_number: usize,
) -> Result<(String, ToolRequest), RequestFailure> {
    let record: ReplayRecord = serde_json::from_str(line).map_err(|e| {
        RequestFailure::invalid_arguments(format!("Line {line_number}: malformed framing: {e}"))
    })?;

    if record.schema_version != 1 {
        return Err(RequestFailure::invalid_arguments(format!(
            "Line {line_number}: unsupported schema_version {}",
            record.schema_version
        )));
    }

    let req_id = record.request_id.unwrap_or_else(|| line_number.to_string());
    let arguments = match record.request.arguments {
        Value::Object(map) => map,
        _ => {
            return Err(RequestFailure::invalid_arguments(format!(
                "Line {line_number}: arguments must be a JSON object"
            )));
        }
    };

    Ok((
        req_id,
        ToolRequest {
            name: record.request.name,
            arguments,
            workspace: record.request.workspace,
            semantics: record.request.semantics.unwrap_or(SemanticMode::Auto),
        },
    ))
}
