# Ad Hoc Large File Inspector Tool

## Status

Starter spec. Parked until the setup/onboarding fixes are complete.

## Problem

Julie is good at indexed source-code intelligence, but the best part of context-mode for day-to-day work is different: it can inspect logs and large text files without dumping raw content into the model context.

Current Julie text indexing is not the right fit for this job:

- Logs and build outputs can be large, volatile, and append-heavy.
- Automatically indexing them would add watcher churn and index bloat.
- The useful interaction is usually narrow: summarize shape, find matching lines, inspect bounded context around a failure.

## Goals

- Let agents inspect large text files with bounded output.
- Keep raw file bytes out of model context unless explicitly sliced.
- Stream files instead of loading the full file into response memory.
- Reuse Julie's existing workspace safety model and spillover pattern.
- Keep this separate from normal source-code indexing.

## Non-Goals

- No automatic indexing of `*.log`, build outputs, or arbitrary text dumps.
- No command execution or shell pipeline replacement in v1.
- No persistent full-text index in v1.
- No semantic embeddings for log text in v1.
- No support for reading files outside the active or registered workspace.

## Tool Sketch

### `inspect_text_file`

Inputs:

- `path`: workspace-relative path, or absolute path under a registered workspace.
- `max_sample_bytes`: optional bounded sample size.

Output:

- file size, modified time, detected encoding.
- estimated line count.
- head/tail samples.
- detected shape: plain text, logfmt-like, JSONL-like, stack-trace-heavy.
- basic counts: error/warn/info/debug/fatal/panic/exception.
- timestamp range when detectable.

### `search_text_file`

Inputs:

- `path`
- `query` or `regex`
- `context_lines`
- `limit`
- optional filters: timestamp range, severity level.

Output:

- bounded match list with line numbers and small context windows.
- total matches scanned when cheap to count.
- `spillover_handle` when results exceed output cap.

### `slice_text_file`

Inputs:

- `path`
- `line_range` or match id from `search_text_file`
- `max_bytes`

Output:

- exact bounded text slice.
- clear truncation markers when caps apply.

## Architecture Boundaries

- New module: `src/tools/large_text/` or `src/tools/text_inspector/`.
- Tool registration stays alongside existing MCP tools, but implementation must not write to the source-code database or Tantivy index.
- Path resolution should reuse workspace routing and path safety helpers where possible.
- Large result pagination should reuse `src/tools/spillover/`.
- File reads should be streaming and blocking I/O should stay off the async executor.

## Safety Rules

- Reject paths outside the selected workspace.
- Reject binary files after a small sniff.
- Enforce hard output caps on every response.
- Enforce hard scan caps for very large files unless the caller opts into a full scan.
- Make truncation explicit.

## Open Questions

- Should v1 support compressed logs (`.gz`) or leave that for a later pass?
- Should repeated scans get an ephemeral cache keyed by `(path, mtime, size, hash_prefix)`?
- Should severity/timestamp parsing be best-effort generic only, or support known formats?
- Should this tool be exposed in CLI standalone mode as well as MCP?

## First Implementation Slice

1. Add `inspect_text_file` with streaming metadata, head/tail samples, severity counts, and workspace path validation.
2. Add targeted tests with synthetic large files and binary-file rejection.
3. Add `search_text_file` only after `inspect_text_file` proves the path and output contracts.

