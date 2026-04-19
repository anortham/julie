# call_path and rewrite_symbol Hardening Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use razorback:team-driven-development (on Claude Code) or razorback:subagent-driven-development (elsewhere) to implement this plan. Fall back to razorback:executing-plans for single-task or tightly-sequential plans.

**Goal:** Fix the real bugs surfaced by dogfooding `call_path` and `rewrite_symbol`, close the cross-language gaps, and make both tools safe to use across Julie's 34 supported languages — not just Rust.

**Architecture:** Close the gaps in the v1 "AST-backed rewrite" design (see `docs/plans/2026-04-17-agent-tool-surface-design.md`) without adding new guardrail axes. For `rewrite_symbol`: reject unsupported operations explicitly instead of silently clobbering (completes the original Task 2 acceptance criterion "unsupported operations fail with narrow, specific errors"), centralize no-op detection, surface the replaced span in dry-run so callers can see what they're about to overwrite, and document the per-operation grammar contract. For `call_path`: split disambiguation into per-endpoint file-path params with suffix-aware matching, and clean up the edge-label code to match what BFS actually traverses.

**Tech Stack:** Rust, tree-sitter, tokio, anyhow, serde, rmcp. Existing helpers: `find_symbol`, `ExtractorManager`, `format_unified_diff`, `compare_symbols_by_priority_and_context`.

---

## Scope Note

All the issues below were validated by (a) dogfood testing in a live MCP session, (b) reading the source, (c) a Codex second opinion, and (d) cross-checking against the original design at `docs/plans/2026-04-17-agent-tool-surface-design.md`. Several findings were caught by Codex and missed in the initial dogfood pass — in particular, the `replace_signature` full-symbol clobber and the no-op detection being shared across all rewrite operations, not just `replace_signature`. The plan deliberately does NOT add post-edit tree-sitter round-trip validation; that would be feature creep beyond the original "narrow tool, one job" design. The `replace_signature` clobber fix completes an acceptance criterion that was already in the original plan ("unsupported operations fail with narrow, specific errors") but wasn't honored in the v1 implementation.

## File Structure

**Modified:**
- `src/tools/editing/rewrite_symbol.rs` — span_for_operation, call_tool, RewriteSymbolTool docstrings, dry-run output format
- `src/tools/editing/validation.rs` — add `is_no_op(before, after)` helper (or similar) if needed; maybe expose to rewrite_symbol
- `src/tools/navigation/call_path.rs` — CallPathTool schema, resolve_unique_symbol, resolve_endpoints, edge_label

**Created (tests):**
- `src/tests/tools/editing/rewrite_symbol_cross_language_tests.rs` — Python/Java/Ruby/Go coverage for the three body-affecting ops
- `src/tests/tools/call_path_disambiguation_tests.rs` — from_file_path / to_file_path, duplicate names, trait-impl qualified names

**Modified (tests):**
- `src/tests/tools/editing/rewrite_symbol_tests.rs` — add replace_signature-clobber guard test, no-op detection test, dry-run span-preview test
- `src/tests/tools/call_path_tests.rs` — add edge-label exhaustiveness test, multi-segment qualified name test

---

## Task 1: rewrite_symbol — explicit errors for unsupported ops

**Files:**
- Modify: `src/tools/editing/rewrite_symbol.rs:263-328` (span_for_operation)
- Test: `src/tests/tools/editing/rewrite_symbol_tests.rs` (add new tests)

**What to build:** Two safety fixes in `span_for_operation`:
1. `replace_signature` currently falls back to `Ok(Some(full_range))` when the node has no `body` field (line 321-323). Silent full-symbol clobber. Replace with an explicit error: `"replace_signature is not supported for symbol '{name}' (kind: {kind}); it has no body-delimited signature in the {language} grammar."`
2. `replace_body` error when `body` field is missing (line 288-294) should list the **node's actual field names** via `cursor_for_fields` / manual iteration over tree-sitter node fields, not the grammar-wide catalog. Message: `"Operation 'replace_body' is not supported for '{name}' ({kind}); node has fields: [{actual_fields}] but no 'body' field."`

**Approach:**
- For (1), keep the `node.child_by_field_name("body")` check but change the `else` branch to return an `Err` instead of `Ok(Some(full_range))`.
- For (2), use `node.walk()` and iterate `TreeCursor::field_name()` across children to collect the set of field names the node actually has. If no fields, report `"no named fields"`.

**Acceptance criteria:**
- [ ] `replace_signature` on a Rust `trait` method declaration (no body) returns an explicit error, not a full-symbol replacement. Regression test asserts the file is unchanged.
- [ ] `replace_body` on a Rust trait method declaration returns an error that lists the available field names on that node.
- [ ] All existing rewrite_symbol tests still pass.
- [ ] Cargo fmt + clippy clean.
- [ ] Tests committed together with the implementation.

---

## Task 2: rewrite_symbol — centralized no-op detection

**Files:**
- Modify: `src/tools/editing/rewrite_symbol.rs:460-519` (call_tool main body)
- Modify or create helper in `src/tools/editing/validation.rs` if useful
- Test: `src/tests/tools/editing/rewrite_symbol_tests.rs`

**What to build:** After `modified_content` is computed and before the dry-run/commit branch, detect `modified_content == original_content` and return an informational result instead of an empty diff. Apply to all rewrite operations, not just `replace_signature`. Messaging: `"No changes: {operation} with supplied content would not modify the file. Symbol '{name}' at {path}:{start_line}-{end_line} is already in the requested state."`

**Approach:**
- Insert a single check right after the `match self.operation.as_str() { ... }` block produces `modified_content`.
- Skip the balance warning, diff formatting, and transaction commit on no-op.
- Return the informational message in both dry_run and non-dry_run modes.

**Acceptance criteria:**
- [ ] `replace_signature` with identical content → no-op message, not empty diff.
- [ ] `replace_body`, `replace_full`, `add_doc`, `insert_before`, `insert_after` with content that would produce no change → no-op message.
- [ ] Non-dry-run no-op does not begin an `EditingTransaction` (no filesystem writes, no mtime change).
- [ ] Tests cover at least two operations for no-op detection.
- [ ] Tests committed together.

---

## Task 3: rewrite_symbol — show replaced span in dry-run output

**Files:**
- Modify: `src/tools/editing/rewrite_symbol.rs` — augment dry-run output to include the span being replaced
- Test: `src/tests/tools/editing/rewrite_symbol_tests.rs`

**What to build:** Close the "caller is blind to the span" footgun that led to the `replace_body`-without-braces silent corruption. Before the diff, show the caller what's about to be replaced: byte range, line range, and the old content itself (truncated if long).

This keeps the tool's job narrow (per original design: "each tool has one job") and matches the "compact diff preview" shape already in place. No new validation axis, no parse-after-edit step — just give the caller the information they need to realize "oh, the braces are part of the span I'm replacing."

**Approach:**
- For operations that replace a span (`replace_full`, `replace_body`, `replace_signature`): once `span_for_operation` returns the `ByteRange`, capture `original_content[range.start..range.end]` and the corresponding line range.
- For insert operations (`insert_before`, `insert_after`, `add_doc`): report the anchor byte/line where the insert happens. No "old content" to show.
- In dry-run output, prepend a short header before the diff:
  ```
  Replacing 142 chars at bytes 1234..1376 (lines 76-83) in src/foo.rs
  --- Old content ---
  fn relationship_priority(kind: &RelationshipKind) -> u8 {
      match kind {
          RelationshipKind::Calls => 0,
          ...
      }
  }
  --- Diff ---
  @@ -76,8 +76,8 @@
   ...
  ```
- If old content is longer than ~30 lines, show the first 15 and last 5 with a `... N lines elided ...` marker. This keeps the preview compact.
- Non-dry-run (apply) output keeps the existing compact summary — no behavior change there.

**Acceptance criteria:**
- [ ] Dry-run output for `replace_body`, `replace_full`, `replace_signature` includes the byte range, line range, and old content (or an elided form for long bodies).
- [ ] Dry-run output for `insert_before`, `insert_after`, `add_doc` reports the anchor position without an "old content" section.
- [ ] Non-dry-run output is unchanged.
- [ ] Tests: (a) dry-run preview for `replace_body` on a Rust function shows the braces in the old content, (b) dry-run preview for `add_doc` shows the anchor line but no old content, (c) long-body elision works as spec'd.
- [ ] Tests committed together.

---

## Task 4: rewrite_symbol — operation docstring clarity

**Files:**
- Modify: `src/tools/editing/rewrite_symbol.rs:32-58` (RewriteSymbolTool struct and its fields' docstrings)

**What to build:** Replace the single-line `operation` docstring with per-operation semantics that are honest about grammar dependence. New docstring:

```
/// Operation to perform. All operations target the symbol's span as extracted from the
/// language's tree-sitter grammar.
///
/// - replace_full: Replace the entire symbol span (signature + body if any).
/// - replace_body: Replace the grammar's `body` field. For brace-delimited languages
///   (Rust, C, Java, Go, JS/TS, C#, Swift, Kotlin, Scala, PHP, etc.) the replaced
///   span INCLUDES the enclosing braces, so your `content` must supply the full
///   `{ ... }` block. For indentation-delimited languages (Python) the replaced
///   span is the indented suite. For declarations without a body (trait methods,
///   interface methods, forward declarations) this operation returns an error.
/// - replace_signature: Replace the text up to the start of the body field. Returns
///   an error if the symbol has no body field.
/// - insert_after / insert_before: Insert content on the line after/before the symbol.
/// - add_doc: Insert a documentation comment before the symbol. Errors if the symbol
///   already has documentation.
```

**Acceptance criteria:**
- [ ] Struct docstrings accurately describe observable behavior for both brace-delimited and indent-delimited languages.
- [ ] Docstring changes are reflected in the JSON schema produced by `schemars` (verify by building and inspecting the tool's schema output).

---

## Task 5: call_path — per-endpoint file-path disambiguation

**Files:**
- Modify: `src/tools/navigation/call_path.rs:27-135` (CallPathTool struct, resolve_unique_symbol, resolve_endpoints)
- Test: `src/tests/tools/call_path_disambiguation_tests.rs` (create)

**What to build:** Add two optional params to `CallPathTool`:
```rust
pub from_file_path: Option<String>,
pub to_file_path: Option<String>,
```
Plumb each through `resolve_endpoints` → `resolve_unique_symbol(db, name, role, file_path: Option<&str>)`. Inside `resolve_unique_symbol`:
1. Pass the `file_path` into `find_symbol` (it already accepts one).
2. After retrieving matches, if the filter was provided, apply a **suffix-aware** filter: `match.file_path == filter || match.file_path.ends_with(&format!("/{}", filter))`. Do NOT use `.contains()` — that produces false positives (`handler.rs` would match `tools/handler.rs`).

Update the ambiguity error to mention which param to set (`from_file_path` or `to_file_path` depending on role).

**Approach:**
- Look at `compare_symbols_by_priority_and_context` at `src/tools/navigation/resolution.rs:175-184` for the suffix-aware matching pattern; extract into a reusable helper: `fn file_path_matches_suffix(path: &str, query: &str) -> bool`.
- Also consider using this helper inside `rewrite_symbol.rs:383-390`, which today uses `.contains()` — fix that while here.

**Acceptance criteria:**
- [ ] `call_path(from=..., to=..., from_file_path="src/handler.rs")` resolves ambiguous `from` name to the handler.rs match.
- [ ] Suffix filter rejects `"handler.rs"` matching `"tools/something/handler.rs"` only when the full segment doesn't match — i.e. `src/tools/handler.rs` ends with `/handler.rs` ✓, but `src/foohandler.rs` does not ✓.
- [ ] Ambiguity error message names the correct param (`from_file_path` vs `to_file_path`) per role.
- [ ] Tests cover: (a) successful disambiguation with from_file_path, (b) successful disambiguation with to_file_path, (c) both simultaneously, (d) substring false-positive rejection, (e) still-ambiguous-after-filter case.
- [ ] rewrite_symbol's `.contains()` filter at line 383-390 updated to suffix-aware matching via the shared helper.
- [ ] Tests committed together.

---

## Task 6: call_path — edge_label cleanup

**Files:**
- Modify: `src/tools/navigation/call_path.rs:85-94` (edge_label) and `76-83` (relationship_priority)
- Test: `src/tests/tools/call_path_tests.rs`

**What to build:** Clean up two functions that contain dead code given the BFS filter at line 161-168 (which keeps only `Calls`, `Instantiates`, `Overrides`).

1. `edge_label`: Remove `Extends | Implements` branch (never reached — BFS filters them out). Remove `_ => "reference"` fallback (never reached). Make the match exhaustive over the three traversed kinds, with `debug_assert!(unreachable)` or `panic!` for the fallthrough.
2. `relationship_priority`: Same treatment — the `_ => u8::MAX` arm is dead. Make exhaustive or panic-on-unreachable.
3. Add a docstring to the module or to `CallPathTool` clarifying: `"BFS traverses Calls, Instantiates, and Overrides relationships only. Extends/Implements/TypeUsage/Reference edges are not followed."` — this matches the existing tool description but makes it internally consistent.

**Acceptance criteria:**
- [ ] Match statements are exhaustive over traversed kinds (or explicit `unreachable!` for the filtered-out kinds).
- [ ] New test: `test_edge_label_exhaustive_over_traversed_kinds` asserts the labels for Calls → "call", Instantiates → "construct", Overrides → "dispatch".
- [ ] Existing call_path tests still pass.
- [ ] Tests committed together.

---

## Task 7: Cross-language tests for rewrite_symbol

**Files:**
- Create: `src/tests/tools/editing/rewrite_symbol_cross_language_tests.rs`
- Register test module: `src/tests/tools/editing/mod.rs`

**What to build:** End-to-end tests for rewrite_symbol on multiple grammar families. Each test runs the tool against a synthetic in-workspace file and verifies the post-edit buffer. Cover at minimum:

| Language | Test cases |
|---|---|
| Python | `replace_body` on a `def` with indented suite (pass); `replace_signature` on a `def` (pass). |
| Java | `replace_body` on a method (brace-delimited, pass); `replace_signature` on an interface method declaration (no body) → expects explicit error from Task 1. |
| Ruby | `replace_body` on a `def ... end` method (pass); `replace_signature` on a `def` (pass). |
| Go | `replace_signature` on a func declaration (pass); `replace_body` on a method (pass). |
| Rust | `replace_signature` on a trait method declaration (no body) → expects explicit error from Task 1; regression test asserts the file is unchanged on that error. |

**Approach:**
- Use the existing test fixture setup in `rewrite_symbol_tests.rs` (likely a temp workspace builder, see line 83 onward for pattern). Extend it to accept different source files per test.
- Each test creates a small source file in the target language, runs `manage_workspace(operation="index")` on the temp workspace, then invokes the tool.
- Assert on both: the returned `CallToolResult` content (error or success message) and the resulting file content (unchanged on error, correctly mutated on success).
- Integrate with Task 3: for at least one brace-delimited `replace_body` case, assert that the dry-run preview includes the old-content section showing the enclosing braces.

**Acceptance criteria:**
- [ ] Five languages covered (Python, Java, Ruby, Go, Rust) with the cases above.
- [ ] Tests that exercise Task 1's explicit-error behavior (declaration-without-body cases in Java and Rust) assert the file is unchanged and the error message names the operation and symbol.
- [ ] At least one test asserts the dry-run preview shows the replaced span's old content (Task 3 integration).
- [ ] Tests run with `cargo nextest run --lib rewrite_symbol_cross_language`.
- [ ] Tests run fast (< 5s combined).
- [ ] Tests committed together.

---

## Task 8: Cross-language tests for call_path

**Files:**
- Create: `src/tests/tools/call_path_disambiguation_tests.rs`
- Register test module: `src/tests/tools/mod.rs`

**What to build:** Tests covering the per-endpoint disambiguation params and the qualified-name trait-impl case.

Cases:
1. Duplicate symbol name in two files (e.g., two `process` functions in `src/a.rs` and `src/b.rs`), path from one specific file resolves correctly via `from_file_path`.
2. Cross-file path: `from` in `a.rs`, `to` in `b.rs`, both disambiguated by their respective file-path params.
3. Substring false-positive: `from_file_path="handler.rs"` when only `tools/foohandler.rs` exists → resolution fails with a clear error (not a false match).
4. Multi-segment qualified name that Julie currently supports (e.g., `MyStruct::my_method` where both parent and child are indexed). Pass case.
5. Trait-impl qualified name (currently broken case surfaced in dogfood): `JulieServerHandler::call_tool` where `call_tool` is defined in `impl Trait for Struct`. Expected behavior: document current limitation with a test that either (a) passes if extractor already links parent_id correctly, or (b) fails with a clear error message rather than silently "not found". If this turns out to be an extractor fix, file a follow-up issue and keep the test as `#[ignore]` with a clear reason.

**Acceptance criteria:**
- [ ] All five cases covered.
- [ ] For case 5 specifically: the tool returns a helpful error, not a bare "not found", OR the extractor is updated to populate parent_id from trait impls (scope the extractor change to this task only if it is a one-line change; otherwise file a follow-up and `#[ignore]` the test).
- [ ] Tests committed together.

---

## Sequencing and Dependencies

- **Parallel bucket A** (rewrite_symbol, all same file — sequential per-teammate within the bucket):
  - Task 1 → Task 2 → Task 3 → Task 4 → Task 7
- **Parallel bucket B** (call_path, all same file — sequential per-teammate within the bucket):
  - Task 5 → Task 6 → Task 8

Bucket A and bucket B are independent (different files). Assign each bucket to a separate teammate. Do NOT split a bucket across teammates (Tasks 1, 2, 3 all edit the same function in `rewrite_symbol.rs`; Tasks 5 and 6 both edit `call_path.rs`).

If a third teammate is available, they can pick up Task 7 and Task 8 once the corresponding implementation tasks land.

## Test Strategy

- During RED/GREEN loops: `cargo nextest run --lib <specific_test_name>`.
- After each task batch completes in a teammate: the teammate runs only the narrow test(s) they wrote (subagent rule).
- Lead runs `cargo xtask test changed` once the batch lands, escalating to `cargo xtask test dev` if `changed` falls back to `dev`.
- Final pre-merge: `cargo xtask test full` (covers dev + system + dogfood).

## Non-goals

- No changes to the tree-sitter grammars themselves.
- No new rewrite operations (e.g., `replace_body_contents` as a separate op). Keep v1 scoped to fixing the current operations' contracts. If we later want body-interior semantics, that is a new plan.
- No changes to `find_symbol` semantics. The trait-impl limitation for qualified names is an extractor-level question, in scope only if it turns out to be trivial; otherwise it's a follow-up.
- No changes to how `call_path` BFS filters relationships. The filter set stays `Calls | Instantiates | Overrides`.

## Risks

- **Old-content preview (Task 3) adds output size.** A 500-line `replace_full` on a 500-line symbol produces a big dry-run. Mitigation: the elision rule (first 15 + last 5 lines for content over ~30 lines) caps the output. Verify in test.
- **Per-endpoint file_path params in `call_path` (Task 5)** change the tool schema. Existing MCP clients use named args, so this should be backward-compatible, but a schema dump comparison before/after would confirm.
- **The `replace_signature` explicit-error change (Task 1)** is technically a behavior change for any caller that was relying on the full-symbol fallback. The fallback was never documented and produced data loss, so strictly an improvement, but worth calling out in release notes.
