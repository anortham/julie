# Tool-by-Tool Audit & Optimization

**Date**: 2026-02-06
**Status**: In Progress
**Goal**: Every MCP tool returns correct, complete, efficient results with good defaults.

## Principle

An agent should be able to use any tool with just the required params and get a useful result. If you need magic param combos to get good output, the defaults are wrong.

## Evaluation Criteria (per tool)

1. **Correctness** — Right data, no false positives, no false negatives
2. **Output efficiency** — No wasted tokens (strip hash IDs, byte offsets, redundant fields, absurd float precision)
3. **Default ergonomics** — Zero-config call does the right thing

## Process (per tool, no skipping)

1. **Understand** — Read implementation, trace code path from handler to output
2. **Exercise** — 3-5 realistic agent queries, vary params
3. **Evaluate** — Correct? Complete? Efficient? Good defaults?
4. **Fix** — Bug fixes, output format, default params
5. **Validate** — Re-run queries, `cargo test --lib`, commit
6. **Checkpoint** — Save what changed and what results look like now

## Priority Order (by agent impact)

### Tier 1: Core Workflow (used on almost every task)

| # | Tool | Issue | Severity |
|---|---|---|---|
| 1 | `fast_search` | Content mode default output is `symbols` — shows file:count with no context. Must pass `output="lines"` for useful results. | Output |
| 2 | `get_symbols` | Structure mode dumps 32-char hash IDs, repeats file_path per symbol, includes byte offsets. ~40% token waste. | Output |
| 3 | `fast_refs` | Only returns definitions/imports. Misses actual usages (calls, type annotations). Safety-critical for refactoring. | Broken |
| 4 | `fast_goto` | Returns imports and markdown docs as "definitions". Real definition buried in noise. | Output |

### Tier 2: Action Tools (used when making changes)

| # | Tool | Issue | Severity |
|---|---|---|---|
| 5 | `edit_symbol` | Dry run shows `old_size/new_size` bytes, no actual diff. Generic next_actions. | Output |
| 6 | `rename_symbol` | Dry run says "1 file, 1 change" but doesn't list locations. | Output |
| 7 | `edit_lines` | Dry run shows line count change, no diff preview. | Output |
| 8 | `fuzzy_replace` | Can't match exact strings at threshold 0.8. Broken matching. | Broken |

### Tier 3: Exploration (deeper investigation)

| # | Tool | Issue | Severity |
|---|---|---|---|
| 9 | `fast_explore(logic)` | Works well. Confidence floats have 16 decimal places. | Minor |
| 10 | `fast_explore(types)` | Correct data but full JSON dump with byte offsets, hash IDs, code_context. Token-heavy. | Output |
| 11 | `fast_explore(dependencies)` | Returns 0 dependencies for well-connected structs. | Broken |
| 12 | `trace_call_path` | Cross-language name matching creates false call paths (Rust `new` → Ruby `new`). | Broken |

### Tier 4: Support (already working)

| # | Tool | Status |
|---|---|---|
| 13 | `manage_workspace` | Fixed this session |
| 14 | `checkpoint/recall/plan` | Working well |

## Progress Tracker

- [ ] Tool 1: `fast_search`
- [ ] Tool 2: `get_symbols`
- [ ] Tool 3: `fast_refs`
- [ ] Tool 4: `fast_goto`
- [ ] Tool 5: `edit_symbol`
- [ ] Tool 6: `rename_symbol`
- [ ] Tool 7: `edit_lines`
- [ ] Tool 8: `fuzzy_replace`
- [ ] Tool 9: `fast_explore(logic)`
- [ ] Tool 10: `fast_explore(types)`
- [ ] Tool 11: `fast_explore(dependencies)`
- [ ] Tool 12: `trace_call_path`
- [x] Tool 13: `manage_workspace`
- [x] Tool 14: `checkpoint/recall/plan`
