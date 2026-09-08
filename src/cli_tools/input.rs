//! CLI input parsing and parameter source resolution for Julie.
//!
//! Enforces the CLI input contract:
//! - 16 MiB size ceiling for all parameter input sources
//! - Exactly one JSON object per input (no multiple JSON documents, no trailing garbage)
//! - Valid UTF-8 encoding
//! - Non-object JSON roots rejected (arrays, numbers, strings, booleans, null)
//! - Parameter source defaulting: when no source is supplied, defaults to `{}`
//! - When an empty file or empty stdin is supplied, returns `RequestFailure::invalid_arguments`

use std::fs::File;
use std::io::Read;
use std::path::Path;

use serde_json::{Map, Value};

use crate::request_engine::RequestFailure;

/// Maximum allowed input size for parameter sources (16 MiB).
pub const MAX_INPUT_BYTES: usize = 16 * 1024 * 1024;

/// Parse raw bytes into a JSON object map according to the CLI input contract.
///
/// Validates:
/// 1. Input size ceiling (<= 16 MiB)
/// 2. Valid UTF-8 encoding
/// 3. Valid JSON syntax
/// 4. Exactly one JSON value in the input (rejects multiple JSON documents or trailing non-whitespace)
/// 5. Root value is a JSON object (rejects arrays, primitives, null)
///
/// Returns `RequestFailure::invalid_arguments` on any validation failure.
pub fn parse_request_input(
    bytes: &[u8],
    source: &str,
) -> Result<Map<String, Value>, RequestFailure> {
    if bytes.len() > MAX_INPUT_BYTES {
        return Err(RequestFailure::invalid_arguments(format!(
            "{source}: input exceeds 16 MiB"
        )));
    }

    let text = std::str::from_utf8(bytes)
        .map_err(|e| RequestFailure::invalid_arguments(format!("{source}: invalid UTF-8: {e}")))?;

    let value: Value = serde_json::from_str(text)
        .map_err(|e| RequestFailure::invalid_arguments(format!("{source}: {e}")))?;

    match value {
        Value::Object(map) => Ok(map),
        _ => Err(RequestFailure::invalid_arguments(format!(
            "{source}: expected object"
        ))),
    }
}

/// Read up to `MAX_INPUT_BYTES + 1` bytes from a reader and parse as a JSON object.
///
/// Bounded to 16 MiB + 1 byte so that unbounded inputs (e.g. /dev/zero, oversized files)
/// do not exhaust memory before size validation fails.
pub fn read_bounded_input<R: Read>(
    reader: R,
    source: &str,
) -> Result<Map<String, Value>, RequestFailure> {
    let mut bounded = reader.take((MAX_INPUT_BYTES + 1) as u64);
    let mut bytes = Vec::new();
    bounded
        .read_to_end(&mut bytes)
        .map_err(|e| RequestFailure::invalid_arguments(format!("{source}: read error: {e}")))?;
    parse_request_input(&bytes, source)
}

/// Resolve tool parameters from the mutually exclusive CLI parameter sources.
///
/// - When no parameter source is supplied (`(None, None, false)`), defaults to `{}`.
/// - When inline parameters are supplied, parses the string.
/// - When a file path is supplied, reads and parses the file (empty file fails).
/// - When `--params-stdin` is set, reads and parses stdin (empty stdin fails).
/// - If multiple parameter sources are provided, returns an invalid arguments error.
pub fn resolve_parameter_map(
    params: Option<&str>,
    params_file: Option<&Path>,
    params_stdin: bool,
) -> Result<Map<String, Value>, RequestFailure> {
    match (params, params_file, params_stdin) {
        (Some(inline), None, false) => parse_request_input(inline.as_bytes(), "--params"),
        (None, Some(path), false) => {
            let file = File::open(path).map_err(|e| {
                RequestFailure::invalid_arguments(format!("{}: {}", path.display(), e))
            })?;
            read_bounded_input(file, &path.display().to_string())
        }
        (None, None, true) => {
            let stdin = std::io::stdin();
            read_bounded_input(stdin.lock(), "stdin")
        }
        (None, None, false) => {
            // Parameter source defaulting: when no parameter source is supplied, default to {}
            Ok(Map::new())
        }
        _ => Err(RequestFailure::invalid_arguments(
            "Mutually exclusive parameter sources: specify only one of --params, --params-file, or --params-stdin",
        )),
    }
}
